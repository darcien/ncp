//! The `auth` commands: OAuth login, logout and status (design section 5).
//!
//! rmcp does the protocol steps. This module chooses the client, runs the
//! redirect, and keeps the items in the place that the user chose.

use std::{io::IsTerminal, sync::Arc};

use rmcp::transport::auth::{
    AuthClient, AuthError, AuthorizationManager, AuthorizationMetadata, AuthorizationRequest,
    AuthorizationSession, OAuthClientConfig,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
};

use super::{
    args::{AuthCommand, CredentialPlace},
    diagnostics,
    outcome::{Exit, Failure},
    secrets,
    session::{self, Context},
};
use crate::{
    client::RequestError,
    config::{RemoteServer, Server, Transport},
    credential::CredentialKey,
    oauth::{
        callback::{self, RedirectUri},
        http::OAuthHttp,
        items::{self, ClientItem},
        store::{TokenStore, parse_tokens},
    },
    store::CredentialStore,
};

/// Opens a URL in the browser of the user. Tests give a fake.
pub type OpenBrowser = fn(&str) -> std::io::Result<()>;

/// The production browser: `open` on macOS, `xdg-open` on Linux. The URL
/// holds the state and the PKCE challenge, which are not secret, so it can
/// be a process argument.
///
/// # Errors
///
/// Returns an error when the program cannot start.
pub fn open_browser(url: &str) -> std::io::Result<()> {
    let program = if cfg!(target_os = "macos") {
        "/usr/bin/open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
}

/// The client name in a registration.
const CLIENT_NAME: &str = "ncp";

pub(super) async fn command(context: &Context<'_>, command: AuthCommand) -> Result<(), Failure> {
    match command {
        AuthCommand::Login {
            server,
            no_browser,
            credential_store,
        } => login(context, &server, no_browser, credential_store).await,
        AuthCommand::Logout { server } => logout(context, &server).await,
        AuthCommand::Status { server } => status(context, server.as_deref()).await,
    }
}

/// The HTTP server of the name. OAuth is only for HTTP servers.
fn remote(server: &Server) -> Result<&RemoteServer, Failure> {
    match &server.transport {
        Transport::Http(remote) => Ok(remote),
        _ => Err(Failure::usage(
            "OAuth is only for http servers; a stdio server gets its credentials through env",
        )
        .server(server.name.as_str())),
    }
}

/// The failure of an OAuth step. The authorization server or the network
/// failed: exit 4 for a refused login, exit 3 for the rest.
fn auth_failure(step: &str, error: &AuthError) -> Failure {
    let exit = match error {
        AuthError::AuthorizationRequired
        | AuthError::AuthorizationFailed(_)
        | AuthError::TokenExchangeFailed(_)
        | AuthError::TokenRefreshRejected(_)
        | AuthError::AuthorizationServerMismatch { .. }
        | AuthError::AuthorizationServerMissingIssuer { .. } => Exit::Auth,
        _ => Exit::Protocol,
    };
    Failure::new(exit, format!("{step}: {error}"))
}

/// The endpoints of the authorization server must use HTTPS, except on a
/// loopback host (MCP 2025-11-25, basic/authorization, "Communication
/// Security"). rmcp does not check this.
fn check_https(metadata: &AuthorizationMetadata) -> Result<(), Failure> {
    let endpoints = [
        Some(&metadata.authorization_endpoint),
        Some(&metadata.token_endpoint),
        metadata.registration_endpoint.as_ref(),
    ];
    endpoints
        .into_iter()
        .flatten()
        .find(|endpoint| !is_secure(endpoint))
        .map_or(Ok(()), |endpoint| {
            Err(Failure::protocol(format!(
                "the authorization server endpoint {endpoint} does not use HTTPS"
            )))
        })
}

fn is_secure(endpoint: &str) -> bool {
    let Ok(url) = url::Url::parse(endpoint) else {
        return false;
    };
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        None => false,
    };
    url.scheme() == "https" || (url.scheme() == "http" && loopback)
}

