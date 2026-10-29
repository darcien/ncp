//! The `auth` commands: OAuth login, logout and status.
//!
//! rmcp does the protocol steps. This module chooses the client, runs the
//! redirect, and keeps the items in the place that the user chose.

use std::{io::IsTerminal, sync::Arc, time::Duration};

use rmcp::transport::auth::{
    AuthClient, AuthError, AuthorizationManager, AuthorizationMetadata, AuthorizationRequest,
    AuthorizationSession, OAuthClientConfig,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt as _, AsyncWriteExt, BufReader},
    net::TcpListener,
};

use super::{
    args::{AuthCommand, CredentialPlace},
    diagnostics,
    outcome::{Exit, Failure, Hint, Warning},
    secrets,
    session::{self, Context},
};
use crate::{
    client::RequestError,
    config::{EntryError, LookupError, PreRegisteredClient, RemoteServer, Server, Transport},
    credential::CredentialKey,
    oauth::{
        callback::{self, RedirectUri},
        http::OAuthHttp,
        items::{self, ClientItem},
        scopes,
        store::{StoreText, TokenStore, parse_item, read_store_text},
    },
    server::ServerName,
    store::CredentialStore,
    text::code,
};

/// Tests replace the browser with a fake.
pub type OpenBrowser = fn(&str) -> std::io::Result<()>;

/// Opens the URL with `open` on macOS and `xdg-open` on Linux. The URL can
/// be a process argument, because its state and PKCE challenge are not
/// secret.
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

const CLIENT_NAME: &str = "ncp";

/// The URL of the Client ID Metadata Document of `ncp`, which is its client
/// ID at a server that supports CIMD. The source of the document is
/// `site/public/oauth/client-metadata.json`. A server does not follow a
/// redirect, so a release that moves the document changes this URL. A new
/// URL is a new client (CIMD draft-02 §8.3).
pub const CLIENT_METADATA_URL: &str = "https://darcien.github.io/ncp/oauth/client-metadata.json";

pub(super) async fn command(context: &Context<'_>, command: AuthCommand) -> Result<(), Failure> {
    match command {
        AuthCommand::Login {
            server,
            no_browser,
            credential_store,
        } => {
            let limit = context.request_limit;
            let timeout = Failure::protocol(format!(
                "The login to the server {} did not complete in {} s",
                code(&server),
                limit.as_secs()
            ))
            .about(server.as_str())
            .hint(session::wait_longer());
            let notes = crate::client::http::ConnectNotes::default();
            let loaded = session::load(context, Some(&server)).await?;
            let name = limited(
                context,
                limit,
                timeout,
                Box::pin(login(
                    context,
                    (&loaded, &server),
                    no_browser,
                    (credential_store, &notes),
                )),
            )
            .await
            .map_err(|failure| login_failure(context, (&loaded, &server), &notes, failure))?;
            // The cleanup is outside the login limit: the store has the tokens,
            // and the cleanup has its own short limit.
            remove_old_tokens(context, &name, credential_store).await
        }
        AuthCommand::Logout { server } => logout(context, &server).await,
        AuthCommand::Status { server } => status(context, server.as_deref()).await,
    }
}

// rmcp puts the server URL in its errors, so the failure shows the URL as
// the file writes it. The URL comes from the configuration of the login,
// because the file can change during a long wait for the browser.
fn login_failure(
    context: &Context<'_>,
    (loaded, word): (&session::Loaded, &str),
    notes: &crate::client::http::ConnectNotes,
    failure: Failure,
) -> Failure {
    let failure = match loaded.config.server(word).ok() {
        Some(server) => failure.map_text(|text| session::file_url(server, &text)),
        None => failure,
    };
    session::with_tls_hint(context.env, &notes.get(), failure)
}

/// Each step of rmcp has its own limit of 30 s, and discovery tries up to
/// eight URLs. Thus one limit applies to all the steps, so that a login
/// cannot hang.
pub(super) async fn limited<T>(
    context: &Context<'_>,
    limit: Duration,
    timeout: Failure,
    work: impl Future<Output = Result<T, Failure>>,
) -> Result<T, Failure> {
    tokio::select! {
        result = work => result,
        () = context.cancel.cancelled() => Err(session::interrupted(None)),
        () = tokio::time::sleep(limit) => Err(timeout),
    }
}

fn remote(server: &Server) -> Result<&RemoteServer, Failure> {
    match &server.transport {
        Transport::Http(remote) => Ok(remote),
        _ => Err(Failure::usage(format!(
            "The server {} is a stdio server, and OAuth is only for HTTP servers",
            code(server.name.as_str())
        ))
        .about(server.name.as_str())
        .hint("To give credentials to a stdio server, use the `env` field of its entry")),
    }
}

// The text of rmcp is a cause, because it is foreign text. A store error
// in the text of rmcp has the marks of `ncp`, so it gets its own cause.
fn auth_failure(step: &str, server: &str, error: &AuthError) -> Failure {
    let failed = |exit| {
        Failure::new(
            exit,
            format!("The {step} for the server {} failed", code(server)),
        )
        .about(server)
    };
    if let AuthError::CredentialStoreError(text) = error {
        return match read_store_text(text) {
            StoreText::Binding(mismatch) => {
                failed(Exit::Auth).cause(binding_reason(&mismatch, &code))
            }
            StoreText::Store { kind, cause, fix } => failed(Exit::from(kind))
                .cause(cause)
                .with_hints(fix.iter().map(super::fixes::fix_hint)),
        };
    }
    let exit = match error {
        AuthError::AuthorizationRequired
        | AuthError::AuthorizationFailed(_)
        | AuthError::TokenExchangeFailed(_)
        | AuthError::TokenRefreshRejected(_)
        | AuthError::AuthorizationServerMismatch { .. }
        | AuthError::AuthorizationServerMissingIssuer { .. } => Exit::Auth,
        _ => Exit::Protocol,
    };
    failed(exit).caused_by(error)
}

// rmcp falls back to endpoints that it makes from the server URL. `ncp`
// refuses that fallback: only MCP 2025-03-26 has it (basic/authorization,
// "Fallbacks for Servers without Metadata Discovery"), and it
// registers a client at a server that names no authorization server.
async fn discover(
    manager: &AuthorizationManager,
    challenge: Option<&str>,
    server: &str,
) -> Result<AuthorizationMetadata, Failure> {
    let resolution = manager
        .resolve_metadata_from_challenge(challenge)
        .await
        .map_err(|error| auth_failure("OAuth discovery", server, &error))?;
    if !resolution.source.is_discovered() {
        return Err(Failure::protocol(format!(
            "The server {} names no authorization server",
            code(server)
        ))
        .about(server)
        .cause(
            "The server has no protected resource metadata and no authorization server metadata",
        ));
    }
    check_https(server, &resolution.metadata)?;
    Ok(resolution.metadata)
}

// The endpoints of the authorization server must use HTTPS (MCP
// 2025-11-25, basic/authorization, "Communication Security"). `ncp`
// also accepts `http` on a loopback host, because the local tests use
// a loopback authorization server. rmcp does not do this check.
fn check_https(server: &str, metadata: &AuthorizationMetadata) -> Result<(), Failure> {
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
                "The authorization server of the server {} has the endpoint {}, which does not use `https`",
                code(server),
                code(endpoint)
            ))
            .about(server))
        })
}

