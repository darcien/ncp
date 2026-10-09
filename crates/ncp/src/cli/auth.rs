//! The `auth` commands: OAuth login, logout and status (design section 5).
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
        store::{TokenStore, parse_item},
    },
    server::ServerName,
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
        } => {
            let limit = context.request_limit;
            let timeout = Failure::protocol(format!(
                "the login did not complete in {} s",
                limit.as_secs()
            ))
            .hint("give a longer limit with --timeout <SECONDS>");
            let name = limited(
                context,
                limit,
                timeout,
                Box::pin(login(context, &server, no_browser, credential_store)),
            )
            .await?;
            // After the limit of the login: the tokens are saved, and the
            // cleanup has its own short limit.
            remove_old_tokens(context, &name, credential_store).await
        }
        AuthCommand::Logout { server } => logout(context, &server).await,
        AuthCommand::Status { server } => status(context, server.as_deref()).await,
    }
}

/// Runs the OAuth steps until they end, the limit passes, or Ctrl-C. Each
/// step of rmcp has its own limit of 30 s, and discovery tries up to eight
/// URLs, so the steps together need one limit (design C7).
pub(super) async fn limited<T>(
    context: &Context<'_>,
    limit: Duration,
    timeout: Failure,
    work: impl Future<Output = Result<T, Failure>>,
) -> Result<T, Failure> {
    tokio::select! {
        result = work => result,
        () = context.cancel.cancelled() => Err(session::interrupted()),
        () = tokio::time::sleep(limit) => Err(timeout),
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

/// The metadata of the authorization server, from the challenge or the
/// well-known URLs. rmcp falls back to endpoints that it makes from the
/// server URL; the spec has that fallback only for 2025-03-26, and it
/// registers a client at a server that names no authorization server.
async fn discover(
    manager: &AuthorizationManager,
    challenge: Option<&str>,
) -> Result<AuthorizationMetadata, Failure> {
    let resolution = manager
        .resolve_metadata_from_challenge(challenge)
        .await
        .map_err(|error| auth_failure("OAuth discovery", &error))?;
    if !resolution.source.is_discovered() {
        return Err(Failure::protocol(
            "the server names no authorization server: it has no protected resource metadata and no authorization server metadata",
        ));
    }
    check_https(&resolution.metadata)?;
    Ok(resolution.metadata)
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
) -> Result<ServerName, Failure> {
    let loaded = session::load_config(&context.paths.config_file, context.env)?;
    let server = session::select(&loaded, word, context.ui)?;
    let name = server.name.as_str();
    let remote = remote(server)?;
    let store = place_store(context, name, place).await?;
    let observer = diagnostics::observer(
        context.ui,
        name,
        context.timings.clone(),
        session::shown_url(server),
    );
    let http = OAuthHttp::new(context.env, &observer)
        .map_err(|error| Failure::protocol(error.to_string()).server(name))?;
    let mut manager = AuthorizationManager::new_with_oauth_http_client(
        remote.url.without_query(),
        Arc::new(http),
    )
    .await
    .map_err(|error| auth_failure("OAuth setup", &error).server(name))?;
    let challenge = session::challenge(context, server).await?;
    let metadata = discover(&manager, challenge.as_deref())
        .await
        .map_err(|failure| failure.server(name))?;
    let issuer = issuer_of(&metadata);
    context
        .ui
        .debug(Some(name), &format!("authorization server: {issuer}"));
    manager.set_metadata(metadata);
    manager.set_credential_store(TokenStore::new(server.name.clone(), store.clone()).fresh());
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
        (&store, place),
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
    let callback = wait_for_redirect(listener, &redirect, &state, read_stdin)
        .await
        .map_err(|failure| failure.server(name))?;
    // A refresh of the old tokens in another process must not save over
    // the new tokens.
    let _lock = hold_tokens_lock(context, &server.name).await;
    session
        .handle_callback_with_issuer(&callback.code, &callback.state, callback.iss.as_deref())
        .await
        .map_err(|error| token_failure(&error, name, place))?;
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
                "place": secrets::place_name(place)})
        },
    )?;
    Ok(server.name.clone())
}

/// The lock file of the refresh of one server, next to the credential
/// file. It is empty; the lock is the only content.
fn refresh_lock(context: &Context<'_>, server: &ServerName) -> std::path::PathBuf {
    context
        .paths
        .credential_file
        .with_file_name(format!("oauth-{}.lock", server.as_str()))
}

