//! Local MCP fixture servers for the tests of `ncp`. Tests never call a
//! third-party server.
//!
//! The fixture has these modes for `server/discover`:
//! - `modern`: answers at once. Also accepts `initialize`.
//! - `legacy`: answers with "method not found", as a `2025-11-25` server does.
//! - `slow-N`: answers after N ms. Also accepts `initialize`.
//! - `modern-only`: answers at once, and rejects `initialize`.
//! - `slow-modern-only-N`: answers after N ms, and rejects `initialize`.
//! - `tools-only`: as `modern`, with no resources and no prompts.
//! - `repeat-cursor`: as `modern`, and `prompts/list` gives the same cursor
//!   for ever, as a broken server can.
//!
//! The lists of resources and prompts come in two pages.
//!
//! [`serve_http_with_oauth`] protects the HTTP fixture with the tokens of a
//! local authorization server ([`auth_server`]).

pub mod auth_server;

use std::{
    borrow::Cow,
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, DiscoverResult,
        ErrorCode, GetPromptRequestParams, GetPromptResponse, GetPromptResult, InputRequiredResult,
        JsonObject, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
        ListToolsResult, PaginatedRequestParams, ProgressNotificationParam, Prompt, PromptArgument,
        PromptMessage, ProtocolVersion, ReadResourceRequestParams, ReadResourceResponse,
        ReadResourceResult, Resource, ResourceContents, ResourceTemplate, Role, ServerCapabilities,
        ServerConfig, Tool,
    },
    service::{MaybeSendFuture, RequestContext},
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Modern,
    Legacy,
    Slow(Duration),
    ModernOnly,
    SlowModernOnly(Duration),
    ToolsOnly,
    RepeatCursor,
}

impl Mode {
    /// # Errors
    ///
    /// Returns the text when it is not a mode.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "modern" => Ok(Self::Modern),
            "legacy" => Ok(Self::Legacy),
            "modern-only" => Ok(Self::ModernOnly),
            "tools-only" => Ok(Self::ToolsOnly),
            "repeat-cursor" => Ok(Self::RepeatCursor),
            other => {
                let millis = |prefix: &str| {
                    other
                        .strip_prefix(prefix)
                        .and_then(|ms| ms.parse().ok())
                        .map(Duration::from_millis)
                };
                millis("slow-modern-only-")
                    .map(Self::SlowModernOnly)
                    .or_else(|| millis("slow-").map(Self::Slow))
                    .ok_or_else(|| other.to_string())
            }
        }
    }
}

#[derive(Clone)]
pub struct Fixture {
    pub mode: Mode,
    /// Counts the `server/discover` requests.
    pub discover_calls: Arc<AtomicUsize>,
}

impl Fixture {
    #[must_use]
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            discover_calls: Arc::default(),
        }
    }
}

fn schema(value: serde_json::Value) -> Arc<JsonObject> {
    match value {
        serde_json::Value::Object(map) => Arc::new(map),
        _ => Arc::default(),
    }
}

fn tools() -> Vec<Tool> {
    let text_arg =
        |name: &str| schema(json!({"type": "object", "properties": {name: {"type": "string"}}}));
    vec![
        Tool::new("echo", "Return the text", text_arg("text")),
        Tool::new("fail", "Return a tool error", text_arg("text")),
        Tool::new("ask", "Return input_required", text_arg("text")),
        Tool::new(
            "progress",
            "Report three steps of progress",
            text_arg("text"),
        ),
        Tool::new("env", "Return an environment variable", text_arg("name")),
        Tool::new("header", "Return an HTTP request header", text_arg("name")),
        Tool::new(
            "wait",
            "Wait; write the marker file when cancelled",
            schema(json!({"type": "object", "properties": {
                "ms": {"type": "integer"}, "marker": {"type": "string"},
                "started": {"type": "string"}}})),
        ),
    ]
}

/// The cursor of a list request, when the client gives one.
fn cursor(request: Option<&PaginatedRequestParams>) -> Option<&str> {
    request.and_then(|params| params.cursor.as_deref())
}