/// The issuer of the metadata. rmcp requires it when discovery can derive
/// it; else the origin of the authorization endpoint names the server.
fn issuer_of(metadata: &AuthorizationMetadata) -> String {
    metadata.issuer.clone().unwrap_or_else(|| {
        url::Url::parse(&metadata.authorization_endpoint).map_or_else(
            |_| metadata.authorization_endpoint.clone(),
            |url| url.origin().ascii_serialization(),
        )
    })
}

/// The store of the place. The native store must answer before the login
/// starts, so that the user does not log in for nothing.
async fn place_store(
    context: &Context<'_>,
    server: &str,
    place: CredentialPlace,
) -> Result<Arc<dyn CredentialStore>, Failure> {
    match place {
        CredentialPlace::Native => secrets::native(context).await.map_err(|error| {
            secrets::store_failure(&error)
                .hint(format!("ncp auth login {server} --credential-store file"))
        }),
        CredentialPlace::File => Ok(secrets::file(context)),
    }
}

async fn login(
    context: &Context<'_>,
    word: &str,
    no_browser: bool,
    place: CredentialPlace,
) -> Result<(), Failure> {
    let loaded = session::load_config(&context.paths.config_file, context.env)?;
    let server = session::select(&loaded, word, context.ui)?;
    let name = server.name.as_str();
    let remote = remote(server)?;
    let store = place_store(context, name, place).await?;
    let observer = diagnostics::observer(context.ui, name, context.timings.clone(), |url| {
        crate::redact::url(url)
    });
    let http = OAuthHttp::new(context.env, &observer)
        .map_err(|error| Failure::protocol(error.to_string()).server(name))?;
    let mut manager =
        AuthorizationManager::new_with_oauth_http_client(remote.url.as_str(), Arc::new(http))
            .await
            .map_err(|error| auth_failure("OAuth setup", &error).server(name))?;
    let metadata = manager
        .resolve_metadata()
        .await
        .map_err(|error| auth_failure("OAuth discovery", &error).server(name))?
        .metadata;
    check_https(&metadata).map_err(|failure| failure.server(name))?;
    let issuer = issuer_of(&metadata);
    context
        .ui
        .debug(Some(name), &format!("authorization server: {issuer}"));
    manager.set_metadata(metadata);
    manager.set_credential_store(TokenStore::new(server.name.clone(), store.clone()));
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|error| Failure::protocol(format!("cannot listen for the redirect: {error}")))?;
    let port = listener
        .local_addr()
        .map_err(|error| Failure::protocol(format!("cannot listen for the redirect: {error}")))?
        .port();
    let redirect = RedirectUri::new(port);
    let request = client_request(
        context,
        server,
        remote,
        &mut manager,
        &store,
        &issuer,
        &redirect,
    )
    .await
    .map_err(|failure| failure.server(name))?;
    let session = AuthorizationSession::new(manager, request)
        .await
        .map_err(|(_, error)| auth_failure("OAuth login", &error).server(name))?;
    let url = session.get_authorization_url().to_string();
    let state = state_of(&url).ok_or_else(|| {
        Failure::protocol("the authorization URL of rmcp has no state").server(name)
    })?;
    let read_stdin = no_browser || std::io::stdin().is_terminal();
    show_url(context, name, &url, read_stdin);
    if !no_browser && let Err(error) = (context.open_browser)(&url) {
        context
            .ui
            .warning(Some(name), &format!("cannot open a browser: {error}"));
    }
    let callback = wait_for_redirect(context, listener, &redirect, &state, read_stdin)
        .await
        .map_err(|failure| failure.server(name))?;
    session
        .handle_callback_with_issuer(&callback.code, &callback.state, callback.iss.as_deref())
        .await
        .map_err(|error| match error {
            // For example a token set that is too long for the Keychain
            // through `security`.
            AuthError::CredentialStoreError(_) if place == CredentialPlace::Native => {
                Failure::new(Exit::Auth, format!("cannot keep the tokens: {error}"))
                    .hint(format!("ncp auth login {name} --credential-store file"))
                    .server(name)
            }
            error => auth_failure("OAuth token request", &error).server(name),
        })?;
    let granted = session.auth_manager.get_current_scopes().await;
    context.ui.result(
        || {
            format!(
                "Logged in to {name} at {issuer}. The tokens are in {}.\n",
                secrets::place_text(place)
            )
        },
        || {
            json!({"server": name, "issuer": issuer, "scopes": granted,
                "place": secrets::place_name(place)})
        },
    )
}