/// The lock of the tokens of the server, or `None` when the lock file
/// cannot be made: the lock only orders a refresh, a login and a logout.
async fn hold_tokens_lock(
    context: &Context<'_>,
    server: &ServerName,
) -> Option<crate::private_file::Lock> {
    let path = refresh_lock(context, server);
    let locked = crate::store::run_blocking(crate::store::CALL_LIMIT, move || {
        Ok(crate::oauth::store::lock_tokens(&path))
    })
    .await;
    match locked {
        Ok(Ok(lock)) => Some(lock),
        // Another process holds the lock past its limit: go on, but say so,
        // because that process can undo this change.
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::TimedOut => {
            context
                .ui
                .warning(Some(server.as_str()), &error.to_string());
            None
        }
        _ => None,
    }
}

/// The failure of the token request of a login. A store that cannot keep
/// the tokens has its own fix: a file that fails is a local error, and a
/// native store that fails (for example a token set that is too long for
/// the Keychain through `security`) can give way to the file.
fn token_failure(error: &AuthError, name: &str, place: CredentialPlace) -> Failure {
    match error {
        AuthError::CredentialStoreError(text) => match crate::oauth::store::file_failure(text) {
            Some(text) => Failure::protocol(format!("cannot keep the tokens: {text}")).server(name),
            None if place == CredentialPlace::Native => {
                Failure::new(Exit::Auth, format!("cannot keep the tokens: {error}"))
                    .hint(format!("ncp auth login {name} --credential-store file"))
                    .server(name)
            }
            None => auth_failure("OAuth token request", error).server(name),
        },
        error => auth_failure("OAuth token request", error).server(name),
    }
}

/// The inputs of [`stored_login`].
#[derive(Clone, Copy)]
pub(super) struct StoredLogin<'a> {
    pub server: &'a Server,
    pub remote: &'a RemoteServer,
    pub http: &'a reqwest::Client,
    pub observer: &'a crate::client::observe::Observer,
    /// The `WWW-Authenticate` challenge of the `401` answer.
    pub challenge: Option<&'a str>,
}

/// The OAuth client with the stored tokens of the server, or `None` when
/// the tokens are for another issuer. `ncp`
/// calls this only after the server answered `401`, so that a server with
/// no login never opens the native store (design C1).
///
/// # Errors
///
/// Returns a failure when a store cannot answer or when discovery fails.
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
        challenge,
    } = *login;
    let name = server.name.as_str();
    let (store, secret) = found;
    let item =
        parse_item(&secret).map_err(|error| Failure::protocol(error.to_string()).server(name))?;
    let client_id = item.tokens.client_id.clone();
    context.ui.debug(Some(name), "read the stored OAuth tokens");
    let oauth_http = OAuthHttp::new(context.env, observer)
        .map_err(|error| Failure::protocol(error.to_string()).server(name))?;
    let mut manager = AuthorizationManager::new_with_oauth_http_client(
        remote.url.without_query(),
        Arc::new(oauth_http),
    )
    .await
    .map_err(|error| auth_failure("OAuth setup", &error).server(name))?;
    manager.set_credential_store(
        TokenStore::new(server.name.clone(), store.clone())
            .with_item(item)
            .with_refresh_lock(refresh_lock(context, &server.name))
            .with_timer(credentials_timer(context)),
    );
    // The challenge can name the protected resource metadata (RFC 9728
    // §5.1). With metadata in place, rmcp does no discovery of its own.
    let metadata = discover(&manager, challenge)
        .await
        .map_err(|failure| failure.server(name))?;
    manager.set_metadata(metadata);
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
    let started = std::time::Instant::now();
    let client = secrets::call(context, &store, move |store| store.load(&client_key)).await;
    if let Some(timer) = credentials_timer(context) {
        timer(started.elapsed());
    }
    let client = client
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

/// Adds the time of a call to the store to `credentialsMs`, with
/// `--timings`: a refresh reads and writes the tokens again.
fn credentials_timer(context: &Context<'_>) -> Option<crate::oauth::store::StoreTimer> {
    let timings = context.timings.clone()?;
    Some(Arc::new(move |elapsed| {
        timings.update(|record| super::timings::add(&mut record.credentials, elapsed));
    }))
}

/// Keeps the scopes that the server asked for in a `403` answer in the
/// tokens item, so that the next `auth login` asks for them with the
/// scopes that the user has, as the spec asks for a step-up. A failure gives a warning:
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
    if let Some((store, secret)) = find_tokens(context, server).await? {
        let mut tokens: Value = serde_json::from_str(secret.expose()).map_err(|error| {
            Failure::protocol(format!("the stored tokens are not JSON: {error}"))
        })?;
        let required = union(&scope_list(&tokens[items::REQUIRED_SCOPES]), scopes);
        tokens[items::REQUIRED_SCOPES] = json!(required);
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
    }
    Ok(())
}