fn text_result(text: impl Into<String>) -> CallToolResponse {
    CallToolResult::success(vec![ContentBlock::text(text)]).into()
}

fn argument<'a>(arguments: &'a JsonObject, name: &str) -> &'a str {
    arguments
        .get(name)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
}

impl ServerHandler for Fixture {
    fn get_info(&self) -> ServerConfig {
        let capabilities = if self.mode == Mode::ToolsOnly {
            ServerCapabilities::builder().enable_tools().build()
        } else {
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build()
        };
        ServerConfig::new(capabilities).with_instructions("Fixture server for the tests of ncp.")
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        match self.mode {
            Mode::ModernOnly | Mode::SlowModernOnly(_) => {
                Cow::Borrowed(&[ProtocolVersion::V_2026_07_28])
            }
            _ => Cow::Borrowed(ProtocolVersion::KNOWN_VERSIONS),
        }
    }

    fn discover(
        &self,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<DiscoverResult, ErrorData>> + MaybeSendFuture + '_ {
        self.discover_calls.fetch_add(1, Ordering::SeqCst);
        let result = DiscoverResult::from_server_info(
            self.supported_protocol_versions().into_owned(),
            self.get_info(),
        );
        let mode = self.mode.clone();
        async move {
            match mode {
                Mode::Modern | Mode::ModernOnly | Mode::ToolsOnly | Mode::RepeatCursor => {
                    Ok(result)
                }
                Mode::Legacy => Err(ErrorData::new(
                    ErrorCode::METHOD_NOT_FOUND,
                    "Method not found",
                    None,
                )),
                Mode::Slow(delay) | Mode::SlowModernOnly(delay) => {
                    tokio::time::sleep(delay).await;
                    Ok(result)
                }
            }
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + MaybeSendFuture + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(tools())))
    }

    fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, ErrorData>> + MaybeSendFuture + '_ {
        let result = match cursor(request.as_ref()) {
            None => {
                let mut page = ListResourcesResult::with_all_items(vec![
                    Resource::new("fixture://greeting", "greeting")
                        .with_description("A greeting")
                        .with_mime_type("text/plain"),
                ]);
                page.next_cursor = Some("2".into());
                page
            }
            Some(_) => ListResourcesResult::with_all_items(vec![
                Resource::new("fixture://logo", "logo").with_mime_type("image/png"),
                Resource::new("fixture://pair", "pair").with_description("Two contents"),
                Resource::new("fixture://ask", "ask").with_description("Asks for input"),
            ]),
        };
        std::future::ready(Ok(result))
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourceTemplatesResult, ErrorData>> + MaybeSendFuture + '_
    {
        std::future::ready(Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new("fixture://echo/{text}", "echo")
                .with_description("Returns the text"),
        ])))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResponse, ErrorData>> + MaybeSendFuture + '_ {
        let uri = request.uri;
        if uri == "fixture://ask" {
            return std::future::ready(Ok(ReadResourceResponse::InputRequired(
                InputRequiredResult::from_request_state("state-r"),
            )));
        }
        if uri == "fixture://pair" {
            return std::future::ready(Ok(ReadResourceResponse::Complete(
                ReadResourceResult::new(vec![
                    ResourceContents::text("one", uri.as_str()),
                    ResourceContents::text("two", uri.as_str()),
                ]),
            )));
        }
        let contents = match uri.as_str() {
            "fixture://greeting" => Some(ResourceContents::text("hello\n", uri.as_str())),
            // The bytes of a PNG signature, in base64.
            "fixture://logo" => Some(
                ResourceContents::blob("iVBORw0KGgo=", uri.as_str()).with_mime_type("image/png"),
            ),
            other => other
                .strip_prefix("fixture://echo/")
                .map(|text| ResourceContents::text(text, uri.as_str())),
        };
        std::future::ready(match contents {
            Some(contents) => Ok(ReadResourceResponse::Complete(ReadResourceResult::new(
                vec![contents],
            ))),
            None => Err(ErrorData::resource_not_found(
                format!("no resource {uri}"),
                None,
            )),
        })
    }

    fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListPromptsResult, ErrorData>> + MaybeSendFuture + '_ {
        let name = PromptArgument::new("name")
            .with_description("Who to greet")
            .with_required(true);
        let greet = Prompt::new("greet", Some("Greet a person"), Some(vec![name]));
        let result = match (cursor(request.as_ref()), &self.mode) {
            (_, Mode::RepeatCursor) => {
                let mut page = ListPromptsResult::with_all_items(vec![greet]);
                page.next_cursor = Some("again".into());
                page
            }
            (None, _) => {
                let mut page = ListPromptsResult::with_all_items(vec![greet]);
                page.next_cursor = Some("2".into());
                page
            }
            (Some(_), _) => ListPromptsResult::with_all_items(vec![Prompt::new(
                "ask",
                Some("Asks for input"),
                None,
            )]),
        };
        std::future::ready(Ok(result))
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<GetPromptResponse, ErrorData>> + MaybeSendFuture + '_ {
        let arguments = request.arguments.unwrap_or_default();
        std::future::ready(match (request.name.as_str(), arguments.get("name")) {
            ("ask", _) => Ok(GetPromptResponse::InputRequired(
                InputRequiredResult::from_request_state("state-p"),
            )),
            ("greet", Some(serde_json::Value::String(name))) => {
                let message = PromptMessage::new_text(Role::User, format!("Hello, {name}!"));
                Ok(GetPromptResponse::Complete(
                    GetPromptResult::new(vec![message]).with_description("A greeting"),
                ))
            }
            ("greet", _) => Err(ErrorData::invalid_params("missing argument name", None)),
            (other, _) => Err(ErrorData::invalid_params(
                format!("no prompt {other}"),
                None,
            )),
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let arguments = request.arguments.unwrap_or_default();
        match request.name.as_ref() {
            "echo" => Ok(text_result(argument(&arguments, "text"))),
            "fail" => Ok(CallToolResult::error(vec![ContentBlock::text(argument(
                &arguments, "text",
            ))])
            .into()),
            "progress" => {
                if let Some(token) = context.meta.get_progress_token() {
                    for step in 1..=3_u8 {
                        let param = ProgressNotificationParam::new(token.clone(), f64::from(step))
                            .with_total(3.0)
                            .with_message(format!("step {step}"));
                        let _ = context.peer.notify_progress(param).await;
                    }
                }
                Ok(text_result("done"))
            }
            "ask" => Ok(CallToolResponse::InputRequired(
                InputRequiredResult::from_request_state("state-1"),
            )),
            "env" => Ok(text_result(
                std::env::var(argument(&arguments, "name")).unwrap_or_else(|_| "<unset>".into()),
            )),
            "header" => {
                let value = context
                    .extensions
                    .get::<http::request::Parts>()
                    .and_then(|parts| parts.headers.get(argument(&arguments, "name")))
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("<unset>")
                    .to_string();
                Ok(text_result(value))
            }
            "wait" => {
                let ms = arguments
                    .get("ms")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let marker = argument(&arguments, "marker").to_string();
                // A test can wait for this file before it interrupts.
                let started = argument(&arguments, "started");
                if !started.is_empty() {
                    let _ = std::fs::write(started, "started");
                }
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_millis(ms)) => Ok(text_result("done")),
                    () = context.ct.cancelled() => {
                        if !marker.is_empty() {
                            let _ = std::fs::write(&marker, "cancelled");
                        }
                        Err(ErrorData::internal_error("cancelled", None))
                    }
                }
            }
            other => Err(ErrorData::invalid_params(format!("no tool {other}"), None)),
        }
    }
}