/// The OAuth client with the stored tokens of the server, or `None` when
/// no place has tokens, or when the tokens are for another issuer. `ncp`
/// calls this only after the server answered `401`, so that a server with
/// no login never opens the native store (design C1).
///
/// # Errors
///
/// Returns a failure when a store cannot answer or when discovery fails.
pub(super) async fn stored_login(
    context: &Context<'_>,
    server: &Server,
    remote: &RemoteServer,
    http: &reqwest::Client,
    observer: &crate::client::observe::Observer,
) -> Result<Option<crate::client::http::Authorized>, Failure> {
    let name = server.name.as_str();
    let key = CredentialKey::OAuthTokens(server.name.clone());
    let native = secrets::native(context).await.ok();
    let file = secrets::file(context);
    let mut found = None;
    for store in native.into_iter().chain([file]) {
        let wanted = key.clone();
        let secret = secrets::call(context, &store, move |store| store.load(&wanted))
            .await
            .map_err(|error| secrets::store_failure(&error).server(name))?;
        if let Some(secret) = secret {
            found = Some((store, secret));
            break;
        }
    }
    let Some((store, secret)) = found else {
        return Ok(None);
    };
    let tokens =
        parse_tokens(&secret).map_err(|error| Failure::protocol(error.to_string()).server(name))?;
    let client_id = tokens.client_id.clone();
    context.ui.debug(Some(name), "read the stored OAuth tokens");
    let oauth_http = OAuthHttp::new(context.env, observer)
        .map_err(|error| Failure::protocol(error.to_string()).server(name))?;
    let mut manager =
        AuthorizationManager::new_with_oauth_http_client(remote.url.as_str(), Arc::new(oauth_http))
            .await
            .map_err(|error| auth_failure("OAuth setup", &error).server(name))?;
    manager.set_credential_store(
        TokenStore::new(server.name.clone(), store.clone()).with_tokens(tokens),
    );
    let usable = manager
        .initialize_from_store()
        .await
        .map_err(|error| auth_failure("OAuth discovery", &error).server(name))?;
    if !usable {
        return Ok(None);
    }
    // rmcp configures only the client ID from the tokens. A refresh of a
    // confidential client needs the secret of the client item too.
    let client_key = CredentialKey::OAuthClient(server.name.clone());
    let client = secrets::call(context, &store, move |store| store.load(&client_key))
        .await
        .map_err(|error| secrets::store_failure(&error).server(name))?
        .and_then(|secret| ClientItem::from_secret(&secret).ok());
    if let Some(ClientItem {
        client_id: stored_id,
        client_secret: Some(client_secret),
        ..
    }) = client
        && stored_id == client_id
    {
        let config = OAuthClientConfig::new(client_id, RedirectUri::new(0).to_string())
            .with_client_secret(client_secret);
        manager
            .configure_client(config)
            .map_err(|error| auth_failure("OAuth setup", &error).server(name))?;
    }
    Ok(Some(AuthClient::new(
        crate::client::http::observed(http, observer),
        manager,
    )))
}

/// The field of the tokens item that keeps the scopes of a `403
/// insufficient_scope` answer until the next login. rmcp ignores it.
const REQUIRED_SCOPES: &str = "ncp_required_scopes";

