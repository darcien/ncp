//! Events of a connection for diagnostics: the JSON-RPC messages, the HTTP
//! requests, and the time of each network step.
//!
//! The client core gives each event to a function of the caller and writes
//! nothing itself. The caller decides what to show (`-vv`) and what to add
//! up (`--timings`).

use std::{
    collections::HashMap,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use futures_util::stream::BoxStream;
use reqwest::header::{HeaderName, HeaderValue};
use rmcp::{
    RoleClient,
    model::ClientJsonRpcMessage,
    service::{RxJsonRpcMessage, TxJsonRpcMessage},
    transport::{
        Transport,
        streamable_http_client::{
            SseError, StreamableHttpClient, StreamableHttpError, StreamableHttpPostResponse,
        },
    },
};
use serde_json::Value;
use sse_stream::Sse;
use tokio::time::Instant;

use crate::redact;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Sent,
    Received,
}

/// One HTTP request of the transport. [`redact::header`] hides the header
/// values. The URL is as reqwest got it: the caller hides it before it
/// shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpExchange {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// What came back: the kind of response, or the error.
    pub outcome: String,
    pub elapsed: Duration,
    /// The bodies of an OAuth request, with secrets hidden. The bodies of
    /// MCP requests show as JSON-RPC messages.
    pub request_body: Option<String>,
    pub response_body: Option<String>,
}

#[derive(Debug)]
pub enum Event<'a> {
    Message {
        direction: Direction,
        message: &'a Value,
    },
    Http(&'a HttpExchange),
    Response {
        method: &'a str,
        elapsed: Duration,
    },
    Dns {
        host: &'a str,
        elapsed: Duration,
    },
    /// A new connection: DNS, TCP, the proxy `CONNECT` and TLS. reqwest has
    /// no hook for each part.
    Connect {
        elapsed: Duration,
    },
    ServerStart {
        elapsed: Duration,
    },
}

/// The function that gets the events. With no function, the client core
/// makes no event.
#[derive(Clone, Default)]
pub struct Observer(Option<Arc<EventFunction>>);

type EventFunction = dyn Fn(&Event<'_>) + Send + Sync;

impl std::fmt::Debug for Observer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "Observer"
        } else {
            "Observer(none)"
        })
    }
}

impl Observer {
    #[must_use]
    pub fn new(function: impl Fn(&Event<'_>) + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(function)))
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.0.is_some()
    }

    pub fn emit(&self, event: &Event<'_>) {
        if let Some(function) = &self.0 {
            function(event);
        }
    }
}

/// A transport that gives each message, and the response time of each
/// request, to the observer.
pub struct Observed<T> {
    inner: T,
    observer: Observer,
    /// The requests with no answer yet: the JSON of the ID, the method and
    /// the send time.
    pending: HashMap<String, (String, Instant)>,
}

impl<T> Observed<T> {
    pub fn new(inner: T, observer: Observer) -> Self {
        Self {
            inner,
            observer,
            pending: HashMap::new(),
        }
    }
}

impl<T: Transport<RoleClient>> Transport<RoleClient> for Observed<T> {
    type Error = T::Error;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        if self.observer.is_active()
            && let Ok(message) = serde_json::to_value(&item)
        {
            if let (Some(id), Some(method)) = (message.get("id"), message.get("method")) {
                let method = method.as_str().unwrap_or_default().to_string();
                self.pending
                    .insert(id.to_string(), (method, Instant::now()));
            }
            self.observer.emit(&Event::Message {
                direction: Direction::Sent,
                message: &message,
            });
        }
        self.inner.send(item)
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        let item = self.inner.receive().await?;
        if self.observer.is_active()
            && let Ok(message) = serde_json::to_value(&item)
        {
            self.observer.emit(&Event::Message {
                direction: Direction::Received,
                message: &message,
            });
            let answer = message
                .get("method")
                .is_none()
                .then(|| message.get("id"))
                .flatten();
            if let Some((method, sent)) = answer.and_then(|id| self.pending.remove(&id.to_string()))
            {
                self.observer.emit(&Event::Response {
                    method: &method,
                    elapsed: sent.elapsed(),
                });
            }
        }
        Some(item)
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.inner.close()
    }
}

/// An HTTP client of the Streamable HTTP transport that gives each request
/// to the observer.
#[derive(Clone)]
pub struct ObservedHttp<C> {
    pub inner: C,
    pub observer: Observer,
}

impl<C> ObservedHttp<C> {
    // Runs one request, and gives it to the observer with its headers,
    // its outcome and its time.
    async fn observed<T, E: std::fmt::Display>(
        &self,
        method: &'static str,
        uri: &str,
        headers: Vec<(String, String)>,
        request: impl Future<Output = Result<T, E>>,
        describe: impl FnOnce(&T) -> String,
    ) -> Result<T, E> {
        if !self.observer.is_active() {
            return request.await;
        }
        let started = Instant::now();
        let result = request.await;
        let outcome = match &result {
            Ok(value) => describe(value),
            Err(error) => format!("error: {error}"),
        };
        self.observer.emit(&Event::Http(&HttpExchange {
            method,
            url: uri.to_string(),
            headers,
            outcome,
            elapsed: started.elapsed(),
            request_body: None,
            response_body: None,
        }));
        result
    }
}

