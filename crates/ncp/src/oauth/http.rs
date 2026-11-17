//! The HTTP client of the OAuth requests: discovery, registration, token
//! and refresh.
//!
//! rmcp sends each OAuth request through this client. It has the settings
//! of the MCP client (proxy, `SSL_CERT_FILE`). The observer gets each
//! request with its secrets hidden, so that `-vv` shows OAuth traffic too.
//!
//! The server names the URLs, so the client applies the address policy of
//! the entry to each request. On a direct route, its resolver keeps only
//! the permitted addresses of each DNS answer, so a later DNS answer cannot
//! pass around an earlier check. On a proxy route, the client uses only the
//! proxy that the check saw. The client follows no redirect, because the
//! target of a redirect got no check.

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use hyper_util::client::proxy::matcher::Intercept;
use reqwest::{
    dns::{Addrs, Name, Resolve, Resolving},
    header::HeaderValue,
};
use rmcp::transport::auth::{
    OAuthHttpClient, OAuthHttpClientError, OAuthHttpClientFuture, OAuthHttpRedirectPolicy,
    OAuthHttpRequest,
};
use tokio::time::Instant;
use url::Url;

use crate::{
    client::{
        http::{
            ConnectNotes, HttpError, causes, configured, noted, observed_lookup, resolving_with,
        },
        observe::{Event, HttpExchange, ObservedResolver, Observer},
    },
    expand::Env,
    oauth::address::{self, Permission, SafetyStop, StopReason},
    proxy::{self, Route},
    redact,
};

/// The largest response body. An OAuth response is small. A larger body is an
/// error of the server.
const MAX_BODY: usize = 1024 * 1024;

/// The limit of one OAuth request when rmcp gives none.
const REQUEST_LIMIT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
enum OAuthHttpError {
    #[error("The response body is larger than {MAX_BODY} bytes")]
    BodyTooLarge,
}

/// One client for each proxy, so that the run reuses its connections.
type ProxyKey = (http::Uri, Option<HeaderValue>, Option<(String, String)>);

/// The test build gives fixed DNS answers for names that a test cannot
/// register.
pub type FixedHosts = fn(&str) -> Option<Vec<IpAddr>>;

#[must_use]
pub fn system_hosts(_: &str) -> Option<Vec<IpAddr>> {
    None
}

/// The proxy matcher reads the process environment and the system
/// settings, so the tests give a fixed route.
type SelectRoute = Arc<dyn Fn(&Url) -> proxy::Selected + Send + Sync>;

pub struct OAuthHttp {
    env: Env,
    select: SelectRoute,
    observer: Observer,
    notes: ConnectNotes,
    permission: Permission,
    entry_host: String,
    resolver: Arc<PolicyResolver>,
    /// It uses no proxy, so that only the filtered answer of its resolver
    /// can give an address.
    direct: reqwest::Client,
    proxied: Mutex<HashMap<ProxyKey, reqwest::Client>>,
}

impl OAuthHttp {
    /// Each client of one entry must get the same `permission`, also on
    /// the error paths.
    ///
    /// # Errors
    ///
    /// Returns an error when the file in `SSL_CERT_FILE` gives no root
    /// certificate, or when reqwest cannot make the client.
    pub fn new(
        env: &Env,
        (observer, notes): (&Observer, &ConnectNotes),
        (permission, entry_host): (Permission, &str),
        fixed: FixedHosts,
    ) -> Result<Self, HttpError> {
        let routes = env.clone();
        Self::with_network(
            env,
            (observer, notes),
            (permission, entry_host),
            Arc::new(move |url: &Url| proxy::select(&routes, url)),
            Arc::new(HostsLookup {
                fixed,
                system: observed_lookup(observer),
            }),
        )
    }

    /// The proxy settings of the host must not change the tests.
    #[cfg(test)]
    pub(crate) fn direct_for_tests(permission: Permission, entry_host: &str) -> Self {
        let observer = Observer::default();
        Self::with_network(
            &Env::new(),
            (&observer, &ConnectNotes::default()),
            (permission, entry_host),
            Arc::new(|_: &Url| proxy::Selected {
                route: Route::Direct,
                proxy: None,
            }),
            Arc::new(observed_lookup(&observer)),
        )
        .unwrap_or_else(|error| panic!("{error}"))
    }

