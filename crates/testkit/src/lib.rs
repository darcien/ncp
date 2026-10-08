//! Local MCP fixture servers for the tests of `ncp`. Tests never call a
//! third-party server.
//!
//! The fixture has these modes for `server/discover`:
//! - `modern`: answers at once. Also accepts `initialize`.
//! - `legacy`: answers with "method not found", as a `2025-11-25` server does.
//! - `slow-N`: answers after N ms. Also accepts `initialize`.
//! - `modern-only`: answers at once, and rejects `initialize`.
//! - `slow-modern-only-N`: answers after N ms, and rejects `initialize`.

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
        ErrorCode, InputRequiredResult, JsonObject, ListToolsResult, PaginatedRequestParams,
        ProtocolVersion, ServerCapabilities, ServerConfig, Tool,
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
        Tool::new("env", "Return an environment variable", text_arg("name")),
        Tool::new("header", "Return an HTTP request header", text_arg("name")),
        Tool::new(
            "wait",
            "Wait; write the marker file when cancelled",
            schema(json!({"type": "object", "properties": {
                "ms": {"type": "integer"}, "marker": {"type": "string"}}})),
        ),
    ]
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
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("Fixture server for the tests of ncp.")
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
                Mode::Modern | Mode::ModernOnly => Ok(result),
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
    let expected = bearer_token.map(|token| format!("Bearer {token}"));
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
    let url = format!("http://{}/mcp", listener.local_addr()?);
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
            let service = service.clone();
            let expected = expected.clone();
            tokio::spawn(async move {
                let handler = hyper::service::service_fn(move |request: http::Request<_>| {
                    let service = service.clone();
                    let sent = request
                        .headers()
                        .get(http::header::AUTHORIZATION)
                        .and_then(|value| value.to_str().ok());
                    let allowed = expected
                        .as_deref()
                        .is_none_or(|expected| sent == Some(expected));
                    async move {
                        if allowed {
                            Ok::<_, Infallible>(service.handle(request).await)
                        } else {
                            Ok(unauthorized())
                        }
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(hyper_util::rt::TokioIo::new(stream), handler)
                    .await;
            });
        }
    });
    Ok(HttpFixture {
        url,
        discover_calls,
        stop,
    })
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