fn is_secure(endpoint: &str) -> bool {
    url::Url::parse(endpoint).is_ok_and(|url| crate::server::is_secure_url(&url))
}

// rmcp requires the issuer only when discovery can derive it. Without an
// issuer, the origin of the authorization endpoint names the server.
fn issuer_of(metadata: &AuthorizationMetadata) -> String {
    metadata.issuer.clone().unwrap_or_else(|| {
        url::Url::parse(&metadata.authorization_endpoint).map_or_else(
            |_| metadata.authorization_endpoint.clone(),
            |url| url.origin().ascii_serialization(),
        )
    })
}

// The native store must answer before the login starts, so that the user
// does not log in for nothing.
async fn place_store(
    context: &Context<'_>,
    server: &str,
    place: CredentialPlace,
) -> Result<Arc<dyn CredentialStore>, Failure> {
    match place {
        CredentialPlace::Native => secrets::native(context).await.map_err(|error| {
            let message = format!(
                "ncp cannot open the native store for the tokens of the server {}",
                code(server)
            );
            let file = Hint::run_text(
                "store the tokens in the credential file",
                &format!(
                    "ncp auth login {} --credential-store file",
                    super::render::shell_word(server)
                ),
            );
            secrets::store_failure_about(Some(server), &error, message, [file])
        }),
        CredentialPlace::File => Ok(secrets::file(context)),
    }
}

async fn login(
    context: &Context<'_>,
    (loaded, word): (&session::Loaded, &str),
    no_browser: bool,
    (place, notes): (CredentialPlace, &crate::client::http::ConnectNotes),
) -> Result<ServerName, Failure> {
    let server = select(context, loaded, word).await?;
    session::know_url_values(context.ui, server);
    let name = server.name.as_str();
    let remote = remote(server)?;
    let store = place_store(context, name, place).await?;
    let observer = diagnostics::observer(
        context.ui,
        name,
        context.timings.clone(),
        session::shown_url(server, context.ui),
    );
    let http = OAuthHttp::new(context.env, &observer, notes)
        .map_err(|error| session::http_failure(name, &error))?;
    let mut manager = AuthorizationManager::new_with_oauth_http_client(
        remote.url.without_query(),
        Arc::new(http),
    )
    .await
    .map_err(|error| auth_failure("OAuth setup", name, &error))?;
    let challenge = session::challenge(context, server).await?;
    let metadata = discover(&manager, challenge.as_deref(), name).await?;
    let issuer = issuer_of(&metadata);
    context
        .ui
        .debug(Some(name), &format!("authorization server: {issuer}"));
    // Before the browser opens: the user must not log in at an issuer that
    // the entry does not name.
    if let Some(client) = &remote.oauth_client {
        check_issuer(name, client, &metadata)?;
    }
    // rmcp reads the scopes of the authorization server only to select
    // scopes. Without them, it falls back to no scope, as the spec does.
    let mut selection = metadata.clone();
    selection.scopes_supported = None;
    manager.set_metadata(selection);
    let initial = initial_scopes(&manager, challenge.as_deref(), remote, &metadata);
    let binding = items::TokenBinding {
        resource: remote.url.without_query(),
        issuer: issuer.clone(),
    };
    let scopes = login_scopes(context, server, (&store, place), (&initial, &binding)).await;
    manager.set_credential_store(
        TokenStore::new(server.name.clone(), store.clone())
            .fresh()
            .with_binding(binding)
            .with_requested_scopes(&scopes)
            .with_known(context.ui.known().clone()),
    );
    let (listener, redirect) = listen().await?;
    let request = client_request(
        context,
        server,
        remote,
        &mut manager,
        &store,
        &metadata,
        (&redirect, &scopes),
    )
    .await?;
    let session = AuthorizationSession::new(manager, request)
        .await
        .map_err(|(_, error)| auth_failure("OAuth login", name, &error))?;
    let url = session.get_authorization_url().to_string();
    let state = state_of(&url).ok_or_else(|| {
        Failure::protocol(format!(
            "The authorization URL for the server {} has no `state` parameter",
            code(name)
        ))
        .about(name)
    })?;
    let read_stdin = no_browser || std::io::stdin().is_terminal();
    show_url(context, name, &url, read_stdin);
    if !no_browser {
        show_in_browser(context, name, &url);
    }
    let expected = expected_redirect(state, &metadata);
    let callback = wait_for_redirect(listener, &redirect, &expected, read_stdin, name).await?;
    // A refresh of the old tokens in another process must not save over
    // the new tokens.
    let _lock = hold_tokens_lock(context, &server.name).await?;
    session
        .handle_callback_with_issuer(&callback.code, &callback.state, callback.iss.as_deref())
        .await
        .map_err(|error| token_failure(&error, server, place))?;
    // A list of the old login can differ from the list of this one.
    crate::list_cache::remove(&crate::list_cache::path(
        &context.paths.cache_dir,
        &server.name,
    ));
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
                "credentialStore": secrets::place_name(place)})
        },
    )?;
    Ok(server.name.clone())
}

// The URL is on the screen already, so a browser that does not open gives
// only a warning.
fn show_in_browser(context: &Context<'_>, name: &str, url: &str) {
    if let Err(error) = (context.open_browser)(url) {
        context.ui.warn(
            &Warning::new(format!(
                "ncp cannot open a browser for the login to the server {}",
                code(name)
            ))
            .about(name)
            .caused_by(&error)
            .hint("To log in, open the URL above in a browser"),
        );
    }
}

