//! The HTTP client and the Streamable HTTP transport.
//!
//! `ncp` gives rmcp its own `reqwest::Client`. The client reads the proxy
//! variables and keeps connections for the next request. It uses the system
//! root certificates, or only the roots in `SSL_CERT_FILE` when that variable
//! has a value.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};

use reqwest::header::{HeaderName, HeaderValue};
use rmcp::transport::{
    DynamicTransportError, StreamableHttpClientTransport,
    auth::AuthClient,
    streamable_http_client::{
        StreamableHttpClient, StreamableHttpClientTransportConfig, StreamableHttpError,
    },
};

use super::observe::{
    Observed as ObservedTransport, ObservedConnectLayer, ObservedHttp, ObservedResolver, Observer,
};
use crate::{
    config::{Expanded, RemoteServer},
    expand::Env,
    text::code,
};

/// The variable that names a file of extra root certificates, as curl,
/// OpenSSL and uv use it.
const CERT_FILE: &str = "SSL_CERT_FILE";

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("ncp cannot make the HTTP client")]
    Client(#[from] reqwest::Error),
    #[error("The header name {} is not valid", code(.0))]
    HeaderName(String),
    #[error("The header {} has a value that is not valid in a header", code(.0))]
    HeaderValue(String),
    #[error("ncp cannot use the root certificates in {} from `{CERT_FILE}`", code(.path))]
    CertFile {
        path: String,
        #[source]
        reason: CertReason,
    },
}

/// Why `ncp` cannot use the file of root certificates.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct CertReason(pub String);

/// The builder of the HTTP client, with the proxy variables of the process.
/// Make one client for each process.
pub fn builder() -> reqwest::ClientBuilder {
    // reqwest with `rustls-no-provider` needs a process-wide crypto provider.
    // A second call finds the provider installed, which is not an error.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
}

/// The client of the MCP connection. The observer gets the time of each
/// DNS lookup and each new connection.
///
/// The client follows no redirect, because the headers of an entry can
/// hold credentials. At a redirect to another host, reqwest removes only
/// the standard credential headers. rmcp has the same default, so that
/// custom headers do not go to the target of a redirect.
///
/// rmcp keeps only the text of a transport error, with no cause. Thus the
/// notes keep the new location of a redirect and a certificate of an
/// unknown issuer, so that the error can name them.
///
/// # Errors
///
/// Returns an error when the file in `SSL_CERT_FILE` gives no root
/// certificate, or when reqwest cannot make the client.
pub fn client(
    env: &Env,
    observer: &Observer,
    notes: &ConnectNotes,
) -> Result<reqwest::Client, HttpError> {
    let redirects = notes.clone();
    let stop = reqwest::redirect::Policy::custom(move |attempt| {
        redirects.update(|notes| {
            notes.redirect = Some(Redirected {
                status: attempt.status().as_u16(),
                location: crate::redact::url(attempt.url().as_str()),
            });
        });
        attempt.stop()
    });
    Ok(noted(configured(env, observer)?, notes)
        .redirect(stop)
        .build()?)
}

/// The builder with a layer that writes a certificate of an unknown issuer
/// to the notes.
pub fn noted(builder: reqwest::ClientBuilder, notes: &ConnectNotes) -> reqwest::ClientBuilder {
    builder.connector_layer(NotedConnectLayer {
        notes: notes.clone(),
    })
}

/// A redirect of the MCP connection. The location shows with its secrets
/// hidden.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "The server answered {status} with a redirect to {}. ncp follows no redirect.",
    code(.location)
)]
pub struct Redirected {
    pub status: u16,
    pub location: String,
}

/// What the HTTP client saw that a transport error of rmcp does not say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notes {
    /// The last redirect that the client did not follow.
    pub redirect: Option<Redirected>,
    /// A TLS connection failed: the certificate of the server has an
    /// issuer that no trusted root names.
    pub unknown_issuer: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ConnectNotes(Arc<Mutex<Notes>>);

impl ConnectNotes {
    fn update(&self, change: impl FnOnce(&mut Notes)) {
        if let Ok(mut notes) = self.0.lock() {
            change(&mut notes);
        }
    }

    #[must_use]
    pub fn get(&self) -> Notes {
        self.0.lock().map(|notes| notes.clone()).unwrap_or_default()
    }
}

/// A layer on the reqwest connector that notes a certificate of an unknown
/// issuer.
#[derive(Clone)]
struct NotedConnectLayer {
    notes: ConnectNotes,
}

impl<S> tower_layer::Layer<S> for NotedConnectLayer {
    type Service = NotedConnect<S>;

    fn layer(&self, inner: S) -> Self::Service {
        NotedConnect {
            inner,
            notes: self.notes.clone(),
        }
    }
}

