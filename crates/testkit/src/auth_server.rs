//! A local OAuth authorization server for the tests of `ncp`. Tests never
//! call a third-party server.
//!
//! The server serves:
//! - the authorization server metadata (RFC 8414)
//! - an authorization endpoint that approves each valid request at once
//! - a token endpoint with PKCE S256 (RFC 7636) and the refresh grant
//! - Dynamic Client Registration (RFC 7591)
//!
//! The issuer is a loopback `http` URL.
//!
//! [`AuthServer::settings`] changes the behavior while the server runs, and
//! [`AuthServer::record`] shows what the server got.

use std::{
    collections::HashMap,
    convert::Infallible,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use base64::Engine as _;
use bytes::Bytes;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

pub type Response = http::Response<BoxBody<Bytes, Infallible>>;

/// The `iss` parameter in the redirect to the client (RFC 9207).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackIss {
    /// The issuer of the server.
    Correct,
    Absent,
    /// An issuer that is not the issuer of the server.
    Wrong,
}

/// The answer at the authorization endpoint. The server asks no person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approve,
    /// A redirect with `error=access_denied`.
    Deny,
}

/// Dynamic Client Registration (RFC 7591).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// A new client has no secret.
    Public,
    /// A new client gets a client secret.
    WithSecret,
    /// The metadata names no registration endpoint, and the endpoint
    /// registers nothing.
    Closed,
}

/// The behavior of the server. Tests can change it while the server runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSettings {
    pub callback_iss: CallbackIss,
    /// The metadata field `authorization_response_iss_parameter_supported`.
    pub announce_iss: bool,
    /// The answer of the user at the authorization endpoint.
    pub decision: Decision,
    /// `expires_in` of each access token, in seconds.
    pub expires_in: u64,
    /// The refresh grant fails with `invalid_grant`.
    pub reject_refresh: bool,
    pub registration: Registration,
    /// The path of the issuer after the host, for example `/b`. A new
    /// path makes a new issuer on the same server.
    pub issuer_path: String,
    pub scopes_supported: Vec<String>,
    /// The protected resource metadata of the MCP fixture names
    /// `scopes_supported`.
    pub resource_scopes: bool,
    /// The `scope` of the `401` challenge of the MCP fixture.
    pub challenge_scope: Option<String>,
    /// Scopes that the user grants on top of the request, as a consent
    /// page with a choice of permissions can.
    pub granted_extra: Vec<String>,
    /// The metadata field `service_documentation`.
    pub service_documentation: Option<String>,
}

impl Default for AuthSettings {
    fn default() -> Self {
        Self {
            callback_iss: CallbackIss::Correct,
            announce_iss: true,
            decision: Decision::Approve,
            expires_in: 3600,
            reject_refresh: false,
            registration: Registration::Public,
            issuer_path: String::new(),
            scopes_supported: vec!["a".into(), "b".into()],
            resource_scopes: true,
            challenge_scope: None,
            granted_extra: Vec::new(),
            service_documentation: None,
        }
    }
}

/// What the server got.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AuthRecord {
    pub metadata_calls: usize,
    pub authorize_calls: usize,
    pub code_grants: usize,
    pub refresh_grants: usize,
    pub register_calls: usize,
    /// The body of each registration request.
    pub registrations: Vec<Value>,
    /// The `resource` of the last authorization request.
    pub authorize_resource: Option<String>,
    /// The `scope` of the last authorization request.
    pub authorize_scope: Option<String>,
    /// The `resource` of the last token request.
    pub token_resource: Option<String>,
    /// The `client_id` of the last token request.
    pub token_client_id: Option<String>,
    /// The last token request sent its client with HTTP Basic.
    pub token_basic: bool,
}

#[derive(Debug, Clone)]
struct Client {
    secret: Option<String>,
    redirect_uris: Vec<String>,
}

#[derive(Debug, Clone)]
struct Code {
    client_id: String,
    redirect_uri: String,
    challenge: String,
    scope: String,
}

#[derive(Debug, Clone)]
struct AccessToken {
    scope: String,
    expires: Instant,
}

#[derive(Debug, Clone)]
struct RefreshToken {
    client_id: String,
    scope: String,
}