// What the redirect of the login must have: its state, and the issuer of the
// metadata in `iss` (RFC 9207 §2.4). The metadata says if `iss` must be
// present.
fn expected_redirect(state: String, metadata: &AuthorizationMetadata) -> callback::Expected {
    callback::Expected {
        state,
        issuer: issuer_of(metadata),
        iss_required: metadata
            .additional_fields
            .get("authorization_response_iss_parameter_supported")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

// The redirect uses a random loopback port (RFC 8252 §7.3).
async fn listen() -> Result<(TcpListener, RedirectUri), Failure> {
    let failure = |error: std::io::Error| {
        Failure::protocol("ncp cannot listen for the redirect of the login").caused_by(&error)
    };
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(failure)?;
    let port = listener.local_addr().map_err(failure)?.port();
    Ok((listener, RedirectUri::new(port)))
}

fn refresh_lock(context: &Context<'_>, server: &ServerName) -> std::path::PathBuf {
    context
        .paths
        .credential_file
        .with_file_name(format!("oauth-{}.lock", server.as_str()))
}

// Gives `None` when `ncp` cannot get the lock. This is not an error,
// because the lock only puts a refresh, a login and a logout in order.
// Ctrl-C ends the wait at once. Its thread stops at the limit or at the exit.
async fn hold_tokens_lock(
    context: &Context<'_>,
    server: &ServerName,
) -> Result<Option<crate::private_file::Lock>, Failure> {
    let path = refresh_lock(context, server);
    let wait = crate::store::run_blocking(crate::store::CALL_LIMIT, move || {
        Ok(crate::oauth::store::lock_tokens(&path))
    });
    match secrets::interruptible(context, wait).await {
        Ok(Ok(lock)) => Ok(Some(lock)),
        Err(crate::store::StoreError::Interrupted) => {
            Err(session::interrupted(Some(server.as_str())))
        }
        // Another process holds the lock past its limit: go on, but say so,
        // because that process can undo this change.
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::TimedOut => {
            context.ui.warn(
                &Warning::new(format!(
                    "ncp continues with no lock on the tokens of the server {}",
                    code(server.as_str())
                ))
                .about(server.as_str())
                .caused_by(&error),
            );
            Ok(None)
        }
        _ => Ok(None),
    }
}

// A store that cannot keep the tokens has its own fix. A file that fails
// is a local error. A native store that fails, for example a locked
// Keychain, can give way to the file.
fn token_failure(error: &AuthError, server: &Server, place: CredentialPlace) -> Failure {
    match error {
        AuthError::CredentialStoreError(text) => {
            token_text_failure(server, text, place == CredentialPlace::Native)
        }
        error => auth_failure("OAuth token request", server.name.as_str(), error),
    }
}

/// The failure for a store error that rmcp keeps as one text. The store
/// text is one cause, then the hints of its fix. When the native store
/// failed and `file_fix` is true, the last hint is the credential file:
/// the fixes of the native store come first, because the file is a plain
/// file that the user must choose.
pub(super) fn token_text_failure(server: &Server, text: &str, file_fix: bool) -> Failure {
    let name = server.name.as_str();
    match read_store_text(text) {
        StoreText::Binding(mismatch) => binding_failure(server, &mismatch),
        StoreText::Store { kind, cause, fix } => {
            let exit = Exit::from(kind);
            let failure = Failure::new(
                exit,
                format!(
                    "ncp cannot store the OAuth tokens of the server {}",
                    code(name)
                ),
            )
            .about(name)
            .cause(cause)
            .with_hints(fix.iter().map(super::fixes::fix_hint));
            if kind == crate::store::StoreKind::Local || !file_fix {
                failure
            } else {
                failure.hint(file_login_hint(name))
            }
        }
    }
}

/// The hint to keep the tokens of a login in the credential file. It has
/// no command, because the credential file is a plain file, and only the
/// user can choose it.
pub(super) fn file_login_hint(name: &str) -> Hint {
    Hint::run_text(
        "store the tokens in the credential file",
        &format!(
            "ncp auth login {} --credential-store file",
            super::render::shell_word(name)
        ),
    )
}

#[derive(Clone, Copy)]
pub(super) struct StoredLogin<'a> {
    pub server: &'a Server,
    pub remote: &'a RemoteServer,
    pub http: &'a reqwest::Client,
    pub observer: &'a crate::client::observe::Observer,
    pub notes: &'a crate::client::http::ConnectNotes,
    /// The `WWW-Authenticate` challenge of the `401` answer.
    pub challenge: Option<&'a str>,
}

/// Gives `None` when the tokens are for another issuer. `ncp` calls this
/// only after a `401` answer, so that a server with no login never opens
/// the native store.
pub(super) async fn stored_login(
    context: &Context<'_>,
    login: &StoredLogin<'_>,
    found: FoundTokens,
) -> Result<Option<crate::client::http::Authorized>, Failure> {
    let StoredLogin {
        server,
        remote,
        http,
        observer,
        notes,
        challenge,
    } = *login;
    let name = server.name.as_str();
    let (store, secret) = found;
    let item = parse_item(&secret).map_err(|error| tokens_failure(name, &error))?;
    let client_id = item.tokens.client_id.clone();
    let stored_binding = item.binding.clone();
    context.ui.debug(Some(name), "read the stored OAuth tokens");
    let oauth_http = OAuthHttp::new(context.env, observer, notes)
        .map_err(|error| session::http_failure(name, &error))?;
    let mut manager = AuthorizationManager::new_with_oauth_http_client(
        remote.url.without_query(),
        Arc::new(oauth_http),
    )
    .await
    .map_err(|error| auth_failure("OAuth setup", name, &error))?;
    manager.set_credential_store(
        TokenStore::new(server.name.clone(), store.clone())
            .with_item(item)
            .with_refresh_lock(refresh_lock(context, &server.name))
            .with_timer(credentials_timer(context))
            .with_known(context.ui.known().clone()),
    );
    // The challenge can name the protected resource metadata (RFC 9728
    // §5.1). With metadata in place, rmcp does no discovery of its own.
    let metadata = discover(&manager, challenge, name).await?;
    // Check the issuer of the configured client before rmcp reads the
    // tokens, because rmcp removes the tokens of another issuer. The check
    // also comes before a refresh sends the secret.
    let configured = remote
        .oauth_client
        .as_ref()
        .filter(|client| client.client_id == client_id);
    if let Some(client) = configured {
        check_issuer(name, client, &metadata)?;
    }
    let current = items::TokenBinding {
        resource: remote.url.without_query(),
        issuer: issuer_of(&metadata),
    };
    items::check_binding(stored_binding.as_ref(), &current)
        .map_err(|mismatch| binding_failure(server, &mismatch))?;
    manager.set_metadata(metadata);
    let usable = manager
        .initialize_from_store()
        .await
        .map_err(|error| auth_failure("OAuth discovery", name, &error))?;
    if !usable {
        return Ok(None);
    }
    // rmcp configures only the client ID from the tokens. A refresh of a
    // confidential client needs its secret too (RFC 6749 §6).
    if let Some(client) = configured {
        if let Some(secret) = secrets::client_secret(context, server, client).await? {
            let config = OAuthClientConfig::new(client_id, RedirectUri::new(0).to_string())
                .with_client_secret(secret.expose());
            manager
                .configure_client(config)
                .map_err(|error| auth_failure("OAuth setup", name, &error))?;
        }
        return Ok(Some(AuthClient::new(
            crate::client::http::observed(http, observer),
            manager,
        )));
    }
    // A CIMD client has no secret and no client item, so no command needs
    // to open the item.
    let secret = if client_id == context.client_metadata_url {
        None
    } else {
        stored_client_secret(context, server, &store, &client_id).await?
    };
    if let Some(secret) = secret {
        context.ui.known().add([secret.clone()]);
        let config = OAuthClientConfig::new(client_id, RedirectUri::new(0).to_string())
            .with_client_secret(secret);
        manager
            .configure_client(config)
            .map_err(|error| auth_failure("OAuth setup", name, &error))?;
    }
    Ok(Some(AuthClient::new(
        crate::client::http::observed(http, observer),
        manager,
    )))
}

// The secret of the stored client item, when the item is for the client
// of the tokens.
async fn stored_client_secret(
    context: &Context<'_>,
    server: &Server,
    store: &Arc<dyn CredentialStore>,
    client_id: &str,
) -> Result<Option<String>, Failure> {
    let name = server.name.as_str();
    let client_key = CredentialKey::OAuthClient(server.name.clone());
    let started = std::time::Instant::now();
    let client = secrets::call(context, store, move |store| store.load(&client_key)).await;
    if let Some(timer) = credentials_timer(context) {
        timer(started.elapsed());
    }
    Ok(client
        .map_err(|error| {
            let message = format!(
                "ncp cannot read the OAuth client of the server {}",
                code(name)
            );
            secrets::store_failure_about(Some(name), &error, message, [])
        })?
        .and_then(|secret| ClientItem::from_secret(&secret).ok())
        .filter(|item| item.client_id == client_id)
        .and_then(|item| item.client_secret))
}

/// The message names only the current value, because a stored URL or
/// issuer can hold an old `${NAME}` value that `ncp` cannot hide.
pub(super) fn binding_failure(server: &Server, mismatch: &items::BindingMismatch) -> Failure {
    let name = server.name.as_str();
    let reason = binding_reason(mismatch, &|current| {
        code(&session::file_url(server, current))
    });
    // The message says that the stop is on purpose: it is a safety check,
    // not a defect.
    Failure::new(
        Exit::Auth,
        format!(
            "ncp did not send the stored tokens of the server {}. ncp did this on purpose.",
            code(name)
        ),
    )
    .about(name)
    .cause(reason)
    .hint(session::login_hint(server))
}

// Why `ncp` did not send the tokens. `shown` writes the current issuer,
// which can hold a `${NAME}` value of the server URL.
fn binding_reason(mismatch: &items::BindingMismatch, shown: &dyn Fn(&str) -> String) -> String {
    match mismatch {
        items::BindingMismatch::Missing => {
            "The tokens do not name the server that they are for".to_string()
        }
        items::BindingMismatch::Resource => {
            "The tokens are for a URL that is not the URL of the entry".to_string()
        }
        items::BindingMismatch::Issuer { current } => format!(
            "The tokens are from an authorization server that is not {}",
            shown(current)
        ),
    }
}

// A stored item that is not valid JSON of ncp.
fn tokens_failure(name: &str, error: &items::ItemError) -> Failure {
    Failure::protocol(format!(
        "ncp cannot read the stored OAuth items of the server {}",
        code(name)
    ))
    .about(name)
    .caused_by(error)
}

// A refresh reads and writes the tokens again, so `--timings` counts these
// calls in `credentialsMs`.
fn credentials_timer(context: &Context<'_>) -> Option<crate::oauth::store::StoreTimer> {
    let timings = context.timings.clone()?;
    Some(Arc::new(move |elapsed| {
        timings.update(|record| super::timings::add(&mut record.credentials, elapsed));
    }))
}

/// Keeps the scopes of a `403` answer in the tokens item. Then the next
/// `auth login` asks for them together with the scopes that the user has,
/// as the spec requires for a step-up. A failure gives only a warning,
/// because the error of the call is the result.
pub(super) async fn remember_scopes(context: &Context<'_>, server: &Server, error: &RequestError) {
    let Some(scopes) = error.required_scopes().filter(|scopes| !scopes.is_empty()) else {
        return;
    };
    let name = server.name.as_str();
    if let Err(failure) = add_required_scopes(context, server, &scopes).await {
        context.ui.warn(
            &Warning::from(failure.context(format!(
                "ncp cannot store the scopes of the server {} for the next login",
                code(name)
            )))
            .about(name),
        );
    }
}

async fn add_required_scopes(
    context: &Context<'_>,
    server: &Server,
    scopes: &[String],
) -> Result<(), Failure> {
    // The read and the write of the item must not go around the save of a
    // login or a refresh in another process.
    let _lock = hold_tokens_lock(context, &server.name).await?;
    let key = CredentialKey::OAuthTokens(server.name.clone());
    if let Some((store, secret)) = find_tokens(context, server).await? {
        let name = server.name.as_str();
        let mut tokens: Value = serde_json::from_str(secret.expose())
            .map_err(|error| not_json(name).caused_by(&items::ItemReason::of(&error)))?;
        let required = scopes::union(&scopes::list(tokens.get(items::REQUIRED_SCOPES)), scopes);
        tokens[items::REQUIRED_SCOPES] = json!(required);
        let secret =
            items::to_secret("tokens", &tokens).map_err(|error| tokens_failure(name, &error))?;
        let saved = key.clone();
        let started = std::time::Instant::now();
        let result = secrets::call(context, &store, move |store| store.save(&saved, &secret)).await;
        if let Some(timer) = credentials_timer(context) {
            timer(started.elapsed());
        }
        result.map_err(|error| {
            let message = format!("ncp cannot store the tokens of the server {}", code(name));
            secrets::store_failure_about(Some(name), &error, message, [])
        })?;
        context.ui.debug(
            Some(server.name.as_str()),
            &format!(
                "stored the scopes {} for the next login",
                required.join(" ")
            ),
        );
    }
    Ok(())
}

/// The tokens item of a server, and the store that has it.
pub(super) type FoundTokens = (Arc<dyn CredentialStore>, crate::credential::Secret);

/// The search order is the native store, then the file, as for `${NAME}`.
/// A native store that cannot answer, for example a locked keyring, does
/// not stop the search. Its error is the result only when the file has no
/// tokens, so that the error tells why.
pub(super) async fn find_tokens(
    context: &Context<'_>,
    server: &Server,
) -> Result<Option<FoundTokens>, Failure> {
    TokenPlaces::open(context).await.find(context, server).await
}

/// A native store that cannot answer stays marked as down. Thus the search
/// for the next server does not wait for the store limit again.
pub(super) struct TokenPlaces {
    native: Option<Arc<dyn CredentialStore>>,
    down: Option<crate::store::StoreError>,
    file: Arc<dyn CredentialStore>,
}

impl TokenPlaces {
    pub(super) async fn open(context: &Context<'_>) -> Self {
        let (native, down) = match secrets::native(context).await {
            Ok(native) => (Some(native), None),
            Err(error) => (None, Some(error)),
        };
        Self {
            native,
            down,
            file: secrets::file(context),
        }
    }

    // A host with no native store has only the file, as for `secret list`.
    // A user who keeps all values in the file must not get a warning at
    // each run.
    async fn open_for_status(context: &Context<'_>) -> Self {
        match Self::open(context).await {
            Self {
                down: Some(crate::store::StoreError::Unavailable(_)),
                file,
                ..
            } => Self {
                native: None,
                down: None,
                file,
            },
            places => places,
        }
    }

    pub(super) async fn find(
        &mut self,
        context: &Context<'_>,
        server: &Server,
    ) -> Result<Option<FoundTokens>, Failure> {
        let started = std::time::Instant::now();
        let found = self.search(context, server).await;
        if let Some(timings) = &context.timings {
            timings
                .update(|record| super::timings::add(&mut record.credentials, started.elapsed()));
        }
        found
    }

    async fn search(
        &mut self,
        context: &Context<'_>,
        server: &Server,
    ) -> Result<Option<FoundTokens>, Failure> {
        let name = server.name.as_str();
        let key = CredentialKey::OAuthTokens(server.name.clone());
        let read_failure = |error: &crate::store::StoreError, place| {
            let message = format!(
                "ncp cannot read the OAuth tokens of the server {} from {}",
                code(name),
                secrets::place_text(place)
            );
            secrets::store_failure_about(Some(name), error, message, [])
        };
        if let Some(native) = self.native.clone() {
            let wanted = key.clone();
            match secrets::call(context, &native, move |store| store.load(&wanted)).await {
                Ok(Some(secret)) => return Ok(Some((native, secret))),
                Ok(None) => {}
                Err(crate::store::StoreError::Interrupted) => {
                    return Err(session::interrupted(Some(name)));
                }
                Err(error) if secrets::store_is_down(&error) => {
                    self.native = None;
                    self.down = Some(error);
                }
                Err(error) => return Err(read_failure(&error, CredentialPlace::Native)),
            }
        }
        let file = self.file.clone();
        let found = secrets::call(context, &file, move |store| store.load(&key))
            .await
            .map_err(|error| read_failure(&error, CredentialPlace::File))?;
        match (found, &self.down) {
            (Some(secret), _) => Ok(Some((file, secret))),
            (None, Some(crate::store::StoreError::Interrupted)) => {
                Err(session::interrupted(Some(name)))
            }
            (None, Some(error)) => Err(Failure::new(
                Exit::Auth,
                format!(
                    "The credential file has no OAuth tokens of the server {}, and ncp cannot use the native store",
                    code(name)
                ),
            )
            .about(name)
            .caused_by(error)
            .with_hints(super::fixes::store_hints(error))
            .hint(file_login_hint(name))),
            (None, None) => Ok(None),
        }
    }
}

// rmcp can parse the challenge too, but it adds the scopes of the
// protected resource metadata. The spec gives the challenge priority
// (MCP 2026-07-28, basic/authorization, "Scope Selection Strategy").
// With no scope in the challenge, rmcp selects from the protected
// resource metadata.
fn initial_scopes(
    manager: &AuthorizationManager,
    challenge: Option<&str>,
    remote: &RemoteServer,
    metadata: &AuthorizationMetadata,
) -> Vec<String> {
    let selected = challenge
        .and_then(|challenge| scopes::challenged(challenge, &remote.url.without_query()))
        .unwrap_or_else(|| manager.select_scopes(None, &[]));
    scopes::with_offline_access(selected, metadata.scopes_supported.as_deref())
}

// Tokens of another server or issuer give no scopes to this login, so
// that one server cannot add scopes to the login of another server.
async fn login_scopes(
    context: &Context<'_>,
    server: &Server,
    chosen: (&Arc<dyn CredentialStore>, CredentialPlace),
    (initial, binding): (&[String], &items::TokenBinding),
) -> Vec<String> {
    let key = CredentialKey::OAuthTokens(server.name.clone());
    let (store, place) = chosen;
    let wanted = key.clone();
    let in_chosen = secrets::call(context, store, move |store| store.load(&wanted))
        .await
        .ok()
        .flatten();
    let found = if in_chosen.is_some() {
        in_chosen
    } else if let Ok(other) = other_place(context, place).await {
        crate::store::run_blocking(CLEANUP_LIMIT, move || other.load(&key))
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    let fields = found
        .and_then(|secret| parse_item(&secret).ok())
        .filter(|item| items::check_binding(item.binding.as_ref(), binding).is_ok())
        .map(|item| item.scope_fields)
        .unwrap_or_default();
    scopes::for_login(
        initial,
        &scopes::list(fields.get(items::REQUESTED_SCOPES)),
        &scopes::list(fields.get(items::REQUIRED_SCOPES)),
    )
}

/// The cleanup after a login must not wait for an unlock prompt of a store
/// that the user did not choose.
const CLEANUP_LIMIT: Duration = Duration::from_secs(5);

// The native store gets the short limit, because the user did not choose
// it.
async fn other_place(
    context: &Context<'_>,
    chosen: CredentialPlace,
) -> Result<Arc<dyn CredentialStore>, crate::store::StoreError> {
    match chosen {
        CredentialPlace::Native => Ok(secrets::file(context)),
        CredentialPlace::File => secrets::open_within(context, CLEANUP_LIMIT).await,
    }
}

// Calls read the native store first. Thus an old item there hides the new
// tokens in the file. A failure gives only a warning, because the login
// worked.
async fn remove_old_tokens(
    context: &Context<'_>,
    name: &ServerName,
    place: CredentialPlace,
) -> Result<(), Failure> {
    let other = other_place(context, place).await;
    let key = CredentialKey::OAuthTokens(name.clone());
    let removed = match other {
        Ok(store) => {
            let delete = crate::store::run_blocking(CLEANUP_LIMIT, move || store.delete(&key));
            secrets::interruptible(context, delete).await
        }
        Err(crate::store::StoreError::Unavailable(_)) => Ok(false),
        Err(error) => Err(error),
    };
    let name = name.as_str();
    match removed {
        Ok(true) => context
            .ui
            .debug(Some(name), "deleted the old tokens from the other store"),
        Ok(false) => {}
        Err(crate::store::StoreError::Interrupted) => {
            return Err(session::interrupted(Some(name)));
        }
        Err(error) => context.ui.warn(
            &Warning::new(format!(
                "ncp cannot delete the old tokens of the server {} from the other store",
                code(name)
            ))
            .about(name)
            .caused_by(&error),
        ),
    }
    Ok(())
}

/// The client of a login, in the priority order of the spec (MCP
/// 2026-07-28, basic/authorization/client-registration, "Client
/// Registration").
enum LoginClient<'a> {
    /// `oauth.clientId` of the entry.
    Configured(&'a PreRegisteredClient),
    /// The URL of the Client ID Metadata Document of `ncp`.
    MetadataDocument(&'a str),
    /// The stored client of the same issuer, or a new registration.
    Registered,
}

fn login_client<'a>(
    remote: &'a RemoteServer,
    metadata: &AuthorizationMetadata,
    document_url: &'a str,
) -> LoginClient<'a> {
    match &remote.oauth_client {
        Some(client) => LoginClient::Configured(client),
        None if supports_metadata_document(metadata) => LoginClient::MetadataDocument(document_url),
        None => LoginClient::Registered,
    }
}

fn supports_metadata_document(metadata: &AuthorizationMetadata) -> bool {
    metadata
        .additional_fields
        .get("client_id_metadata_document_supported")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

async fn client_request(
    context: &Context<'_>,
    server: &Server,
    remote: &RemoteServer,
    manager: &mut AuthorizationManager,
    store: &Arc<dyn CredentialStore>,
    metadata: &AuthorizationMetadata,
    (redirect, scopes): (&RedirectUri, &[String]),
) -> Result<AuthorizationRequest, Failure> {
    let name = server.name.as_str();
    let request = AuthorizationRequest::new(redirect.to_string())
        .with_client_name(CLIENT_NAME)
        .with_scopes(scopes.to_vec());
    match login_client(remote, metadata, context.client_metadata_url) {
        LoginClient::Configured(client) => {
            context
                .ui
                .debug(Some(name), "the login uses oauth.clientId of the entry");
            let request = request.with_preregistered_client(client.client_id.clone());
            Ok(
                match secrets::client_secret(context, server, client).await? {
                    Some(secret) => request.with_client_secret(secret.expose()),
                    None => request,
                },
            )
        }
        // rmcp uses the URL only when the metadata supports CIMD, and it
        // refuses a URL that is not HTTPS.
        LoginClient::MetadataDocument(url) => {
            context.ui.debug(
                Some(name),
                &format!("the login uses the client metadata document {url}"),
            );
            Ok(request.with_client_metadata_url(url))
        }
        LoginClient::Registered => {
            let client = registered_client(
                context,
                server,
                manager,
                store,
                metadata,
                (redirect, scopes),
            )
            .await?;
            let request = request.with_preregistered_client(client.client_id);
            Ok(match client.client_secret {
                Some(secret) => {
                    context.ui.known().add([secret.clone()]);
                    request.with_client_secret(secret)
                }
                None => request,
            })
        }
    }
}

// The stored client of the same issuer, else a new registration that is
// stored before the login continues. rmcp keeps a registered client only
// in memory, and a later failure must not lose the registration.
async fn registered_client(
    context: &Context<'_>,
    server: &Server,
    manager: &mut AuthorizationManager,
    store: &Arc<dyn CredentialStore>,
    metadata: &AuthorizationMetadata,
    (redirect, scopes): (&RedirectUri, &[String]),
) -> Result<ClientItem, Failure> {
    let name = server.name.as_str();
    let issuer = issuer_of(metadata);
    let key = CredentialKey::OAuthClient(server.name.clone());
    let stored = secrets::call(context, store, {
        let key = key.clone();
        move |store| store.load(&key)
    })
    .await
    .map_err(|error| {
        let message = format!(
            "ncp cannot read the OAuth client of the server {}",
            code(name)
        );
        secrets::store_failure_about(Some(name), &error, message, [])
    })?
    .map(|secret| ClientItem::from_secret(&secret))
    .transpose()
    .map_err(|error| tokens_failure(name, &error))?;
    if let Some(item) = stored.filter(|item| item.is_for(&issuer)) {
        context.ui.debug(
            Some(name),
            &format!("the login uses the stored client {}", item.client_id),
        );
        return Ok(item);
    }
    if metadata.registration_endpoint.is_none() {
        return Err(no_registration(name, metadata));
    }
    let item = register(manager, name, &issuer, redirect, scopes).await?;
    let secret = item
        .to_secret()
        .map_err(|error| tokens_failure(name, &error))?;
    secrets::call(context, store, move |store| store.save(&key, &secret))
        .await
        .map_err(|error| {
            let message = format!(
                "ncp cannot store the OAuth client of the server {}",
                code(name)
            );
            secrets::store_failure_about(Some(name), &error, message, [])
        })?;
    context.ui.debug(
        Some(name),
        &format!("registered the client {} at {issuer}", item.client_id),
    );
    Ok(item)
}

// The page that tells how to register a client (RFC 8414 §2
// `service_documentation`), only for an `https` or a loopback `http` URL.
fn registration_page(metadata: &AuthorizationMetadata) -> Option<String> {
    metadata
        .additional_fields
        .get("service_documentation")
        .and_then(Value::as_str)
        .and_then(|text| crate::elicit::safe_url(text).ok())
        .map(String::from)
}

// "register a client at `I`." and, for an `https` or a loopback `http` URL,
// the page that gives the steps. The caller gives the first word its case.
fn register_there(metadata: &AuthorizationMetadata) -> String {
    let issuer = code(&issuer_of(metadata));
    match registration_page(metadata) {
        Some(page) => format!(
            "register a client at {issuer}. The page {} gives the steps.",
            code(&page)
        ),
        None => format!("register a client at {issuer}."),
    }
}

fn no_registration(name: &str, metadata: &AuthorizationMetadata) -> Failure {
    Failure::new(
        Exit::Auth,
        format!(
            "The authorization server {} does not register clients, and the entry of the server {} names no client",
            code(&issuer_of(metadata)),
            code(name)
        ),
    )
    .about(name)
    .hint(format!(
        "To log in, {} Then set `clientId`, `clientSecret` and `issuer` in the `oauth` object of the entry of the server {}.",
        register_there(metadata),
        code(name)
    ))
}

// Pre-registered credentials are bound to their issuer (MCP 2026-07-28,
// client registration, "Authorization Server Binding"). The compare is
// exact, as for the `iss` parameter (RFC 9207 §2.4).
fn check_issuer(
    name: &str,
    client: &PreRegisteredClient,
    metadata: &AuthorizationMetadata,
) -> Result<(), Failure> {
    let actual = issuer_of(metadata);
    let Some(expected) = client.issuer().filter(|expected| *expected != actual) else {
        return Ok(());
    };
    let not_sent = if client.secret().is_some() {
        " ncp did not send the client secret."
    } else {
        ""
    };
    let keys = if client.secret().is_some() {
        "`clientId`, `clientSecret` and `issuer`"
    } else {
        "`clientId` and `issuer`"
    };
    let object = format!(
        "the `oauth` object of the entry of the server {}",
        code(name)
    );
    let moved = if metadata.registration_endpoint.is_some() {
        format!(
            "If the server moved on purpose, remove {keys} from {object}. Then run `ncp auth login {}`.",
            super::render::shell_word(name)
        )
    } else {
        format!(
            "If the server moved on purpose, {} Then change {keys} in {object}.",
            register_there(metadata)
        )
    };
    Err(Failure::new(
        Exit::Auth,
        format!(
            "ncp stopped the login to the server {} on purpose. The server names the authorization server {}, and the entry names {}.{not_sent}",
            code(name),
            code(&actual),
            code(expected)
        ),
    )
    .about(name)
    .hint(moved)
    .hint("If you did not expect the move, tell the owner of the server. Do not change the entry."))
}

// For an entry with a client secret and no issuer, the failure names the
// issuer of the server.
async fn select<'a>(
    context: &Context<'_>,
    loaded: &'a session::Loaded,
    word: &str,
) -> Result<&'a Server, Failure> {
    let failure = match session::select(loaded, word, context.ui) {
        Ok(server) => return Ok(server),
        Err(failure) => failure,
    };
    // A name that the gate stops gets no lookup of its issuer.
    if loaded.blocked_by(word).is_some() {
        return Err(failure);
    }
    Err(match loaded.config.server(word) {
        Err(LookupError::Invalid {
            error: EntryError::SecretWithoutIssuer { url },
            ..
        }) => with_server_issuer(context, failure, word, &url).await,
        _ => failure,
    })
}