/// The tokens item of a server, and the store that has it.
pub(super) type FoundTokens = (Arc<dyn CredentialStore>, crate::credential::Secret);

/// The tokens item of the server from the first place that has it: the
/// native store, then the file, as for `${NAME}` (design 2.4). A host with
/// no native store has only the file. A native store that cannot answer,
/// for example a locked keyring, does not stop the search; its error is
/// the result only when the file has no tokens, so that the error says
/// why. The time goes to the credentials of `--timings`.
///
/// # Errors
///
/// Returns a failure when the file cannot be read, when the native store
/// fails for one item, or when it cannot answer and the file has no
/// tokens.
pub(super) async fn find_tokens(
    context: &Context<'_>,
    server: &Server,
) -> Result<Option<FoundTokens>, Failure> {
    TokenPlaces::open(context).await.find(context, server).await
}

/// The places of the tokens for one command. A native store that cannot
/// answer is kept as such, so that the search for the next server does
/// not wait for the store limit again.
pub(super) struct TokenPlaces {
    native: Option<Arc<dyn CredentialStore>>,
    down: Option<crate::store::StoreError>,
    file: Arc<dyn CredentialStore>,
}

impl TokenPlaces {
    pub(super) async fn open(context: &Context<'_>) -> Self {
        let (native, down) = match secrets::native(context).await {
            Ok(native) => (Some(native), None),
            Err(crate::store::StoreError::Unavailable(_)) => (None, None),
            Err(error) => (None, Some(error)),
        };
        Self {
            native,
            down,
            file: secrets::file(context),
        }
    }

    /// The tokens of the server, timed for the credentials of `--timings`.
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
        found.map_err(|failure| failure.server(server.name.as_str()))
    }

    async fn search(
        &mut self,
        context: &Context<'_>,
        server: &Server,
    ) -> Result<Option<FoundTokens>, Failure> {
        let key = CredentialKey::OAuthTokens(server.name.clone());
        if let Some(native) = self.native.clone() {
            let wanted = key.clone();
            match secrets::call(context, &native, move |store| store.load(&wanted)).await {
                Ok(Some(secret)) => return Ok(Some((native, secret))),
                Ok(None) => {}
                Err(crate::store::StoreError::Interrupted) => return Err(session::interrupted()),
                Err(error) if secrets::store_is_down(&error) => {
                    self.native = None;
                    self.down = Some(error);
                }
                Err(error) => return Err(secrets::store_failure(&error)),
            }
        }
        let file = self.file.clone();
        let found = secrets::call(context, &file, move |store| store.load(&key))
            .await
            .map_err(|error| secrets::store_failure(&error))?;
        match (found, &self.down) {
            (Some(secret), _) => Ok(Some((file, secret))),
            (None, Some(error)) => Err(secrets::store_failure(error).hint(format!(
                "ncp auth login {} --credential-store file",
                super::render::shell_word(server.name.as_str())
            ))),
            (None, None) => Ok(None),
        }
    }
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
/// login. A step-up asks for the old and the new scopes.
async fn login_scopes(
    context: &Context<'_>,
    server: &Server,
    chosen: (&Arc<dyn CredentialStore>, CredentialPlace),
    selected: &[String],
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
    let stored = found
        .and_then(|secret| serde_json::from_str::<Value>(secret.expose()).ok())
        .unwrap_or(Value::Null);
    let held = union(
        &scope_list(&stored["granted_scopes"]),
        &scope_list(&stored[items::REQUIRED_SCOPES]),
    );
    union(selected, &held)
}

/// The limit of the cleanup after a login. It is a cleanup, so it must
/// not wait for an unlock prompt of a store that the user did not choose.
const CLEANUP_LIMIT: Duration = Duration::from_secs(5);

/// The other place than the one that the user chose. The native store
/// gets the short limit: the user did not choose it.
async fn other_place(
    context: &Context<'_>,
    chosen: CredentialPlace,
) -> Result<Arc<dyn CredentialStore>, crate::store::StoreError> {
    match chosen {
        CredentialPlace::Native => Ok(secrets::file(context)),
        CredentialPlace::File => secrets::open_within(context, CLEANUP_LIMIT).await,
    }
}