#[derive(Debug, Default)]
struct State {
    settings: AuthSettings,
    record: AuthRecord,
    clients: HashMap<String, Client>,
    codes: HashMap<String, Code>,
    access_tokens: HashMap<String, AccessToken>,
    refresh_tokens: HashMap<String, RefreshToken>,
    counter: usize,
}

impl State {
    fn next(&mut self) -> usize {
        self.counter += 1;
        self.counter
    }
}

/// The shared state of an authorization server. The MCP fixture reads the
/// access tokens from it.
#[derive(Debug, Clone)]
pub struct AuthState {
    origin: String,
    state: Arc<Mutex<State>>,
}

impl AuthState {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The issuer: the origin and the issuer path.
    #[must_use]
    pub fn issuer(&self) -> String {
        format!("{}{}", self.origin, self.lock().settings.issuer_path)
    }

    #[must_use]
    pub fn scopes_supported(&self) -> Vec<String> {
        self.lock().settings.scopes_supported.clone()
    }

    /// The `scopes_supported` of the protected resource metadata of the MCP
    /// fixture: the scopes of this server, or none.
    #[must_use]
    pub fn resource_scopes(&self) -> Option<Vec<String>> {
        let guard = self.lock();
        guard
            .settings
            .resource_scopes
            .then(|| guard.settings.scopes_supported.clone())
    }

    /// The `scope` of the `401` challenge of the MCP fixture.
    #[must_use]
    pub fn challenge_scope(&self) -> Option<String> {
        self.lock().settings.challenge_scope.clone()
    }

    /// The scopes of a valid access token, or `None` for an unknown or an
    /// expired token.
    #[must_use]
    pub fn token_scopes(&self, token: &str) -> Option<Vec<String>> {
        self.lock()
            .access_tokens
            .get(token)
            .filter(|token| Instant::now() < token.expires)
            .map(|token| token.scope.split_whitespace().map(String::from).collect())
    }
}

/// An authorization server on a free loopback port. It stops at the drop of the
/// value.
pub struct AuthServer {
    pub state: AuthState,
    stop: CancellationToken,
}

impl Drop for AuthServer {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

impl AuthServer {
    #[must_use]
    pub fn issuer(&self) -> String {
        self.state.issuer()
    }

    #[must_use]
    pub fn record(&self) -> AuthRecord {
        self.state.lock().record.clone()
    }

    pub fn settings(&self, change: impl FnOnce(&mut AuthSettings)) {
        change(&mut self.state.lock().settings);
    }

    /// Adds a pre-registered client, with no check of its redirect URI.
    pub fn add_client(&self, client_id: &str, secret: Option<&str>) {
        self.state.lock().clients.insert(
            client_id.to_string(),
            Client {
                secret: secret.map(String::from),
                redirect_uris: Vec::new(),
            },
        );
    }
}

/// # Errors
///
/// Returns an error when no loopback port is free.
pub async fn serve_auth(settings: AuthSettings) -> std::io::Result<AuthServer> {
    serve_auth_with(settings, None).await
}

/// The authorization server over HTTPS, with the certificate of the test
/// CA ([`crate::TEST_CA`]).
///
/// # Errors
///
/// Returns an error when no loopback port is free.
pub async fn serve_auth_tls(settings: AuthSettings) -> std::io::Result<AuthServer> {
    serve_auth_with(settings, Some(crate::test_tls()?)).await
}

async fn serve_auth_with(
    settings: AuthSettings,
    tls: Option<tokio_rustls::TlsAcceptor>,
) -> std::io::Result<AuthServer> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let scheme = if tls.is_some() { "https" } else { "http" };
    let state = AuthState {
        origin: format!("{scheme}://{}", listener.local_addr()?),
        state: Arc::new(Mutex::new(State {
            settings,
            ..State::default()
        })),
    };
    let stop = CancellationToken::new();
    let (shared, accept_stop) = (state.clone(), stop.clone());
    tokio::spawn(async move {
        loop {
            let stream = tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok((stream, _)) => stream,
                    Err(_) => return,
                },
                () = accept_stop.cancelled() => return,
            };
            let state = shared.clone();
            let tls = tls.clone();
            tokio::spawn(async move {
                let handler = hyper::service::service_fn(move |request| {
                    let state = state.clone();
                    async move { Ok::<_, Infallible>(handle(&state, request).await) }
                });
                let builder = hyper::server::conn::http1::Builder::new();
                match tls {
                    Some(tls) => {
                        if let Ok(stream) = tls.accept(stream).await {
                            let _ = builder
                                .serve_connection(hyper_util::rt::TokioIo::new(stream), handler)
                                .await;
                        }
                    }
                    None => {
                        let _ = builder
                            .serve_connection(hyper_util::rt::TokioIo::new(stream), handler)
                            .await;
                    }
                }
            });
        }
    });
    Ok(AuthServer { state, stop })
}