/// Keeps the scopes that the server asked for in a `403` answer in the
/// tokens item, so that the next `auth login` asks for them with the
/// scopes that the user has (requirement A9). A failure gives a warning:
/// the error of the call is the result.
pub(super) async fn remember_scopes(context: &Context<'_>, server: &Server, error: &RequestError) {
    let Some(scopes) = error.required_scopes().filter(|scopes| !scopes.is_empty()) else {
        return;
    };
    let name = server.name.as_str();
    if let Err(failure) = add_required_scopes(context, server, &scopes).await {
        context.ui.warning(
            Some(name),
            &format!(
                "cannot keep the scopes for the next login: {}",
                failure.message
            ),
        );
    }
}

async fn add_required_scopes(
    context: &Context<'_>,
    server: &Server,
    scopes: &[String],
) -> Result<(), Failure> {
    let key = CredentialKey::OAuthTokens(server.name.clone());
    let native = secrets::native(context).await.ok();
    let file = secrets::file(context);
    for store in native.into_iter().chain([file]) {
        let wanted = key.clone();
        let Some(secret) = secrets::call(context, &store, move |store| store.load(&wanted))
            .await
            .map_err(|error| secrets::store_failure(&error))?
        else {
            continue;
        };
        let mut tokens: Value = serde_json::from_str(secret.expose()).map_err(|error| {
            Failure::protocol(format!("the stored tokens are not JSON: {error}"))
        })?;
        let required = union(&scope_list(&tokens[REQUIRED_SCOPES]), scopes);
        tokens[REQUIRED_SCOPES] = json!(required);
        let secret = items::to_secret("tokens", &tokens)
            .map_err(|error| Failure::protocol(error.to_string()))?;
        let saved = key.clone();
        secrets::call(context, &store, move |store| store.save(&saved, &secret))
            .await
            .map_err(|error| secrets::store_failure(&error))?;
        context.ui.debug(
            Some(server.name.as_str()),
            &format!("kept the scopes {} for the next login", required.join(" ")),
        );
        return Ok(());
    }
    Ok(())
}