/// The lookup only adds a hint. It gets at most half of the login limit,
/// so that a slow server cannot hide the usage error behind that limit.
const ISSUER_LOOKUP_LIMIT: Duration = Duration::from_secs(10);

// Discovery sends no secret, so the lookup is safe before `ncp` knows the
// issuer.
async fn with_server_issuer(
    context: &Context<'_>,
    failure: Failure,
    name: &str,
    url: &crate::server::ServerUrl,
) -> Failure {
    let notes = crate::client::http::ConnectNotes::default();
    let Ok(http) = OAuthHttp::new(
        context.env,
        &crate::client::observe::Observer::default(),
        &notes,
    ) else {
        return failure;
    };
    let Ok(manager) =
        AuthorizationManager::new_with_oauth_http_client(url.without_query(), Arc::new(http)).await
    else {
        return failure;
    };
    let limit = ISSUER_LOOKUP_LIMIT.min(context.request_limit / 2);
    match tokio::time::timeout(limit, discover(&manager, None, name)).await {
        // The issuer of the server is the same fix as the general hint,
        // with the value. Thus it replaces that hint.
        Ok(Ok(metadata)) => {
            let issuer = issuer_of(&metadata);
            failure.with_hints([Hint::text(format!(
                "If you registered the client at {}, add {} to the `oauth` object of the entry of the server {}",
                code(&issuer),
                code(&format!("\"issuer\": \"{issuer}\"")),
                code(name)
            ))])
        }
        Ok(Err(_)) | Err(_) => failure,
    }
}