async fn handle(state: &AuthState, request: http::Request<hyper::body::Incoming>) -> Response {
    let (parts, body) = request.into_parts();
    let body = body
        .collect()
        .await
        .map(http_body_util::Collected::to_bytes)
        .unwrap_or_default();
    let path = parts.uri.path().to_string();
    let query = parts.uri.query().unwrap_or_default().to_string();
    let issuer_path = state.lock().settings.issuer_path.clone();
    let metadata_path = format!("/.well-known/oauth-authorization-server{issuer_path}");
    match (parts.method.as_str(), path.as_str()) {
        ("GET", path) if path == metadata_path => metadata(state),
        ("GET", "/authorize") => authorize(state, &pairs(query.as_bytes())),
        ("POST", "/token") => token(state, &parts.headers, &pairs(&body)),
        ("POST", "/register") => register(state, &body),
        _ => text(http::StatusCode::NOT_FOUND, "not found"),
    }
}

fn pairs(text: &[u8]) -> HashMap<String, String> {
    url::form_urlencoded::parse(text).into_owned().collect()
}

#[must_use]
pub fn json_response(status: http::StatusCode, value: &Value) -> Response {
    let mut response = http::Response::new(Full::new(Bytes::from(value.to_string())).boxed());
    *response.status_mut() = status;
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    response.headers_mut().insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-store"),
    );
    response
}

fn text(status: http::StatusCode, message: &str) -> Response {
    let mut response = http::Response::new(Full::new(Bytes::from(message.to_string())).boxed());
    *response.status_mut() = status;
    response
}

fn oauth_error(status: http::StatusCode, error: &str) -> Response {
    json_response(status, &json!({"error": error}))
}