/// Removes the tokens of the other place after a login. Calls read the
/// native store first, so an old item there would hide the new tokens in
/// the file. A failure gives a warning: the login worked. Ctrl-C gives
/// 130, as at each other step.
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
            .debug(Some(name), "removed the old tokens of the other place"),
        Ok(false) => {}
        Err(crate::store::StoreError::Interrupted) => {
            return Err(session::interrupted().server(name));
        }
        Err(error) => context.ui.warning(
            Some(name),
            &format!("cannot remove the old tokens of the other place: {error}"),
        ),
    }
    Ok(())
}

/// The client for the login, in the order of the spec:
/// the `oauth.clientId` of the configuration, then the stored client of
/// the same issuer, then a new registration. A Client ID Metadata Document
/// waits for a release decision: it needs a stable HTTPS URL.
async fn client_request(
    context: &Context<'_>,
    server: &Server,
    remote: &RemoteServer,
    manager: &mut AuthorizationManager,
    (store, place): (&Arc<dyn CredentialStore>, CredentialPlace),
    issuer: &str,
    redirect: &RedirectUri,
) -> Result<AuthorizationRequest, Failure> {
    let selected = manager.select_scopes(None, &[]);
    let scopes = login_scopes(context, server, (store, place), &selected).await;
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

/// The time that one connection on the listener gets to send its request
/// line, and the longest line. A connection that sends nothing must not
/// block the real redirect.
const REQUEST_LINE_LIMIT: Duration = Duration::from_secs(5);
const MAX_REQUEST_LINE: u64 = 16 * 1024;

/// Reads the request line of one connection, and answers it. Gives the
/// result only for a redirect of this login; another request gets `404`
/// or `400`, and the wait continues.
async fn redirect_of(
    mut stream: tokio::net::TcpStream,
    redirect: &RedirectUri,
    state: &str,
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
    let result = callback::parse(&url, redirect, state);
    match &result {
        Ok(_) => {
            let message = "The login is complete. You can close this tab.";
            let _ = stream.write_all(page("200 OK", message).as_bytes()).await;
        }
        Err(error) if error.is_for_this_login() => {
            let message = format!("The login failed: {error}.");
            let _ = stream.write_all(page("200 OK", &message).as_bytes()).await;
        }
        Err(error) => {
            let message = format!("This redirect is not for the login that waits: {error}.");
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

/// The first redirect of this login on the listener. Each connection gets
/// its own task, so that a slow connection cannot block another one.
async fn from_listener(
    listener: TcpListener,
    redirect: &RedirectUri,
    state: &str,
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
                let (sender, redirect, state) = (sender.clone(), redirect.clone(), state.to_string());
                tasks.spawn(async move {
                    if let Some(result) = redirect_of(stream, &redirect, &state).await {
                        let _ = sender.send(result).await;
                    }
                });
            }
            Some(result) = found.recv() => return result,
        }
    }
}

/// The first non-empty line of stdin, read on its own thread. A blocking
/// read on the runtime would delay the exit of `ncp` by the shutdown limit.
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

/// Waits for the redirect: on the listener, or pasted on stdin. A pasted
/// URL is for this login, so each error of it ends the login.
async fn wait_for_redirect(
    listener: TcpListener,
    redirect: &RedirectUri,
    state: &str,
    read_stdin: bool,
) -> Result<callback::Callback, Failure> {
    let from_stdin = async {
        if !read_stdin {
            return std::future::pending().await;
        }
        match pasted_line().await {
            Some(line) => callback::parse(&line, redirect, state),
            // The end of stdin: only the listener can answer now.
            None => std::future::pending().await,
        }
    };
    // The limit and Ctrl-C of the whole login apply.
    let result = tokio::select! {
        result = from_listener(listener, redirect, state) => result,
        result = from_stdin => result,
    };
    result.map_err(|error| Failure::new(Exit::Auth, error.to_string()))
}

async fn logout(context: &Context<'_>, word: &str) -> Result<(), Failure> {
    let loaded = session::load_config(&context.paths.config_file, context.env)?;
    let server = session::select(&loaded, word, context.ui)?;
    let name = server.name.as_str();
    remote(server)?;
    let key = CredentialKey::OAuthTokens(server.name.clone());
    // A refresh in another process must not bring the tokens back.
    let _lock = hold_tokens_lock(context, &server.name).await;
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
    let mut places = TokenPlaces::open(context).await;
    let mut rows = Vec::new();
    for server in servers {
        let tokens = match places.find(context, server).await? {
            Some((_, secret)) => Some(serde_json::from_str::<Value>(secret.expose()).map_err(
                |error| Failure::protocol(format!("the stored tokens are not JSON: {error}")),
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