#[derive(Clone)]
struct NotedConnect<S> {
    inner: S,
    notes: ConnectNotes,
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

impl<S, R> tower_service::Service<R> for NotedConnect<S>
where
    S: tower_service::Service<R, Error = BoxError>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = BoxError;
    type Future = std::pin::Pin<Box<dyn Future<Output = Result<S::Response, BoxError>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: R) -> Self::Future {
        let notes = self.notes.clone();
        let connect = self.inner.call(request);
        Box::pin(async move {
            let result = connect.await;
            if let Err(error) = &result
                && is_unknown_issuer(error.as_ref())
            {
                notes.update(|notes| notes.unknown_issuer = true);
            }
            result
        })
    }
}

// True when the error or a cause is the rustls error for a certificate of
// an unknown issuer. An `io::Error` hides its inner error from `source`,
// so the search goes into it.
fn is_unknown_issuer(error: &(dyn std::error::Error + 'static)) -> bool {
    std::iter::successors(Some(error), |error| {
        error
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::get_ref)
            .map(|inner| inner as &(dyn std::error::Error + 'static))
            .or_else(|| error.source())
    })
    .any(|error| {
        matches!(
            error.downcast_ref::<rustls::Error>(),
            Some(rustls::Error::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer
            ))
        )
    })
}

/// The builder with the settings of each client of `ncp`: the proxy, the
/// roots of `SSL_CERT_FILE`, and the observer. The OAuth requests use it
/// too, so that the same proxy and root rules apply to them.
///
/// # Errors
///
/// Returns an error when the file in `SSL_CERT_FILE` gives no root
/// certificate.
pub fn configured(env: &Env, observer: &Observer) -> Result<reqwest::ClientBuilder, HttpError> {
    let builder = builder()
        .dns_resolver(std::sync::Arc::new(ObservedResolver {
            observer: observer.clone(),
        }))
        .connector_layer(ObservedConnectLayer {
            observer: observer.clone(),
        });
    with_cert_file(builder, env)
}

// Replaces the system roots with the roots in the file that `SSL_CERT_FILE`
// names, on each OS, as uv does. On Linux, the system store reads the variable
// in place of the system roots, so a merge gives a different rule on each OS.
// A file that `ncp` cannot use is an error, because a silent fallback gives a
// TLS error that does not name the cause.
fn with_cert_file(
    builder: reqwest::ClientBuilder,
    env: &Env,
) -> Result<reqwest::ClientBuilder, HttpError> {
    let Some(path) = env.get(CERT_FILE) else {
        return Ok(builder);
    };
    // On Linux, an empty value loads no root at all. The same error on each
    // host is clearer.
    if path.is_empty() {
        return Err(HttpError::CertFile {
            path: String::new(),
            reason: CertReason("The variable is empty".to_string()),
        });
    }
    let failed = |reason: String| HttpError::CertFile {
        path: path.clone(),
        reason: CertReason(reason),
    };
    let pem = std::fs::read(path).map_err(|error| failed(error.to_string()))?;
    let roots =
        reqwest::Certificate::from_pem_bundle(&pem).map_err(|error| failed(error.to_string()))?;
    if roots.is_empty() {
        return Err(failed("The file has no PEM certificate".to_string()));
    }
    Ok(builder.tls_certs_only(roots))
}

// A value from `${NAME}` gets the sensitive mark, so debug output hides
// it.
fn headers(
    config: &BTreeMap<String, Expanded>,
) -> Result<HashMap<HeaderName, HeaderValue>, HttpError> {
    config
        .iter()
        .map(|(name, expanded)| header(name, &expanded.value, expanded.from_variable))
        .collect()
}

fn header(
    name: &str,
    value: &str,
    sensitive: bool,
) -> Result<(HeaderName, HeaderValue), HttpError> {
    let header_name = HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| HttpError::HeaderName(name.to_string()))?;
    let mut header_value =
        HeaderValue::from_str(value).map_err(|_| HttpError::HeaderValue(name.to_string()))?;
    header_value.set_sensitive(sensitive);
    Ok((header_name, header_value))
}

pub type Observed = ObservedHttp<reqwest::Client>;

/// The HTTP client that sends the OAuth access token of the server, and
/// refreshes it after a `401` (rmcp `AuthClient`).
pub type Authorized = AuthClient<Observed>;

#[must_use]
pub fn observed(http: &reqwest::Client, observer: &Observer) -> Observed {
    ObservedHttp {
        inner: http.clone(),
        observer: observer.clone(),
    }
}

/// Makes the transport for one connection, with the expanded headers of the
/// entry. The observer gets each message.
///
/// # Errors
///
/// Returns an error for a header name or value that HTTP does not permit.
pub fn transport<C: StreamableHttpClient>(
    client: C,
    server: &RemoteServer,
    expanded: &BTreeMap<String, Expanded>,
    observer: &Observer,
) -> Result<ObservedTransport<StreamableHttpClientTransport<C>>, HttpError> {
    let config = StreamableHttpClientTransportConfig::with_uri(server.url.as_str())
        .custom_headers(headers(expanded)?);
    Ok(ObservedTransport::new(
        StreamableHttpClientTransport::with_client(client, config),
        observer.clone(),
    ))
}