fn unauthorized() -> http::Response<BoxBody<Bytes, Infallible>> {
    let mut response = http::Response::new(Full::new(Bytes::new()).boxed());
    *response.status_mut() = http::StatusCode::UNAUTHORIZED;
    response.headers_mut().insert(
        http::header::WWW_AUTHENTICATE,
        http::HeaderValue::from_static("Bearer"),
    );
    response
}

/// A Streamable HTTP fixture on a free loopback port. The server stops when
/// the value is dropped.
pub struct HttpFixture {
    pub url: String,
    pub discover_calls: Arc<AtomicUsize>,
    stop: CancellationToken,
}

impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

/// Starts a Streamable HTTP fixture.
///
/// # Errors
///
/// Returns an error when no loopback port is free.
pub async fn serve_http(mode: Mode) -> std::io::Result<HttpFixture> {
    serve_http_with(mode, None).await
}

/// Starts a Streamable HTTP fixture. With a token, each request without
/// the header `Authorization: Bearer <token>` gets `401`.
///
/// # Errors
///
/// Returns an error when no loopback port is free.
pub async fn serve_http_with(
    mode: Mode,
    bearer_token: Option<&str>,
) -> std::io::Result<HttpFixture> {
    let access = bearer_token.map_or(Access::Open, |token| {
        Access::Bearer(format!("Bearer {token}"))
    });
    serve(mode, access, None).await
}