    fn with_network(
        env: &Env,
        (observer, notes): (&Observer, &ConnectNotes),
        (permission, entry_host): (Permission, &str),
        select: SelectRoute,
        lookup: Arc<dyn Resolve>,
    ) -> Result<Self, HttpError> {
        let resolver = Arc::new(PolicyResolver { permission, lookup });
        let direct = noted(resolving_with(env, observer, resolver.clone())?, notes)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            env: env.clone(),
            select,
            observer: observer.clone(),
            notes: notes.clone(),
            permission,
            entry_host: entry_host.to_string(),
            resolver,
            direct,
            proxied: Mutex::default(),
        })
    }

    /// A host name passes, because the request checks its DNS answer.
    ///
    /// # Errors
    ///
    /// Returns the stop when the URL breaks the policy.
    pub fn check_request_url(&self, text: &str) -> Result<(), SafetyStop> {
        let url = self.parsed(text)?;
        let route = (self.select)(&url).route;
        address::check_url(&url, self.permission, &route)
            .map_err(|reason| self.stopped_before_request(&url, reason))
    }

    /// The browser selects its own route, resolves the name and follows
    /// redirects, so only the scheme, an IP literal and a localhost name
    /// get a check.
    ///
    /// # Errors
    ///
    /// Returns the stop when the URL breaks the policy.
    pub fn check_browser_url(&self, text: &str) -> Result<(), SafetyStop> {
        let url = self.parsed(text)?;
        address::check_url(&url, self.permission, &Route::Direct)
            .map_err(|reason| self.stopped_before_request(&url, reason))
    }

    /// A login calls this before the browser opens, so that the user does
    /// not log in for a request that must stop. The check at the
    /// connection still applies, because DNS can change. A lookup error is
    /// not a stop: the request reports it.
    ///
    /// # Errors
    ///
    /// Returns the stop when the answer has no permitted address.
    pub async fn check_lookup(&self, text: &str) -> Result<(), SafetyStop> {
        let url = self.parsed(text)?;
        let direct = (self.select)(&url).proxy.is_none();
        let Some(url::Host::Domain(host)) = url.host().filter(|_| direct) else {
            return Ok(());
        };
        let Ok(name) = host.parse::<Name>() else {
            return Ok(());
        };
        match self.resolver.resolve(name).await {
            Err(error) => match refused_answer(error.as_ref()) {
                Some(reason) => Err(self.stopped_before_request(&url, reason)),
                None => Ok(()),
            },
            Ok(_) => Ok(()),
        }
    }

    fn parsed(&self, text: &str) -> Result<Url, SafetyStop> {
        Url::parse(text).map_err(|_| {
            self.traced(self.noted(SafetyStop {
                url: redact::url(text),
                host: String::new(),
                on_entry_host: false,
                reason: StopReason::NotValid,
            }))
        })
    }

    // A check before a request has no `Http` event, so the stop gets its
    // own line in `-vv`.
    fn stopped_before_request(&self, url: &Url, reason: StopReason) -> SafetyStop {
        self.traced(self.stop(url, reason))
    }

    fn stop(&self, url: &Url, reason: StopReason) -> SafetyStop {
        let host = url.host_str().unwrap_or_default();
        self.noted(SafetyStop {
            url: redact::url(url.as_str()),
            host: host.to_string(),
            on_entry_host: host == self.entry_host,
            reason,
        })
    }

    fn noted(&self, stop: SafetyStop) -> SafetyStop {
        self.notes.set_oauth_stop(Some(stop.clone()));
        stop
    }

    fn traced(&self, stop: SafetyStop) -> SafetyStop {
        if self.observer.is_active() {
            self.observer.emit(&Event::OAuthStop {
                url: &stop.url,
                reason: &stop.describe(&|text| text.to_string()),
            });
        }
        stop
    }

    // A proxy client looks up only the name of the proxy, so the policy of
    // the entry does not filter it.
    fn client_for(&self, proxy: Option<&Intercept>) -> Result<reqwest::Client, HttpError> {
        let Some(intercept) = proxy else {
            return Ok(self.direct.clone());
        };
        let key: ProxyKey = (
            intercept.uri().clone(),
            intercept.basic_auth().cloned(),
            intercept
                .raw_auth()
                .map(|(user, password)| (user.to_string(), password.to_string())),
        );
        let mut clients = self.proxied.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }
        let client = noted(configured(&self.env, &self.observer)?, &self.notes)
            .proxy(only_proxy(intercept)?)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        clients.insert(key, client.clone());
        Ok(client)
    }
}