fn scope_list(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|scopes| {
            scopes
                .iter()
                .filter_map(|scope| scope.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The scopes of both lists, each one once, in the order of the lists.
fn union(first: &[String], second: &[String]) -> Vec<String> {
    first
        .iter()
        .chain(second)
        .fold(Vec::new(), |mut all, scope| {
            if !all.contains(scope) {
                all.push(scope.clone());
            }
            all
        })
}

/// The scopes for a new login: the scopes that rmcp selects, the scopes
/// that the user has, and the scopes of a `403` answer since the last
/// login. A step-up asks for the old and the new scopes (requirement A9).
async fn login_scopes(
    context: &Context<'_>,
    server: &Server,
    store: &Arc<dyn CredentialStore>,
    selected: &[String],
) -> Vec<String> {
    let key = CredentialKey::OAuthTokens(server.name.clone());
    let stored = secrets::call(context, store, move |store| store.load(&key))
        .await
        .ok()
        .flatten()
        .and_then(|secret| serde_json::from_str::<Value>(secret.expose()).ok())
        .unwrap_or(Value::Null);
    let held = union(
        &scope_list(&stored["granted_scopes"]),
        &scope_list(&stored[REQUIRED_SCOPES]),
    );
    union(selected, &held)
}

/// The client for the login, in the order of the spec (requirement A6):
/// the `oauth.clientId` of the configuration, then the stored client of
/// the same issuer, then a new registration. A Client ID Metadata Document
/// waits for a release decision (requirement A7).
async fn client_request(
    context: &Context<'_>,
    server: &Server,
    remote: &RemoteServer,
    manager: &mut AuthorizationManager,
    store: &Arc<dyn CredentialStore>,
    issuer: &str,
    redirect: &RedirectUri,
) -> Result<AuthorizationRequest, Failure> {
    let selected = manager.select_scopes(None, &[]);
    let scopes = login_scopes(context, server, store, &selected).await;
    let request = AuthorizationRequest::new(redirect.to_string())
        .with_client_name(CLIENT_NAME)
        .with_scopes(scopes.clone());
    if let Some(client_id) = &remote.oauth_client_id {
        return Ok(request.with_preregistered_client(client_id.clone()));
    }
    let key = CredentialKey::OAuthClient(server.name.clone());
    let stored = secrets::call(context, store, {
        let key = key.clone();
        move |store| store.load(&key)
    })
    .await
    .map_err(|error| secrets::store_failure(&error))?
    .map(|secret| ClientItem::from_secret(&secret))
    .transpose()
    .map_err(|error| Failure::protocol(error.to_string()))?;
    let client = if let Some(item) = stored.filter(|item| item.is_for(issuer)) {
        item
    } else {
        let item = register(manager, issuer, redirect, &scopes).await?;
        let secret = item
            .to_secret()
            .map_err(|error| Failure::protocol(error.to_string()))?;
        secrets::call(context, store, move |store| store.save(&key, &secret))
            .await
            .map_err(|error| secrets::store_failure(&error))?;
        context.ui.debug(
            Some(server.name.as_str()),
            &format!("registered the client {} at {issuer}", item.client_id),
        );
        item
    };
    let request = request.with_preregistered_client(client.client_id);
    Ok(match client.client_secret {
        Some(secret) => request.with_client_secret(secret),
        None => request,
    })
}

/// A new client by Dynamic Client Registration (RFC 7591), with the
/// scopes that the login asks for.
async fn register(
    manager: &mut AuthorizationManager,
    issuer: &str,
    redirect: &RedirectUri,
    scopes: &[String],
) -> Result<ClientItem, Failure> {
    let scope_refs: Vec<&str> = scopes.iter().map(String::as_str).collect();
    let config = manager
        .register_client(CLIENT_NAME, &redirect.to_string(), &scope_refs)
        .await
        .map_err(|error| auth_failure("OAuth client registration", &error))?;
    Ok(ClientItem {
        issuer: issuer.to_string(),
        client_id: config.client_id,
        client_secret: config.client_secret,
    })
}

fn state_of(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .query_pairs()
        .find(|(name, _)| name == "state")
        .map(|(_, value)| value.into_owned())
}

/// Shows the URL. Text: lines for a person. JSON: `{"login": {"server",
/// "url"}}`, so that an agent can give the URL to a person.
fn show_url(context: &Context<'_>, server: &str, url: &str, read_stdin: bool) {
    let paste = if read_stdin {
        "\nAfter the login, the browser goes to a page on 127.0.0.1. If that \
         page does not load, paste its URL here and press Enter.\n"
    } else {
        ""
    };
    let text = format!("Open this URL in a browser to log in to {server}:\n\n  {url}\n{paste}\n");
    match context.ui.format {
        super::ui::Format::Text => context.ui.diagnostic(&text),
        super::ui::Format::Json => context.ui.diagnostic(&super::ui::json_line(
            &json!({"login": {"server": server, "url": url}}),
        )),
    }
}

/// The page that the browser shows after the redirect.
fn page(message: &str) -> String {
    let body = format!(
        "<!doctype html><title>ncp</title><p>{}</p>",
        message.replace('&', "&amp;").replace('<', "&lt;")
    );
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// One redirect on the listener: the URL, or `None` for another request
/// (for example the icon of the page).
async fn accept_redirect(
    listener: &TcpListener,
    port: u16,
) -> Option<(String, tokio::net::TcpStream)> {
    let (mut stream, _) = listener.accept().await.ok()?;
    let mut line = String::new();
    BufReader::new(&mut stream)
        .read_line(&mut line)
        .await
        .ok()?;
    let target = line.split_whitespace().nth(1)?.to_string();
    if !target.starts_with("/callback") {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .await;
        return None;
    }
    Some((format!("http://127.0.0.1:{port}{target}"), stream))
}

/// Waits for the redirect: on the listener, or pasted on stdin. Ends at
/// Ctrl-C and at the time limit, so that nothing waits forever.
async fn wait_for_redirect(
    context: &Context<'_>,
    listener: TcpListener,
    redirect: &RedirectUri,
    state: &str,
    read_stdin: bool,
) -> Result<callback::Callback, Failure> {
    let port = redirect.port();
    let from_listener = async {
        loop {
            if let Some((url, mut stream)) = accept_redirect(&listener, port).await {
                let result = callback::parse(&url, redirect, state);
                let message = match &result {
                    Ok(_) => "The login is complete. You can close this tab.".to_string(),
                    Err(error) => format!("The login failed: {error}."),
                };
                let _ = stream.write_all(page(&message).as_bytes()).await;
                return result;
            }
        }
    };
    let from_stdin = async {
        if !read_stdin {
            return std::future::pending().await;
        }
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) if line.trim().is_empty() => {}
                Ok(Some(line)) => return callback::parse(&line, redirect, state),
                // The end of stdin: only the listener can answer now.
                _ => return std::future::pending().await,
            }
        }
    };
    let limit = context.request_limit;
    let result = tokio::select! {
        result = from_listener => result,
        result = from_stdin => result,
        () = context.cancel.cancelled() => return Err(session::interrupted()),
        () = tokio::time::sleep(limit) => return Err(Failure::new(
            Exit::Auth,
            format!("the login did not complete in {} s", limit.as_secs()),
        ).hint("give a longer limit with --timeout <SECONDS>")),
    };
    result.map_err(|error| Failure::new(Exit::Auth, error.to_string()))
}

async fn logout(context: &Context<'_>, word: &str) -> Result<(), Failure> {
    let loaded = session::load_config(&context.paths.config_file, context.env)?;
    let server = session::select(&loaded, word, context.ui)?;
    let name = server.name.as_str();
    remote(server)?;
    let key = CredentialKey::OAuthTokens(server.name.clone());
    let removed = remove_everywhere(context, name, &key).await?;
    let places: Vec<&str> = removed
        .iter()
        .map(|place| secrets::place_name(*place))
        .collect();
    context.ui.result(
        || match removed.as_slice() {
            [] => format!("No tokens of {name} were stored.\n"),
            places => {
                let names: Vec<&str> = places.iter().map(|p| secrets::place_text(*p)).collect();
                format!(
                    "Removed the tokens of {name} from {}.\n",
                    names.join(" and ")
                )
            }
        },
        || json!({"server": name, "places": places}),
    )
}

/// Removes the item from the native store and the file. A native store
/// that cannot answer gives a warning, so that the file copy goes away.
async fn remove_everywhere(
    context: &Context<'_>,
    name: &str,
    key: &CredentialKey,
) -> Result<Vec<CredentialPlace>, Failure> {
    let mut removed = Vec::new();
    match secrets::native(context).await {
        Ok(native) => {
            let key = key.clone();
            match secrets::call(context, &native, move |store| store.delete(&key)).await {
                Ok(true) => removed.push(CredentialPlace::Native),
                Ok(false) => {}
                Err(error) => return Err(secrets::store_failure(&error).server(name)),
            }
        }
        Err(crate::store::StoreError::Unavailable(_)) => {}
        Err(error) => context.ui.warning(
            Some(name),
            &format!("the native store gave no answer: {error}"),
        ),
    }
    let file = secrets::file(context);
    let key = key.clone();
    if secrets::call(context, &file, move |store| store.delete(&key))
        .await
        .map_err(|error| secrets::store_failure(&error).server(name))?
    {
        removed.push(CredentialPlace::File);
    }
    Ok(removed)
}

async fn status(context: &Context<'_>, word: Option<&str>) -> Result<(), Failure> {
    let loaded = session::load_config(&context.paths.config_file, context.env)?;
    let servers: Vec<&Server> = match word {
        Some(word) => {
            let server = session::select(&loaded, word, context.ui)?;
            remote(server)?;
            vec![server]
        }
        None => loaded
            .config
            .servers()
            .filter(|server| matches!(server.transport, Transport::Http(_)))
            .collect(),
    };
    let native = if servers.is_empty() {
        None
    } else {
        secrets::native(context).await.ok()
    };
    let file = secrets::file(context);
    let mut rows = Vec::new();
    for server in servers {
        let key = CredentialKey::OAuthTokens(server.name.clone());
        let tokens = read_tokens(context, native.as_ref(), &file, &key).await?;
        rows.push((server.name.to_string(), items::status(tokens.as_ref())));
    }
    context.ui.result(
        || status_text(&rows),
        || {
            json!({"servers": rows.iter().map(|(name, status)| json!({
                "name": name,
                "loggedIn": status.logged_in,
                "issuer": status.issuer,
                "scopes": status.scopes,
                "expiresAt": status.expires_at.map(items::rfc3339),
            })).collect::<Vec<Value>>()})
        },
    )
}

/// The tokens of the first place that has them, as JSON. No value leaves
/// this function except through the status view.
async fn read_tokens(
    context: &Context<'_>,
    native: Option<&Arc<dyn CredentialStore>>,
    file: &Arc<dyn CredentialStore>,
    key: &CredentialKey,
) -> Result<Option<Value>, Failure> {
    for store in native.into_iter().chain([file]) {
        let wanted = key.clone();
        let found = secrets::call(context, store, move |store| store.load(&wanted))
            .await
            .map_err(|error| secrets::store_failure(&error))?;
        if let Some(secret) = found {
            let tokens =
                parse_tokens(&secret).map_err(|error| Failure::protocol(error.to_string()))?;
            return Ok(serde_json::to_value(tokens).ok());
        }
    }
    Ok(None)
}

fn status_text(rows: &[(String, items::Status)]) -> String {
    if rows.is_empty() {
        return "No http server is configured.\n".to_string();
    }
    let table: Vec<Vec<String>> = rows
        .iter()
        .map(|(name, status)| {
            let state = if status.logged_in {
                "logged in"
            } else {
                "not logged in"
            };
            vec![
                name.clone(),
                state.to_string(),
                status.issuer.clone().unwrap_or_default(),
                status.scopes.join(" "),
                status.expires_at.map(items::rfc3339).unwrap_or_default(),
            ]
        })
        .collect();
    super::render::table(&table)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(endpoint: &str) -> AuthorizationMetadata {
        let mut metadata = AuthorizationMetadata::default();
        metadata.authorization_endpoint = format!("{endpoint}/authorize");
        metadata.token_endpoint = format!("{endpoint}/token");
        metadata
    }

    #[test]
    fn authorization_server_must_use_https_except_on_loopback() {
        assert!(check_https(&metadata("https://as.test")).is_ok());
        assert!(check_https(&metadata("http://127.0.0.1:9")).is_ok());
        assert!(check_https(&metadata("http://localhost:9")).is_ok());
        let failure = check_https(&metadata("http://as.test")).unwrap_err();
        assert!(
            failure.message.contains("http://as.test/authorize"),
            "{failure:?}"
        );
    }

    #[test]
    fn issuer_falls_back_to_the_origin_of_the_authorization_endpoint() {
        assert_eq!(
            issuer_of(&metadata("https://as.test:8443")),
            "https://as.test:8443"
        );
        let mut named = metadata("https://as.test");
        named.issuer = Some("https://as.test/tenant".into());
        assert_eq!(issuer_of(&named), "https://as.test/tenant");
    }

    #[test]
    fn state_comes_from_the_authorization_url() {
        assert_eq!(
            state_of("https://as.test/authorize?client_id=c&state=s1&code_challenge=x"),
            Some("s1".into())
        );
        assert_eq!(state_of("https://as.test/authorize"), None);
    }
}