/// A tool that needs one more scope. A call with a token that does not
/// have the scope gets `403` with `insufficient_scope` (step-up).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepUp {
    pub tool: String,
    pub scope: String,
}

/// Starts a Streamable HTTP fixture that accepts only the access tokens of
/// the authorization server. It serves its protected resource metadata
/// (RFC 9728) with the server as its authorization server. A request with
/// no valid token gets `401` with the metadata URL in `WWW-Authenticate`.
///
/// # Errors
///
/// Returns an error when no loopback port is free.
pub async fn serve_http_with_oauth(
    mode: Mode,
    auth: &auth_server::AuthServer,
    step_up: Option<StepUp>,
) -> std::io::Result<HttpFixture> {
    let access = Access::OAuth(Arc::new(OAuthAccess {
        auth: auth.state.clone(),
        step_up,
        open_handshake: false,
    }));
    serve(mode, access, None).await
}

/// The OAuth fixture with an open handshake: only the requests after the
/// handshake need a token, and get `401` with no token.
///
/// # Errors
///
/// Returns an error when no loopback port is free.
pub async fn serve_http_with_oauth_after_handshake(
    mode: Mode,
    auth: &auth_server::AuthServer,
) -> std::io::Result<HttpFixture> {
    let access = Access::OAuth(Arc::new(OAuthAccess {
        auth: auth.state.clone(),
        step_up: None,
        open_handshake: true,
    }));
    serve(mode, access, None).await
}

/// The access rule of the HTTP fixture.
#[derive(Clone)]
enum Access {
    Open,
    /// The exact `Authorization` header value.
    Bearer(String),
    OAuth(Arc<OAuthAccess>),
}

struct OAuthAccess {
    auth: auth_server::AuthState,
    step_up: Option<StepUp>,
    /// The handshake and the stream requests need no token; the other
    /// requests do. Some servers work so.
    open_handshake: bool,
}

/// The methods that a server with an open handshake answers with no token.
const HANDSHAKE_METHODS: [&str; 3] = ["initialize", "server/discover", "notifications/initialized"];

/// The URL of the protected resource metadata of the MCP URL, with the path
/// of the resource after the well-known part (RFC 9728 §3.1).
#[must_use]
pub fn metadata_url(mcp_url: &str) -> String {
    match url::Url::parse(mcp_url) {
        Ok(url) => format!(
            "{}{WELL_KNOWN_RESOURCE}{}",
            url.origin().ascii_serialization(),
            url.path()
        ),
        Err(_) => String::new(),
    }
}

const WELL_KNOWN_RESOURCE: &str = "/.well-known/oauth-protected-resource";

/// The test CA that signed the certificate of [`serve_https`].
pub const TEST_CA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/tls/ca.pem");

/// Starts a Streamable HTTP fixture over TLS, with a certificate from
/// [`TEST_CA`] for `127.0.0.1`. No system trusts this CA.
///
/// # Errors
///
/// Returns an error when the test certificate does not load, or when no
/// loopback port is free.
pub async fn serve_https(mode: Mode) -> std::io::Result<HttpFixture> {
    serve(mode, Access::Open, Some(test_tls()?)).await
}