// The matcher keeps the credentials apart from the URI: a header for an
// HTTP proxy, or a user and a password for SOCKS.
fn only_proxy(intercept: &Intercept) -> Result<reqwest::Proxy, HttpError> {
    let proxy = reqwest::Proxy::all(intercept.uri().to_string())?;
    Ok(match (intercept.basic_auth(), intercept.raw_auth()) {
        (Some(header), _) => proxy.custom_http_auth(header.clone()),
        (None, Some((user, password))) => proxy.basic_auth(user, password),
        (None, None) => proxy,
    })
}

struct HostsLookup {
    fixed: FixedHosts,
    system: ObservedResolver,
}

impl Resolve for HostsLookup {
    fn resolve(&self, name: Name) -> Resolving {
        match (self.fixed)(name.as_str()) {
            Some(addresses) => {
                let addresses: Vec<SocketAddr> = addresses
                    .into_iter()
                    .map(|address| SocketAddr::new(address, 0))
                    .collect();
                Box::pin(async move { Ok(Box::new(addresses.into_iter()) as Addrs) })
            }
            None => self.system.resolve(name),
        }
    }
}

/// reqwest connects only to an address of this answer, so the check
/// applies to the address of the connection itself.
struct PolicyResolver {
    permission: Permission,
    lookup: Arc<dyn Resolve>,
}

#[derive(Debug, thiserror::Error)]
#[error("The DNS answer has no address that the entry permits for OAuth requests")]
struct RefusedAnswer(StopReason);

impl Resolve for PolicyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let permission = self.permission;
        let lookup = self.lookup.clone();
        Box::pin(async move {
            let host = name.as_str().to_string();
            let answer: Vec<SocketAddr> = lookup.resolve(name).await?.collect();
            let permitted =
                address::permitted_addresses(&host, answer, permission).map_err(RefusedAnswer)?;
            Ok(Box::new(permitted.into_iter()) as Addrs)
        })
    }
}

fn refused_answer(error: &(dyn std::error::Error + 'static)) -> Option<StopReason> {
    causes(error).find_map(|cause| {
        cause
            .downcast_ref::<RefusedAnswer>()
            .map(|refused| refused.0.clone())
    })
}

enum Failed {
    Stopped(SafetyStop),
    Error(OAuthHttpClientError),
}

impl From<Failed> for OAuthHttpClientError {
    fn from(failed: Failed) -> Self {
        match failed {
            Failed::Stopped(stop) => Box::new(stop),
            Failed::Error(error) => error,
        }
    }
}

impl OAuthHttp {
    async fn send(
        &self,
        request: http::Request<Vec<u8>>,
        follows: bool,
        timeout: Option<Duration>,
    ) -> Result<http::Response<Vec<u8>>, OAuthHttpClientError> {
        let request_body = shown_request_body(request.body());
        let mut outgoing = reqwest::Request::try_from(request)?;
        *outgoing.timeout_mut() = Some(timeout.unwrap_or(REQUEST_LIMIT));
        let method = outgoing.method().clone();
        let url = outgoing.url().clone();
        let shown_headers = headers(&outgoing);
        let started = Instant::now();
        let result = self.deliver(outgoing, follows).await;
        // A request that failed for another reason did not complete, so an
        // earlier stop stays the best explanation.
        if result.is_ok() {
            self.notes.set_oauth_stop(None);
        }
        if self.observer.is_active() {
            let (outcome, response_body) = match &result {
                Ok(response) => (
                    response.status().to_string(),
                    shown_response_body(response.body()),
                ),
                Err(Failed::Stopped(stop)) => (
                    format!(
                        "stopped on purpose: {}",
                        stop.describe(&|text| text.to_string())
                    ),
                    None,
                ),
                Err(Failed::Error(error)) => (format!("error: {error}"), None),
            };
            self.observer.emit(&Event::Http(&HttpExchange {
                method: static_method(&method),
                url: url.to_string(),
                headers: shown_headers,
                outcome,
                elapsed: started.elapsed(),
                request_body,
                response_body,
            }));
        }
        result.map_err(OAuthHttpClientError::from)
    }

    async fn deliver(
        &self,
        request: reqwest::Request,
        follows: bool,
    ) -> Result<http::Response<Vec<u8>>, Failed> {
        let url = request.url().clone();
        let selected = (self.select)(&url);
        address::check_url(&url, self.permission, &selected.route)
            .map_err(|reason| Failed::Stopped(self.stop(&url, reason)))?;
        let client = self
            .client_for(selected.proxy.as_ref())
            .map_err(|error| Failed::Error(Box::new(error)))?;
        let response = receive(&client, request).await.map_err(|error| {
            match refused_answer(error.as_ref()) {
                Some(reason) => Failed::Stopped(self.stop(&url, reason)),
                None => Failed::Error(error),
            }
        })?;
        match redirect_of(&url, &response).filter(|_| follows) {
            Some(reason) => Err(Failed::Stopped(self.stop(&url, reason))),
            None => Ok(response),
        }
    }
}

// The client gives other redirects to rmcp, which follows only one to the
// same origin, with a new request and so a new check.
fn redirect_of(url: &Url, response: &http::Response<Vec<u8>>) -> Option<StopReason> {
    let status = response.status();
    let location = response
        .headers()
        .get(http::header::LOCATION)
        .filter(|_| status.is_redirection())?;
    let location = String::from_utf8_lossy(location.as_bytes());
    let target = url
        .join(&location)
        .map_or_else(|_| location.to_string(), String::from);
    Some(StopReason::Redirect {
        status: status.as_u16(),
        location: redact::url(&target),
    })
}

// The request body as it shows: a form or JSON with each secret hidden.
fn shown_request_body(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(body);
    Some(match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(json) => redact::oauth_json(&json).to_string(),
        Err(_) => redact::form(&text),
    })
}