async fn register(
    manager: &mut AuthorizationManager,
    server: &str,
    issuer: &str,
    redirect: &RedirectUri,
    scopes: &[String],
) -> Result<ClientItem, Failure> {
    let scope_refs: Vec<&str> = scopes.iter().map(String::as_str).collect();
    let config = manager
        .register_client(CLIENT_NAME, &redirect.to_string(), &scope_refs)
        .await
        .map_err(|error| auth_failure("OAuth client registration", server, &error))?;
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

// JSON output gives `{"login": {"server", "url"}}`, so that an agent can
// give the URL to a person.
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

fn page(status: &str, message: &str) -> String {
    let body = format!(
        "<!doctype html><title>ncp</title><p>{}</p>",
        message.replace('&', "&amp;").replace('<', "&lt;")
    );
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// A connection that sends nothing must not block the real redirect.
const REQUEST_LINE_LIMIT: Duration = Duration::from_secs(5);
const MAX_REQUEST_LINE: u64 = 16 * 1024;

// Gives a result only for a redirect of this login. Another request gets
// `404` or `400`, and the wait continues.
async fn redirect_of(
    mut stream: tokio::net::TcpStream,
    redirect: &RedirectUri,
    expected: &callback::Expected,
) -> Option<Result<callback::Callback, callback::CallbackError>> {
    let mut line = String::new();
    let mut reader = BufReader::new(&mut stream).take(MAX_REQUEST_LINE);
    tokio::time::timeout(REQUEST_LINE_LIMIT, reader.read_line(&mut line))
        .await
        .ok()?
        .ok()?;
    let target = line.split_whitespace().nth(1)?.to_string();
    if !target.starts_with("/callback") {
        let _ = stream.write_all(NOT_FOUND).await;
        return None;
    }
    let url = format!("http://127.0.0.1:{}{target}", redirect.port());
    let result = callback::parse(&url, redirect, expected);
    match &result {
        Ok(_) => {
            let message = "The login is complete. You can close this tab.";
            let _ = stream.write_all(page("200 OK", message).as_bytes()).await;
        }
        Err(error) if error.is_for_this_login() => {
            let message = format!(
                "The login failed. {}.",
                crate::text::chain_text(error).trim_end_matches('.')
            );
            let _ = stream.write_all(page("200 OK", &message).as_bytes()).await;
        }
        Err(error) => {
            let message =
                format!("This redirect is not for the login that ncp waits for. {error}.");
            let _ = stream
                .write_all(page("400 Bad Request", &message).as_bytes())
                .await;
            return None;
        }
    }
    Some(result)
}

const NOT_FOUND: &[u8] =
    b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";

// Each connection gets its own task, so that a slow connection cannot
// block another one.
async fn from_listener(
    listener: TcpListener,
    redirect: &RedirectUri,
    expected: &callback::Expected,
) -> Result<callback::Callback, callback::CallbackError> {
    let (sender, mut found) = tokio::sync::mpsc::channel(1);
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else {
                    // For example too many open files: wait, then go on.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                };
                let (sender, redirect, expected) = (sender.clone(), redirect.clone(), expected.clone());
                tasks.spawn(async move {
                    if let Some(result) = redirect_of(stream, &redirect, &expected).await {
                        let _ = sender.send(result).await;
                    }
                });
            }
            Some(result) = found.recv() => return result,
        }
    }
}