/// True when the server answered `401` or `403`: it needs a login or more
/// scopes.
#[must_use]
pub fn needs_authorization(error: &DynamicTransportError) -> bool {
    matches!(
        error
            .error
            .downcast_ref::<StreamableHttpError<reqwest::Error>>(),
        Some(StreamableHttpError::AuthRequired(_) | StreamableHttpError::InsufficientScope(_))
    )
}

/// The `WWW-Authenticate` challenge of a `401` or `403` answer. It names
/// the protected resource metadata and the scopes (RFC 9728 §5.1).
#[must_use]
pub fn challenge(error: &DynamicTransportError) -> Option<String> {
    match error
        .error
        .downcast_ref::<StreamableHttpError<reqwest::Error>>()
    {
        Some(StreamableHttpError::AuthRequired(required)) => {
            Some(required.www_authenticate_header.clone())
        }
        Some(StreamableHttpError::InsufficientScope(insufficient)) => {
            Some(insufficient.www_authenticate_header.clone())
        }
        _ => None,
    }
}

/// The error of the credential store of the OAuth tokens, for example a
/// refresh whose new tokens the store cannot keep.
#[must_use]
pub fn token_store_error(error: &DynamicTransportError) -> Option<String> {
    match error
        .error
        .downcast_ref::<StreamableHttpError<reqwest::Error>>()
    {
        Some(StreamableHttpError::Auth(
            rmcp::transport::auth::AuthError::CredentialStoreError(text),
        )) => Some(text.clone()),
        _ => None,
    }
}

/// The challenge of a `401` answer: the server needs a login.
#[must_use]
pub fn login_challenge(error: &DynamicTransportError) -> Option<String> {
    match error
        .error
        .downcast_ref::<StreamableHttpError<reqwest::Error>>()
    {
        Some(StreamableHttpError::AuthRequired(required)) => {
            Some(required.www_authenticate_header.clone())
        }
        _ => None,
    }
}

/// The scopes that the server asked for in its `403 insufficient_scope`
/// answer, or `None` for another error.
#[must_use]
pub fn required_scopes(error: &DynamicTransportError) -> Option<Vec<String>> {
    match error
        .error
        .downcast_ref::<StreamableHttpError<reqwest::Error>>()
    {
        Some(StreamableHttpError::InsufficientScope(insufficient)) => Some(
            insufficient
                .required_scope
                .as_deref()
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_string)
                .collect(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_texts_follow_the_message_rules() {
        let texts = [
            HttpError::HeaderName("x y".into()).to_string(),
            HttpError::HeaderValue("x".into()).to_string(),
            HttpError::CertFile {
                path: "/roots.pem".into(),
                reason: CertReason("The file has no PEM certificate".into()),
            }
            .to_string(),
            Redirected {
                status: 308,
                location: "https://h/".into(),
            }
            .to_string(),
        ];
        crate::style::assert_style(&texts);
    }

    fn expanded(value: &str, from_variable: bool) -> Expanded {
        Expanded {
            value: value.into(),
            from_variable,
        }
    }

    #[test]
    fn header_from_a_variable_is_hidden_in_debug_output() {
        let config = BTreeMap::from([
            ("X-Org".to_string(), expanded("o", false)),
            ("Authorization".to_string(), expanded("Bearer t0ken", true)),
        ]);
        let headers = headers(&config).unwrap();
        let authorization = &headers[&HeaderName::from_static("authorization")];
        assert_eq!(authorization, "Bearer t0ken");
        assert!(authorization.is_sensitive());
        assert!(!format!("{headers:?}").contains("t0ken"));
        assert_eq!(headers[&HeaderName::from_static("x-org")], "o");
    }

    fn env(pairs: &[(&str, &str)]) -> Env {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn empty_cert_file_variable_is_an_error_on_each_host() {
        let error = client(
            &env(&[(CERT_FILE, "")]),
            &Observer::default(),
            &ConnectNotes::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("SSL_CERT_FILE"), "{error}");
    }

    #[test]
    fn cert_file_with_no_certificate_names_the_file_and_the_variable() {
        let dir = std::env::temp_dir().join(format!("ncp-cert-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.pem");
        std::fs::write(&path, "not a certificate\n").unwrap();
        let path = path.to_string_lossy().to_string();
        let error = client(
            &env(&[(CERT_FILE, &path)]),
            &Observer::default(),
            &ConnectNotes::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains(&path) && error.contains(CERT_FILE),
            "{error}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_header_value_with_line_break() {
        let config = BTreeMap::from([("X".to_string(), expanded("a\r\nB: c", false))]);
        assert!(matches!(headers(&config), Err(HttpError::HeaderValue(_))));
    }
}