// The response body as it shows: JSON with each secret hidden, else only
// its size, because an HTML page is long and says nothing new.
fn shown_response_body(body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    Some(match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(json) => redact::oauth_json(&json).to_string(),
        Err(_) => format!("({} bytes that are not JSON)", body.len()),
    })
}

fn headers(request: &reqwest::Request) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .map(|(name, value)| {
            let text = String::from_utf8_lossy(value.as_bytes());
            (
                name.as_str().to_string(),
                redact::header(name.as_str(), &text, value.is_sensitive()),
            )
        })
        .collect();
    headers.sort();
    headers
}

// Sends the request and reads the whole body, up to the limit.
async fn receive(
    client: &reqwest::Client,
    request: reqwest::Request,
) -> Result<http::Response<Vec<u8>>, OAuthHttpClientError> {
    let mut response = client.execute(request).await?;
    let mut builder = http::Response::builder()
        .status(response.status())
        .version(response.version());
    for (name, value) in response.headers() {
        builder = builder.header(name, value);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > MAX_BODY - body.len() {
            return Err(Box::new(OAuthHttpError::BodyTooLarge));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(builder.body(body)?)
}

// The method as a static name. OAuth uses only GET and POST.
fn static_method(method: &reqwest::Method) -> &'static str {
    match *method {
        reqwest::Method::GET => "GET",
        reqwest::Method::POST => "POST",
        reqwest::Method::DELETE => "DELETE",
        _ => "OTHER",
    }
}

impl OAuthHttpClient for OAuthHttp {
    fn execute(&self, request: OAuthHttpRequest) -> OAuthHttpClientFuture<'_> {
        let follows = matches!(request.redirect_policy, OAuthHttpRedirectPolicy::Follow);
        Box::pin(self.send(request.request, follows, request.timeout))
    }
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::*;

    #[test]
    fn bodies_show_with_secrets_hidden() {
        assert_eq!(
            shown_request_body(b"grant_type=refresh_token&refresh_token=rt&client_id=c"),
            Some("grant_type=refresh_token&refresh_token=****&client_id=c".into())
        );
        assert_eq!(
            shown_response_body(br#"{"access_token":"at","expires_in":60}"#),
            Some(r#"{"access_token":"****","expires_in":60}"#.into())
        );
        assert_eq!(
            shown_response_body(b"<html></html>"),
            Some("(13 bytes that are not JSON)".into())
        );
        assert_eq!(shown_request_body(b""), None);
    }

    #[derive(Default)]
    struct FixedLookup(Mutex<HashMap<String, Vec<Vec<IpAddr>>>>);

    impl FixedLookup {
        fn with(entries: &[(&str, &[&[&str]])]) -> Arc<Self> {
            let answers = entries
                .iter()
                .map(|(name, answers)| {
                    let answers = answers
                        .iter()
                        .map(|answer| answer.iter().map(|ip| ip.parse().unwrap()).collect())
                        .collect();
                    ((*name).to_string(), answers)
                })
                .collect();
            Arc::new(Self(Mutex::new(answers)))
        }
    }

    impl Resolve for FixedLookup {
        fn resolve(&self, name: Name) -> Resolving {
            let mut answers = self.0.lock().unwrap();
            let answer = match answers.get_mut(name.as_str()) {
                Some(list) if list.len() > 1 => list.remove(0),
                Some(list) => list[0].clone(),
                None => Vec::new(),
            };
            let addresses: Vec<SocketAddr> = answer
                .into_iter()
                .map(|ip| SocketAddr::new(ip, 0))
                .collect();
            Box::pin(async move { Ok(Box::new(addresses.into_iter()) as Addrs) })
        }
    }

    async fn serve(head: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let lines: Arc<Mutex<Vec<String>>> = Arc::default();
        let seen = lines.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = vec![0; 4096];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                seen.lock().unwrap().push(text);
                let _ = stream.write_all(head.as_bytes()).await;
            }
        });
        (port, lines)
    }

    const OK: &str = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}";

    fn direct() -> SelectRoute {
        Arc::new(|_: &Url| proxy::Selected {
            route: Route::Direct,
            proxy: None,
        })
    }

    fn via(proxy: &str, no: &str) -> SelectRoute {
        let matcher = hyper_util::client::proxy::matcher::Matcher::builder()
            .all(proxy.to_string())
            .no(no.to_string())
            .build();
        Arc::new(move |url: &Url| {
            let uri: http::Uri = url.as_str().parse().unwrap();
            match matcher.intercept(&uri) {
                Some(intercept) => proxy::Selected {
                    route: Route::Proxy {
                        url: intercept.uri().to_string(),
                        source: proxy::Source::Variable("ALL_PROXY"),
                    },
                    proxy: Some(intercept),
                },
                None => proxy::Selected {
                    route: Route::DirectByNoProxy,
                    proxy: None,
                },
            }
        })
    }

    fn client_on(
        entry: &str,
        private: bool,
        select: SelectRoute,
        lookup: Arc<FixedLookup>,
    ) -> (OAuthHttp, ConnectNotes) {
        let notes = ConnectNotes::default();
        let entry = Url::parse(entry).unwrap();
        let permission = Permission::of_entry(&entry, private);
        let http = OAuthHttp::with_network(
            &Env::new(),
            (&Observer::default(), &notes),
            (permission, entry.host_str().unwrap()),
            select,
            lookup,
        )
        .unwrap();
        (http, notes)
    }

    fn client(entry: &str, private: bool, lookup: Arc<FixedLookup>) -> (OAuthHttp, ConnectNotes) {
        client_on(entry, private, direct(), lookup)
    }

    fn request(method: http::Method, url: &str) -> http::Request<Vec<u8>> {
        http::Request::builder()
            .method(method)
            .uri(url)
            .body(Vec::new())
            .unwrap()
    }

    // rmcp does not follow redirects for discovery.
    async fn get(
        http: &OAuthHttp,
        url: &str,
    ) -> Result<http::Response<Vec<u8>>, OAuthHttpClientError> {
        http.send(request(http::Method::GET, url), false, None)
            .await
    }

    // rmcp asks to follow redirects for a registration.
    async fn register(
        http: &OAuthHttp,
        url: &str,
    ) -> Result<http::Response<Vec<u8>>, OAuthHttpClientError> {
        http.send(request(http::Method::POST, url), true, None)
            .await
    }

    #[tokio::test]
    async fn a_direct_hostname_request_uses_only_the_filtered_answer() {
        let (port, lines) = serve(OK).await;
        let lookup = FixedLookup::with(&[("as.localhost", &[&["93.184.216.34", "127.0.0.1"]])]);
        let (http, notes) = client("http://localhost:1/mcp", false, lookup);
        let response = get(&http, &format!("http://as.localhost:{port}/metadata"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(lines.lock().unwrap().len(), 1);
        assert_eq!(notes.get().oauth_stop, None);
    }

    #[tokio::test]
    async fn a_changed_answer_gets_the_check_again() {
        let (port, lines) = serve(OK).await;
        // The server closes each connection, so the second request looks
        // up the name again.
        let lookup = FixedLookup::with(&[("as.localhost", &[&["127.0.0.1"], &["10.0.0.5"]])]);
        let (http, notes) = client("http://localhost:1/mcp", true, lookup);
        let url = format!("http://as.localhost:{port}/metadata");
        assert!(get(&http, &url).await.is_ok());
        assert!(get(&http, &url).await.is_err());
        assert_eq!(lines.lock().unwrap().len(), 1);
        assert_eq!(
            notes.get().oauth_stop.map(|stop| stop.reason),
            Some(StopReason::NotLoopback {
                address: "10.0.0.5".parse().unwrap()
            })
        );
    }

    #[tokio::test]
    async fn a_loopback_answer_marks_the_stop_only_at_the_host_of_the_entry() {
        let lookup = FixedLookup::with(&[
            ("app.test", &[&["127.0.0.1"]]),
            ("app.test.", &[&["127.0.0.1"]]),
            ("other.test", &[&["127.0.0.1"]]),
        ]);
        let (http, notes) = client("https://app.test/mcp", false, lookup);
        for (url, on_entry_host) in [
            (
                "https://app.test/.well-known/oauth-protected-resource",
                true,
            ),
            ("https://other.test/token", false),
            ("https://app.test./token", false),
        ] {
            assert!(get(&http, url).await.is_err(), "{url}");
            let stop = notes.get().oauth_stop.unwrap();
            assert_eq!(
                stop.reason,
                StopReason::Class {
                    found: address::Found::Resolved("127.0.0.1".parse().unwrap()),
                    class: address::Refused::Loopback
                },
                "{url}"
            );
            assert_eq!(stop.url, url, "{url}");
            assert_eq!(stop.on_entry_host, on_entry_host, "{url}");
        }
    }

    async fn serve_paths(
        answer: impl Fn(u16, &str) -> String + Send + 'static,
    ) -> (u16, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let paths: Arc<Mutex<Vec<String>>> = Arc::default();
        let seen = paths.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = vec![0; 4096];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                let path = text
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                let response = answer(port, &path);
                seen.lock().unwrap().push(path);
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        (port, paths)
    }

    #[tokio::test]
    async fn a_same_origin_discovery_redirect_stops_when_the_dns_answer_changes() {
        use rmcp::transport::auth::{AuthError, AuthorizationManager};

        const FIRST: &str = "/.well-known/oauth-authorization-server";
        let (port, paths) = serve_paths(|port, path| match path {
            "/prm" => {
                let body = format!(
                    r#"{{"resource":"http://127.0.0.1:{port}/mcp","authorization_servers":["http://as.localhost:{port}"]}}"#
                );
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
            }
            FIRST => format!(
                "HTTP/1.1 307 Temporary Redirect\r\nlocation: {FIRST}/moved\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
            ),
            _ => "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".into(),
        })
        .await;
        // The closed connection makes the redirected request look up the
        // name again.
        let lookup = FixedLookup::with(&[("as.localhost", &[&["127.0.0.1"], &["10.0.0.5"]])]);
        let (http, notes) = client(&format!("http://127.0.0.1:{port}/mcp"), false, lookup);
        let manager = AuthorizationManager::new_with_oauth_http_client(
            format!("http://127.0.0.1:{port}/mcp").as_str(),
            Arc::new(http),
        )
        .await
        .unwrap();
        let challenge = format!(r#"Bearer resource_metadata="http://127.0.0.1:{port}/prm""#);
        let error = manager
            .resolve_metadata_from_challenge(Some(&challenge))
            .await
            .unwrap_err();
        let first = format!("http://as.localhost:{port}{FIRST}");
        assert!(
            matches!(&error, AuthError::DiscoveryRequestFailed { url, .. } if url.as_str() == first),
            "{error:?}"
        );
        let stop = notes.get().oauth_stop.unwrap();
        assert_eq!(stop.url, format!("{first}/moved"));
        assert_eq!(
            stop.reason,
            StopReason::NotLoopback {
                address: "10.0.0.5".parse().unwrap()
            }
        );
        assert_eq!(*paths.lock().unwrap(), ["/prm", FIRST]);
    }

    #[tokio::test]
    async fn a_private_hostname_stops_by_default_with_its_address_in_the_notes() {
        let lookup = FixedLookup::with(&[("as.corp.example", &[&["10.0.0.5"]])]);
        let (http, notes) = client("https://mcp.corp.example/mcp", false, lookup);
        let error = get(
            &http,
            "https://as.corp.example/.well-known/oauth-authorization-server",
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("on purpose"), "{error}");
        assert_eq!(
            notes.get().oauth_stop,
            Some(SafetyStop {
                url: "https://as.corp.example/.well-known/oauth-authorization-server".into(),
                host: "as.corp.example".into(),
                on_entry_host: false,
                reason: StopReason::Class {
                    found: address::Found::Resolved("10.0.0.5".parse().unwrap()),
                    class: address::Refused::Private
                }
            })
        );
    }

    #[tokio::test]
    async fn a_private_hostname_passes_the_lookup_with_the_option() {
        let lookup = FixedLookup::with(&[("as.corp.example", &[&["10.0.0.5"]])]);
        let (http, _) = client("https://mcp.corp.example/mcp", true, lookup.clone());
        assert_eq!(
            http.check_lookup("https://as.corp.example/token").await,
            Ok(())
        );
        let (http, _) = client("https://mcp.corp.example/mcp", false, lookup);
        assert!(
            http.check_lookup("https://as.corp.example/token")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn the_configured_hostname_gets_no_exemption() {
        let lookup = FixedLookup::with(&[("mcp.corp.example", &[&["10.0.0.5"]])]);
        let (http, _) = client("https://mcp.corp.example/mcp", false, lookup);
        assert!(
            http.check_lookup("https://mcp.corp.example/token")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_loopback_literal_from_a_public_entry_sends_nothing() {
        let (port, lines) = serve(OK).await;
        let (http, notes) = client("https://mcp.example.com/mcp", true, FixedLookup::with(&[]));
        assert!(
            register(&http, &format!("http://127.0.0.1:{port}/register"))
                .await
                .is_err()
        );
        assert!(lines.lock().unwrap().is_empty());
        assert!(matches!(
            notes.get().oauth_stop.map(|stop| stop.reason),
            Some(StopReason::Class {
                class: address::Refused::Loopback,
                ..
            })
        ));
    }

    // A loopback entry permits the whole loopback class, not one port. The
    // same holds when an honest tunnel on that port forwards the metadata
    // of a hostile remote server: the other port is reachable. This test
    // records that accepted authority, not a blocked attack.
    #[tokio::test]
    async fn another_loopback_port_works_for_a_loopback_entry() {
        let (port, _) = serve(OK).await;
        let (http, _) = client("http://127.0.0.1:1/mcp", false, FixedLookup::with(&[]));
        let response = get(&http, &format!("http://127.0.0.1:{port}/token"))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn a_registration_redirect_stops_with_no_second_request() {
        let (port, lines) = serve(
            "HTTP/1.1 307 Temporary Redirect\r\nlocation: /elsewhere?code=c\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
        )
        .await;
        let (http, notes) = client("http://127.0.0.1:1/mcp", false, FixedLookup::with(&[]));
        let url = format!("http://127.0.0.1:{port}/register");
        assert!(register(&http, &url).await.is_err());
        assert_eq!(lines.lock().unwrap().len(), 1);
        assert_eq!(
            notes.get().oauth_stop.map(|stop| stop.reason),
            Some(StopReason::Redirect {
                status: 307,
                location: format!("http://127.0.0.1:{port}/elsewhere?code=****"),
            })
        );
        let token = http
            .send(request(http::Method::POST, &url), false, None)
            .await;
        assert_eq!(token.unwrap().status(), 307);
        assert_eq!(lines.lock().unwrap().len(), 2);
        assert_eq!(notes.get().oauth_stop, None);
    }

    #[tokio::test]
    async fn a_completed_request_clears_an_earlier_stop() {
        let (port, _) = serve(OK).await;
        let (http, notes) = client("http://127.0.0.1:1/mcp", false, FixedLookup::with(&[]));
        assert!(get(&http, "http://as.example/x").await.is_err());
        assert_eq!(
            notes.get().oauth_stop.map(|stop| stop.reason),
            Some(StopReason::Scheme)
        );
        assert!(
            get(&http, &format!("http://127.0.0.1:{port}/x"))
                .await
                .is_ok()
        );
        assert_eq!(notes.get().oauth_stop, None);
    }

    #[test]
    fn the_browser_url_gets_the_literal_and_localhost_checks() {
        let (http, _) = client("https://mcp.example.com/mcp", false, FixedLookup::with(&[]));
        assert_eq!(
            http.check_browser_url("https://as.example/authorize"),
            Ok(())
        );
        for url in [
            "http://as.example/authorize",
            "https://localhost/authorize",
            "https://10.0.0.5/authorize",
            "https://[fe80::1]/authorize",
            "not a url",
        ] {
            assert!(http.check_browser_url(url).is_err(), "{url}");
        }
    }

    #[tokio::test]
    async fn a_proxy_route_uses_only_the_selected_proxy_with_its_credentials() {
        // The target name has no DNS answer: only the proxy resolves it.
        let (port, requests) = serve(OK).await;
        let (http, notes) = client_on(
            "http://localhost:1/mcp",
            false,
            via(&format!("http://user:pa%40ss@127.0.0.1:{port}"), ""),
            FixedLookup::with(&[]),
        );
        let response = get(&http, "http://auth.localhost:9/token").await.unwrap();
        assert_eq!(response.status(), 200);
        let requests = requests.lock().unwrap();
        let head = requests[0].to_ascii_lowercase();
        assert!(
            head.starts_with("get http://auth.localhost:9/token http/1.1"),
            "{head}"
        );
        // `user:pa@ss` in Base64.
        assert!(
            head.contains("proxy-authorization: basic dxnlcjpwyubzcw=="),
            "{head}"
        );
        assert_eq!(notes.get().oauth_stop, None);
    }

    #[tokio::test]
    async fn the_policy_does_not_filter_the_host_of_the_proxy() {
        // `localhost` is a loopback name, which a public entry cannot
        // reach. As the proxy, it gets no check: the proxy resolves the
        // target and applies its own policy.
        let (port, requests) = serve(OK).await;
        let (http, notes) = client_on(
            "https://mcp.example.com/mcp",
            false,
            via(&format!("http://localhost:{port}"), ""),
            FixedLookup::with(&[]),
        );
        let _ = get(
            &http,
            "https://as.example/.well-known/oauth-authorization-server",
        )
        .await;
        let requests = requests.lock().unwrap();
        assert!(
            requests[0].starts_with("CONNECT as.example:443 "),
            "{requests:?}"
        );
        assert_eq!(notes.get().oauth_stop, None);
    }

    #[tokio::test]
    async fn no_proxy_takes_the_direct_route_with_the_full_check() {
        let (_, requests) = serve(OK).await;
        let lookup = FixedLookup::with(&[
            ("as.corp.example", &[&["10.0.0.5"]]),
            ("proxy.corp.example", &[&["10.0.0.9"]]),
        ]);
        let (http, notes) = client_on(
            "https://mcp.corp.example/mcp",
            false,
            via(
                "http://proxy.corp.example:3128",
                "as.corp.example,proxy.corp.example",
            ),
            lookup,
        );
        assert!(
            http.check_lookup("https://as.corp.example/token")
                .await
                .is_err()
        );
        assert!(get(&http, "https://proxy.corp.example/x").await.is_err());
        assert_eq!(
            notes.get().oauth_stop.map(|stop| stop.reason),
            Some(StopReason::Class {
                found: address::Found::Resolved("10.0.0.9".parse().unwrap()),
                class: address::Refused::Private
            })
        );
        // On the proxy route, the proxy resolves the name, so the local
        // lookup does not apply.
        assert_eq!(
            http.check_lookup("https://as.other.example/token").await,
            Ok(())
        );
        assert!(requests.lock().unwrap().is_empty());
    }

    #[test]
    fn a_url_that_cannot_be_parsed_stops_with_its_own_reason() {
        let (http, notes) = client("https://mcp.example.com/mcp", false, FixedLookup::with(&[]));
        assert!(
            http.check_request_url("https://exa mple.com/token")
                .is_err()
        );
        assert_eq!(
            notes.get().oauth_stop.map(|stop| stop.reason),
            Some(StopReason::NotValid)
        );
    }
}