fn test_tls() -> std::io::Result<tokio_rustls::TlsAcceptor> {
    use tokio_rustls::rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    };
    let invalid = |error: &dyn std::fmt::Display| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string())
    };
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/tls");
    let chain = CertificateDer::pem_file_iter(format!("{dir}/server.pem"))
        .and_then(Iterator::collect::<Result<Vec<_>, _>>)
        .map_err(|error| invalid(&error))?;
    let key = PrivateKeyDer::from_pem_file(format!("{dir}/server.key"))
        .map_err(|error| invalid(&error))?;
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|error| invalid(&error))?
    .with_no_client_auth()
    .with_single_cert(chain, key)
    .map_err(|error| invalid(&error))?;
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
}

async fn serve(
    mode: Mode,
    access: Access,
    tls: Option<tokio_rustls::TlsAcceptor>,
) -> std::io::Result<HttpFixture> {
    let fixture = Fixture::new(mode);
    let discover_calls = fixture.discover_calls.clone();
    let stop = CancellationToken::new();
    let service: StreamableHttpService<Fixture, LocalSessionManager> = StreamableHttpService::new(
        move || Ok(fixture.clone()),
        Arc::default(),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(true)
            .with_cancellation_token(stop.child_token()),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let scheme = if tls.is_some() { "https" } else { "http" };
    let url = format!("{scheme}://{}/mcp", listener.local_addr()?);
    let resource: Arc<str> = Arc::from(url.as_str());
    let accept_stop = stop.clone();
    tokio::spawn(async move {
        loop {
            let stream = tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok((stream, _)) => stream,
                    Err(_) => return,
                },
                () = accept_stop.cancelled() => return,
            };
            let (service, access, tls) = (service.clone(), access.clone(), tls.clone());
            let resource = resource.clone();
            tokio::spawn(async move {
                match tls {
                    Some(tls) => {
                        if let Ok(stream) = tls.accept(stream).await {
                            serve_connection(stream, service, access, resource).await;
                        }
                    }
                    None => serve_connection(stream, service, access, resource).await,
                }
            });
        }
    });
    Ok(HttpFixture {
        url,
        discover_calls,
        stop,
    })
}

async fn serve_connection(
    stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    service: StreamableHttpService<Fixture, LocalSessionManager>,
    access: Access,
    resource: Arc<str>,
) {
    let handler = hyper::service::service_fn(move |request: http::Request<_>| {
        let (service, access, resource) = (service.clone(), access.clone(), resource.clone());
        async move {
            let sent = request
                .headers()
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .map(String::from);
            Ok::<_, Infallible>(match access {
                Access::Open => service.handle(request).await,
                Access::Bearer(expected) if sent.as_deref() == Some(expected.as_str()) => {
                    service.handle(request).await
                }
                Access::Bearer(_) => unauthorized(),
                Access::OAuth(oauth) => {
                    oauth_handle(&service, &oauth, &resource, sent.as_deref(), request).await
                }
            })
        }
    });
    let _ = hyper::server::conn::http1::Builder::new()
        .serve_connection(hyper_util::rt::TokioIo::new(stream), handler)
        .await;
}

type FixtureResponse = http::Response<BoxBody<Bytes, Infallible>>;

fn challenge(status: http::StatusCode, value: &str) -> FixtureResponse {
    let mut response = http::Response::new(Full::new(Bytes::new()).boxed());
    *response.status_mut() = status;
    if let Ok(value) = http::HeaderValue::from_str(value) {
        response
            .headers_mut()
            .insert(http::header::WWW_AUTHENTICATE, value);
    }
    response
}

/// True when the JSON-RPC body calls the tool.
fn calls_tool(body: &[u8], tool: &str) -> bool {
    serde_json::from_slice::<serde_json::Value>(body)
        .is_ok_and(|message| message["method"] == "tools/call" && message["params"]["name"] == tool)
}