// The read is on its own thread, because a blocking read on the runtime delays
// the exit of `ncp` by the shutdown limit.
async fn pasted_line() -> Option<String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let line = std::io::stdin()
            .lines()
            .map_while(Result::ok)
            .find(|line| !line.trim().is_empty());
        let _ = sender.send(line);
    });
    receiver.await.ok().flatten()
}

// A pasted URL is for this login, so each error in it ends the login.
async fn wait_for_redirect(
    listener: TcpListener,
    redirect: &RedirectUri,
    expected: &callback::Expected,
    read_stdin: bool,
    server: &str,
) -> Result<callback::Callback, Failure> {
    let from_stdin = async {
        if !read_stdin {
            return std::future::pending().await;
        }
        match pasted_line().await {
            Some(line) => callback::parse(&line, redirect, expected),
            // The end of stdin: only the listener can answer now.
            None => std::future::pending().await,
        }
    };
    // The limit and Ctrl-C of the whole login apply.
    let result = tokio::select! {
        result = from_listener(listener, redirect, expected) => result,
        result = from_stdin => result,
    };
    result.map_err(|error| {
        Failure::new(
            Exit::Auth,
            format!("The login to the server {} failed", code(server)),
        )
        .about(server)
        .caused_by(&error)
    })
}

async fn logout(context: &Context<'_>, word: &str) -> Result<(), Failure> {
    let loaded = session::load(context, Some(word)).await?;
    let server = session::select(&loaded, word, context.ui)?;
    let name = server.name.as_str();
    remote(server)?;
    let key = CredentialKey::OAuthTokens(server.name.clone());
    // A refresh in another process must not bring the tokens back.
    let _lock = hold_tokens_lock(context, &server.name).await?;
    let removed = remove_everywhere(context, name, &key).await?;
    crate::list_cache::remove(&crate::list_cache::path(
        &context.paths.cache_dir,
        &server.name,
    ));
    let places: Vec<&str> = removed
        .iter()
        .map(|place| secrets::place_name(*place))
        .collect();
    context.ui.result(
        || match removed.as_slice() {
            [] => format!("ncp found no stored tokens of {name}.\n"),
            places => {
                let names: Vec<&str> = places.iter().map(|p| secrets::place_text(*p)).collect();
                format!(
                    "Deleted the tokens of {name} from {}.\n",
                    names.join(" and ")
                )
            }
        },
        || json!({"server": name, "credentialStores": places}),
    )
}