// The headers that the transport gives, hidden. reqwest adds `accept`,
// `content-type` and `content-length`.
fn shown_headers(
    session_id: Option<&str>,
    auth_header: Option<&str>,
    custom: &HashMap<HeaderName, HeaderValue>,
) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = custom
        .iter()
        .map(|(name, value)| {
            let text = String::from_utf8_lossy(value.as_bytes());
            (
                name.as_str().to_string(),
                redact::header(name.as_str(), &text, value.is_sensitive()),
            )
        })
        .chain(auth_header.map(|value| ("authorization".to_string(), redact::hide(value).into())))
        // A session ID lets another client take the session (MCP
        // 2025-11-25, security best practices, "Session Hijacking").
        .chain(session_id.map(|id| ("mcp-session-id".to_string(), redact::hide(id).into())))
        .collect();
    headers.sort();
    headers
}

fn post_outcome(response: &StreamableHttpPostResponse) -> String {
    match response {
        StreamableHttpPostResponse::Accepted => "202 accepted",
        StreamableHttpPostResponse::Json(..) => "JSON response",
        StreamableHttpPostResponse::Sse(..) => "event stream",
        _ => "response",
    }
    .to_string()
}

type EventStream = BoxStream<'static, Result<Sse, SseError>>;

impl<C: StreamableHttpClient + Sync> StreamableHttpClient for ObservedHttp<C> {
    type Error = C::Error;

    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let headers = shown_headers(
            session_id.as_deref(),
            auth_header.as_deref(),
            &custom_headers,
        );
        let request = self.inner.post_message(
            uri.clone(),
            message,
            session_id,
            auth_header,
            custom_headers,
        );
        self.observed("POST", &uri, headers, request, post_outcome)
            .await
    }

    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session_id: Option<Arc<str>>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let headers = shown_headers(
            session_id.as_deref(),
            auth_header.as_deref(),
            &custom_headers,
        );
        let request = self.inner.post_message_with_max_sse_event_size(
            uri.clone(),
            message,
            session_id,
            auth_header,
            custom_headers,
            max_sse_event_size,
        );
        self.observed("POST", &uri, headers, request, post_outcome)
            .await
    }

    async fn delete_session(
        &self,
        uri: Arc<str>,
        session_id: Arc<str>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        let headers = shown_headers(Some(&session_id), auth_header.as_deref(), &custom_headers);
        let request =
            self.inner
                .delete_session(uri.clone(), session_id, auth_header, custom_headers);
        self.observed("DELETE", &uri, headers, request, |()| {
            "session closed or not supported".to_string()
        })
        .await
    }

    async fn get_stream(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<EventStream, StreamableHttpError<Self::Error>> {
        let headers = shown_headers(
            session_id.as_deref(),
            auth_header.as_deref(),
            &custom_headers,
        );
        let request = self.inner.get_stream(
            uri.clone(),
            session_id,
            last_event_id,
            auth_header,
            custom_headers,
        );
        self.observed("GET", &uri, headers, request, |_| {
            "event stream".to_string()
        })
        .await
    }

    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session_id: Option<Arc<str>>,
        last_event_id: Option<String>,
        auth_header: Option<String>,
        custom_headers: HashMap<HeaderName, HeaderValue>,
        max_sse_event_size: usize,
    ) -> Result<EventStream, StreamableHttpError<Self::Error>> {
        let headers = shown_headers(
            session_id.as_deref(),
            auth_header.as_deref(),
            &custom_headers,
        );
        let request = self.inner.get_stream_with_max_sse_event_size(
            uri.clone(),
            session_id,
            last_event_id,
            auth_header,
            custom_headers,
            max_sse_event_size,
        );
        self.observed("GET", &uri, headers, request, |_| {
            "event stream".to_string()
        })
        .await
    }
}

/// A DNS resolver that gives the time of each lookup to the observer.
pub struct ObservedResolver {
    pub observer: Observer,
}

impl reqwest::dns::Resolve for ObservedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let observer = self.observer.clone();
        Box::pin(async move {
            let host = name.as_str().to_string();
            let started = Instant::now();
            let addresses: Vec<std::net::SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            observer.emit(&Event::Dns {
                host: &host,
                elapsed: started.elapsed(),
            });
            let addresses: reqwest::dns::Addrs = Box::new(addresses.into_iter());
            Ok(addresses)
        })
    }
}

/// A layer on the reqwest connector that gives the time of each new
/// connection to the observer.
#[derive(Clone)]
pub struct ObservedConnectLayer {
    pub observer: Observer,
}

impl<S> tower_layer::Layer<S> for ObservedConnectLayer {
    type Service = ObservedConnect<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ObservedConnect {
            inner,
            observer: self.observer.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ObservedConnect<S> {
    inner: S,
    observer: Observer,
}

type BoxedFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

impl<S, R> tower_service::Service<R> for ObservedConnect<S>
where
    S: tower_service::Service<R>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxedFuture<Result<S::Response, S::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: R) -> Self::Future {
        let started = Instant::now();
        let observer = self.observer.clone();
        let connect = self.inner.call(request);
        Box::pin(async move {
            let result = connect.await;
            observer.emit(&Event::Connect {
                elapsed: started.elapsed(),
            });
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shown_headers_hide_credentials_and_the_session() {
        let mut secret = HeaderValue::from_static("k");
        secret.set_sensitive(true);
        let custom = HashMap::from([
            (HeaderName::from_static("x-api-key"), secret),
            (
                HeaderName::from_static("x-org"),
                HeaderValue::from_static("acme"),
            ),
        ]);
        assert_eq!(
            shown_headers(Some("s1"), Some("t0ken"), &custom),
            [
                ("authorization".to_string(), "****".to_string()),
                ("mcp-session-id".into(), "****".into()),
                ("x-api-key".into(), "****".into()),
                ("x-org".into(), "acme".into()),
            ]
        );
    }
}