fn metadata(state: &AuthState) -> Response {
    let mut guard = state.lock();
    guard.record.metadata_calls += 1;
    let settings = guard.settings.clone();
    drop(guard);
    let origin = &state.origin;
    let mut body = json!({
        "issuer": format!("{origin}{}", settings.issuer_path),
        "authorization_endpoint": format!("{origin}/authorize"),
        "token_endpoint": format!("{origin}/token"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported":
            ["none", "client_secret_post", "client_secret_basic"],
        "authorization_response_iss_parameter_supported": settings.announce_iss,
        "scopes_supported": settings.scopes_supported,
    });
    if settings.registration != Registration::Closed {
        body["registration_endpoint"] = json!(format!("{origin}/register"));
    }
    if let Some(page) = settings.service_documentation {
        body["service_documentation"] = json!(page);
    }
    json_response(http::StatusCode::OK, &body)
}

// A loopback redirect URI matches with any port (RFC 8252 §7.3).
fn same_redirect(registered: &str, given: &str) -> bool {
    let without_port = |text: &str| {
        url::Url::parse(text).ok().map(|mut url| {
            let _ = url.set_port(None);
            url.to_string()
        })
    };
    without_port(registered).is_some() && without_port(registered) == without_port(given)
}

fn redirect(target: &str, parameters: &[(&str, &str)]) -> Response {
    let Ok(mut url) = url::Url::parse(target) else {
        return text(http::StatusCode::BAD_REQUEST, "bad redirect_uri");
    };
    url.query_pairs_mut().extend_pairs(parameters);
    let mut response = http::Response::new(Full::new(Bytes::new()).boxed());
    *response.status_mut() = http::StatusCode::FOUND;
    if let Ok(location) = http::HeaderValue::from_str(url.as_str()) {
        response
            .headers_mut()
            .insert(http::header::LOCATION, location);
    }
    response
}

fn authorize(state: &AuthState, query: &HashMap<String, String>) -> Response {
    let issuer = state.issuer();
    let mut guard = state.lock();
    guard.record.authorize_calls += 1;
    guard.record.authorize_resource = query.get("resource").cloned();
    guard.record.authorize_scope = query.get("scope").cloned();
    let get = |name: &str| query.get(name).map(String::as_str).unwrap_or_default();
    let (client_id, redirect_uri, state_value) =
        (get("client_id"), get("redirect_uri"), get("state"));
    let Some(client) = guard.clients.get(client_id).cloned() else {
        return text(http::StatusCode::BAD_REQUEST, "unknown client_id");
    };
    if redirect_uri.is_empty()
        || !(client.redirect_uris.is_empty()
            || client
                .redirect_uris
                .iter()
                .any(|registered| same_redirect(registered, redirect_uri)))
    {
        return text(http::StatusCode::BAD_REQUEST, "bad redirect_uri");
    }
    let invalid = get("response_type") != "code"
        || get("code_challenge").is_empty()
        || get("code_challenge_method") != "S256"
        || state_value.is_empty()
        || get("resource").is_empty();
    let iss = match guard.settings.callback_iss {
        CallbackIss::Correct => Some(issuer),
        CallbackIss::Absent => None,
        CallbackIss::Wrong => Some("http://wrong.invalid".to_string()),
    };
    let with_iss = |mut parameters: Vec<(&'static str, String)>| {
        if let Some(iss) = &iss {
            parameters.push(("iss", iss.clone()));
        }
        parameters
    };
    let parameters = if invalid {
        with_iss(vec![
            ("error", "invalid_request".into()),
            ("state", state_value.into()),
        ])
    } else if guard.settings.decision == Decision::Deny {
        with_iss(vec![
            ("error", "access_denied".into()),
            ("state", state_value.into()),
        ])
    } else {
        let code = format!("code-{}", guard.next());
        let grant = Code {
            client_id: client_id.into(),
            redirect_uri: redirect_uri.into(),
            challenge: get("code_challenge").into(),
            scope: get("scope")
                .split_whitespace()
                .chain(guard.settings.granted_extra.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" "),
        };
        guard.codes.insert(code.clone(), grant);
        with_iss(vec![("code", code), ("state", state_value.into())])
    };
    let parameters: Vec<(&str, &str)> = parameters
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    redirect(redirect_uri, &parameters)
}

// The client of a token request: `client_secret_basic` or the form.
fn client_credentials(
    headers: &http::HeaderMap,
    form: &HashMap<String, String>,
) -> (Option<String>, Option<String>) {
    let basic = headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Basic "))
        .and_then(|encoded| {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()
        })
        .and_then(|decoded| String::from_utf8(decoded).ok())
        .and_then(|pair| {
            let (id, secret) = pair.split_once(':')?;
            let decode = |text: &str| {
                url::form_urlencoded::parse(format!("x={text}").as_bytes())
                    .next()
                    .map(|(_, value)| value.into_owned())
            };
            Some((decode(id)?, decode(secret)?))
        });
    match basic {
        Some((id, secret)) => (Some(id), Some(secret)),
        None => (
            form.get("client_id").cloned(),
            form.get("client_secret").cloned(),
        ),
    }
}

fn s256(verifier: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest.as_ref())
}

fn issue(guard: &mut State, client_id: &str, scope: &str) -> Value {
    let n = guard.next();
    let (access, refresh) = (format!("at-{n}"), format!("rt-{n}"));
    let expires_in = guard.settings.expires_in;
    guard.access_tokens.insert(
        access.clone(),
        AccessToken {
            scope: scope.into(),
            expires: Instant::now() + Duration::from_secs(expires_in),
        },
    );
    guard.refresh_tokens.insert(
        refresh.clone(),
        RefreshToken {
            client_id: client_id.into(),
            scope: scope.into(),
        },
    );
    let mut body = json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": expires_in,
        "refresh_token": refresh,
    });
    if !scope.is_empty() {
        body["scope"] = json!(scope);
    }
    body
}