/// What `server remove` did with the OAuth items of a removed entry.
#[derive(Debug, Default)]
pub(super) struct TokenRemoval {
    /// The places that had the tokens of the entry.
    pub removed: Vec<CredentialPlace>,
    /// A place keeps tokens of the name that are for another server, or
    /// that `ncp` cannot compare.
    pub kept: bool,
    /// The error of a Ctrl-C that stopped the work before `ncp` checked
    /// each place.
    pub interrupted: Option<Failure>,
}

enum Found {
    Nothing,
    Removed,
    Kept,
}

/// The server name is the key of the tokens. Thus the tokens go only when
/// their binding is the URL of the entry (`resource`). When the entry
/// names an issuer, the issuer must also match. With no URL to compare,
/// the tokens stay. The client item goes with them when it is for the
/// issuer of the tokens.
///
/// A store that cannot answer gives only a warning. The remove of the
/// entry must not depend on the store, and the binding stops the use of
/// the old tokens. Ctrl-C stops the work, and the tokens that are not
/// deleted yet stay.
pub(super) async fn remove_bound_items(
    context: &Context<'_>,
    server: &ServerName,
    resource: Option<&str>,
    issuer: Option<&str>,
) -> TokenRemoval {
    let _lock = match hold_tokens_lock(context, server).await {
        Ok(lock) => lock,
        Err(failure) => {
            return TokenRemoval {
                interrupted: Some(failure),
                ..TokenRemoval::default()
            };
        }
    };
    let mut removal = TokenRemoval::default();
    for (place, opened) in secrets::places(context).await {
        let found = match opened {
            Ok(store) => remove_bound(context, &store, server, resource, issuer).await,
            Err(crate::store::StoreError::Unavailable(_)) => Ok(Found::Nothing),
            Err(error) => Err(error),
        };
        match found {
            Ok(Found::Nothing) => {}
            Ok(Found::Removed) => removal.removed.push(place),
            Ok(Found::Kept) => removal.kept = true,
            Err(crate::store::StoreError::Interrupted) => {
                return TokenRemoval {
                    interrupted: Some(session::interrupted(Some(server.as_str()))),
                    ..removal
                };
            }
            Err(error) => context.ui.warn(
                &Warning::new(format!(
                    "ncp cannot check {} for tokens of the server {}, so the tokens stay",
                    secrets::place_text(place),
                    code(server.as_str())
                ))
                .about(server.as_str())
                .caused_by(&error),
            ),
        }
    }
    removal
}