async fn oauth_handle(
    service: &StreamableHttpService<Fixture, LocalSessionManager>,
    oauth: &OAuthAccess,
    resource: &str,
    sent: Option<&str>,
    request: http::Request<hyper::body::Incoming>,
) -> FixtureResponse {
    let metadata = metadata_url(resource);
    let path = request.uri().path();
    if request.method() == http::Method::GET
        && path.starts_with(WELL_KNOWN_RESOURCE)
        && metadata.ends_with(path)
    {
        return auth_server::json_response(
            http::StatusCode::OK,
            &json!({
                "resource": resource,
                "authorization_servers": [oauth.auth.issuer()],
                "bearer_methods_supported": ["header"],
                "scopes_supported": oauth.auth.scopes_supported(),
            }),
        );
    }
    let token = sent.and_then(|value| value.strip_prefix("Bearer "));
    let scopes = token.and_then(|token| oauth.auth.token_scopes(token));
    if scopes.is_none() && oauth.open_handshake {
        let (parts, body) = request.into_parts();
        let body = body
            .collect()
            .await
            .map(http_body_util::Collected::to_bytes)
            .unwrap_or_default();
        let method = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|message| message["method"].as_str().map(str::to_string));
        let open = parts.method != http::Method::POST
            || method.is_some_and(|method| HANDSHAKE_METHODS.contains(&method.as_str()));
        if open {
            return service
                .handle(http::Request::from_parts(parts, Full::new(body)))
                .await;
        }
        return challenge(
            http::StatusCode::UNAUTHORIZED,
            &format!("Bearer resource_metadata=\"{metadata}\""),
        );
    }
    let Some(scopes) = scopes else {
        let error = if token.is_some() {
            "error=\"invalid_token\", "
        } else {
            ""
        };
        return challenge(
            http::StatusCode::UNAUTHORIZED,
            &format!("Bearer {error}resource_metadata=\"{metadata}\""),
        );
    };
    let Some(step_up) = &oauth.step_up else {
        return service.handle(request).await;
    };
    let (parts, body) = request.into_parts();
    let body = body
        .collect()
        .await
        .map(http_body_util::Collected::to_bytes)
        .unwrap_or_default();
    if calls_tool(&body, &step_up.tool) && !scopes.contains(&step_up.scope) {
        return challenge(
            http::StatusCode::FORBIDDEN,
            &format!(
                "Bearer error=\"insufficient_scope\", scope=\"{}\", resource_metadata=\"{metadata}\"",
                step_up.scope
            ),
        );
    }
    service
        .handle(http::Request::from_parts(parts, Full::new(body)))
        .await
}

/// A proxy that records the first line of each request and answers each
/// one with `502 Bad Gateway`. It stops when the value is dropped.
pub struct FakeProxy {
    pub url: String,
    pub request_lines: Arc<std::sync::Mutex<Vec<String>>>,
    stop: CancellationToken,
}

impl Drop for FakeProxy {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

/// Starts a fake proxy on a free loopback port.
///
/// # Errors
///
/// Returns an error when no loopback port is free.
pub async fn serve_fake_proxy() -> std::io::Result<FakeProxy> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}", listener.local_addr()?);
    let request_lines: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let stop = CancellationToken::new();
    let (lines, accept_stop) = (request_lines.clone(), stop.clone());
    tokio::spawn(async move {
        loop {
            let mut stream = tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok((stream, _)) => stream,
                    Err(_) => return,
                },
                () = accept_stop.cancelled() => return,
            };
            let lines = lines.clone();
            tokio::spawn(async move {
                let mut first = String::new();
                let mut reader = BufReader::new(&mut stream);
                if reader.read_line(&mut first).await.is_ok() {
                    if let Ok(mut lines) = lines.lock() {
                        lines.push(first.trim_end().to_string());
                    }
                    let _ = stream
                        .write_all(b"HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                        .await;
                }
            });
        }
    });
    Ok(FakeProxy {
        url,
        request_lines,
        stop,
    })
}