fn token(state: &AuthState, headers: &http::HeaderMap, form: &HashMap<String, String>) -> Response {
    let mut guard = state.lock();
    guard.record.token_resource = form.get("resource").cloned();
    let (client_id, secret) = client_credentials(headers, form);
    guard.record.token_client_id.clone_from(&client_id);
    guard.record.token_basic = headers.contains_key(http::header::AUTHORIZATION);
    let client_id = client_id.unwrap_or_default();
    let Some(client) = guard.clients.get(&client_id).cloned() else {
        return oauth_error(http::StatusCode::UNAUTHORIZED, "invalid_client");
    };
    if client.secret.is_some() && client.secret != secret {
        return oauth_error(http::StatusCode::UNAUTHORIZED, "invalid_client");
    }
    let get = |name: &str| form.get(name).map(String::as_str).unwrap_or_default();
    match get("grant_type") {
        "authorization_code" => {
            guard.record.code_grants += 1;
            let Some(code) = guard.codes.remove(get("code")) else {
                return oauth_error(http::StatusCode::BAD_REQUEST, "invalid_grant");
            };
            if code.client_id != client_id
                || code.redirect_uri != get("redirect_uri")
                || code.challenge != s256(get("code_verifier"))
                || get("resource").is_empty()
            {
                return oauth_error(http::StatusCode::BAD_REQUEST, "invalid_grant");
            }
            let body = issue(&mut guard, &client_id, &code.scope);
            json_response(http::StatusCode::OK, &body)
        }
        "refresh_token" => {
            guard.record.refresh_grants += 1;
            if guard.settings.reject_refresh {
                return oauth_error(http::StatusCode::BAD_REQUEST, "invalid_grant");
            }
            let Some(refresh) = guard.refresh_tokens.remove(get("refresh_token")) else {
                return oauth_error(http::StatusCode::BAD_REQUEST, "invalid_grant");
            };
            if refresh.client_id != client_id {
                return oauth_error(http::StatusCode::BAD_REQUEST, "invalid_grant");
            }
            let body = issue(&mut guard, &client_id, &refresh.scope);
            json_response(http::StatusCode::OK, &body)
        }
        _ => oauth_error(http::StatusCode::BAD_REQUEST, "unsupported_grant_type"),
    }
}

fn register(state: &AuthState, body: &[u8]) -> Response {
    let mut guard = state.lock();
    guard.record.register_calls += 1;
    if guard.settings.registration == Registration::Closed {
        return text(http::StatusCode::NOT_FOUND, "no registration");
    }
    let Ok(request) = serde_json::from_slice::<Value>(body) else {
        return oauth_error(http::StatusCode::BAD_REQUEST, "invalid_client_metadata");
    };
    guard.record.registrations.push(request.clone());
    let redirect_uris: Vec<String> = request["redirect_uris"]
        .as_array()
        .map(|uris| {
            uris.iter()
                .filter_map(|uri| uri.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if redirect_uris.is_empty() {
        return oauth_error(http::StatusCode::BAD_REQUEST, "invalid_redirect_uri");
    }
    let n = guard.next();
    let client_id = format!("client-{n}");
    let secret =
        (guard.settings.registration == Registration::WithSecret).then(|| format!("secret-{n}"));
    guard.clients.insert(
        client_id.clone(),
        Client {
            secret: secret.clone(),
            redirect_uris,
        },
    );
    let mut response = match request {
        Value::Object(fields) => Value::Object(fields),
        _ => json!({}),
    };
    response["client_id"] = json!(client_id);
    match secret {
        Some(secret) => {
            response["client_secret"] = json!(secret);
            response["token_endpoint_auth_method"] = json!("client_secret_post");
        }
        None => response["token_endpoint_auth_method"] = json!("none"),
    }
    json_response(http::StatusCode::CREATED, &response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s256_matches_the_example_of_rfc_7636() {
        assert_eq!(
            s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn loopback_redirect_matches_with_any_port() {
        assert!(same_redirect(
            "http://127.0.0.1/callback",
            "http://127.0.0.1:5000/callback"
        ));
        assert!(!same_redirect(
            "http://127.0.0.1/callback",
            "http://127.0.0.1:5000/other"
        ));
    }
}