async fn remove_bound(
    context: &Context<'_>,
    store: &Arc<dyn CredentialStore>,
    server: &ServerName,
    resource: Option<&str>,
    issuer: Option<&str>,
) -> Result<Found, crate::store::StoreError> {
    let tokens_key = CredentialKey::OAuthTokens(server.clone());
    let wanted = tokens_key.clone();
    let Some(secret) = secrets::call(context, store, move |store| store.load(&wanted)).await?
    else {
        return Ok(Found::Nothing);
    };
    let binding = parse_item(&secret).ok().and_then(|item| item.binding);
    let bound = binding.as_ref().filter(|binding| {
        resource == Some(binding.resource.as_str())
            && issuer.is_none_or(|issuer| issuer == binding.issuer)
    });
    let Some(binding) = bound else {
        return Ok(Found::Kept);
    };
    secrets::call(context, store, move |store| store.delete(&tokens_key)).await?;
    let client_key = CredentialKey::OAuthClient(server.clone());
    let wanted = client_key.clone();
    let client = secrets::call(context, store, move |store| store.load(&wanted)).await?;
    if client
        .and_then(|secret| ClientItem::from_secret(&secret).ok())
        .is_some_and(|client| client.is_for(&binding.issuer))
    {
        secrets::call(context, store, move |store| store.delete(&client_key)).await?;
    }
    Ok(Found::Removed)
}

// A native store that cannot answer gives only a warning, so that the
// file copy still goes away.
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
                Err(error) => return Err(delete_failure(name, CredentialPlace::Native, &error)),
            }
        }
        Err(crate::store::StoreError::Unavailable(_)) => {}
        Err(crate::store::StoreError::Interrupted) => {
            return Err(session::interrupted(Some(name)));
        }
        Err(error) => context.ui.warn(&Warning::from(secrets::store_failure_about(
            Some(name),
            &error,
            format!(
                "ncp cannot open the native store to delete the tokens of the server {}",
                code(name)
            ),
            [],
        ))),
    }
    let file = secrets::file(context);
    let key = key.clone();
    if secrets::call(context, &file, move |store| store.delete(&key))
        .await
        .map_err(|error| delete_failure(name, CredentialPlace::File, &error))?
    {
        removed.push(CredentialPlace::File);
    }
    Ok(removed)
}

fn delete_failure(name: &str, place: CredentialPlace, error: &crate::store::StoreError) -> Failure {
    let message = format!(
        "ncp cannot delete the tokens of the server {} from {}",
        code(name),
        secrets::place_text(place)
    );
    secrets::store_failure_about(Some(name), error, message, [])
}

fn not_json(name: &str) -> Failure {
    Failure::protocol(format!(
        "The stored OAuth tokens of the server {} are not JSON",
        code(name)
    ))
    .about(name)
}

async fn status(context: &Context<'_>, word: Option<&str>) -> Result<(), Failure> {
    // No question: auth status only reads.
    let loaded = session::load(context, None).await?;
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
    let mut places = TokenPlaces::open_for_status(context).await;
    let mut rows = Vec::new();
    for server in servers {
        let tokens = match places.find(context, server).await? {
            Some((_, secret)) => Some(serde_json::from_str::<Value>(secret.expose()).map_err(
                |error| not_json(server.name.as_str()).caused_by(&items::ItemReason::of(&error)),
            )?),
            None => None,
        };
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

fn status_text(rows: &[(String, items::Status)]) -> String {
    if rows.is_empty() {
        return "No HTTP server is configured.\n".to_string();
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

    #[test]
    fn binding_stop_names_no_stored_url_or_issuer() {
        let env = crate::expand::Env::from([("KEY".to_string(), "n3w-k3y".to_string())]);
        let config = crate::config::parse(
            r#"{"mcpServers": {"web": {"type": "http", "url": "https://h/${KEY}/mcp"}}}"#,
            &env,
        )
        .unwrap();
        let server = config.server("web").unwrap();
        let resource = binding_failure(server, &items::BindingMismatch::Resource);
        assert!(
            resource.message().contains("did not send")
                && resource.message().contains("on purpose"),
            "{}",
            resource.message()
        );
        let issuer = binding_failure(
            server,
            &items::BindingMismatch::Issuer {
                current: "https://as/n3w-k3y".into(),
            },
        );
        for failure in [&resource, &issuer] {
            assert!(!failure.message().contains("k3y"), "{}", failure.message());
            assert!(failure.message().contains("did not send"));
            assert!(failure.message().contains("on purpose"));
        }
    }

    #[test]
    fn published_metadata_document_matches_the_client_id_in_code() {
        let document: Value = serde_json::from_str(include_str!(
            "../../../../site/public/oauth/client-metadata.json"
        ))
        .unwrap();
        assert_eq!(document["client_id"], CLIENT_METADATA_URL);
        // A loopback redirect URI matches with any port (RFC 8252 §7.3).
        let mut redirect = url::Url::parse(&RedirectUri::new(0).to_string()).unwrap();
        redirect.set_port(None).unwrap();
        assert_eq!(document["redirect_uris"], json!([redirect.as_str()]));
    }

    #[test]
    fn metadata_document_support_needs_the_value_true() {
        let with = |value: Value| {
            let mut metadata = metadata("https://as.test");
            metadata
                .additional_fields
                .insert("client_id_metadata_document_supported".into(), value);
            supports_metadata_document(&metadata)
        };
        assert!(with(json!(true)));
        assert!(!with(json!(false)));
        assert!(!with(json!("true")));
        assert!(!supports_metadata_document(&metadata("https://as.test")));
    }

    fn metadata(endpoint: &str) -> AuthorizationMetadata {
        let mut metadata = AuthorizationMetadata::default();
        metadata.authorization_endpoint = format!("{endpoint}/authorize");
        metadata.token_endpoint = format!("{endpoint}/token");
        metadata
    }

    #[test]
    fn authorization_server_must_use_https_except_on_loopback() {
        assert!(check_https("web", &metadata("https://as.test")).is_ok());
        assert!(check_https("web", &metadata("http://127.0.0.1:9")).is_ok());
        assert!(check_https("web", &metadata("http://localhost:9")).is_ok());
        let failure = check_https("web", &metadata("http://as.test")).unwrap_err();
        assert!(
            failure.message().contains("http://as.test/authorize"),
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
