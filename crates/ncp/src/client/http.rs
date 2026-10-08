//! The HTTP client and the Streamable HTTP transport.
//!
//! `ncp` gives rmcp its own `reqwest::Client`. The client reads the proxy
//! variables, uses the system root certificates and the roots in
//! `SSL_CERT_FILE`, and keeps connections for the next request.

use std::collections::{BTreeMap, HashMap};

use reqwest::header::{HeaderName, HeaderValue};
use rmcp::transport::{
    DynamicTransportError, StreamableHttpClientTransport,
    streamable_http_client::{StreamableHttpClientTransportConfig, StreamableHttpError},
};

use super::observe::{Observed, ObservedConnectLayer, ObservedHttp, ObservedResolver, Observer};
use crate::{
    config::{Expanded, RemoteServer},
    expand::Env,
};

/// The variable that names a file of extra root certificates, as curl,
/// OpenSSL and uv use it.
const CERT_FILE: &str = "SSL_CERT_FILE";

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("cannot make the HTTP client: {0}")]
    Client(#[from] reqwest::Error),
    #[error("header {0:?} is not a valid header name")]
    HeaderName(String),
    #[error("header {0:?} has a value that is not valid in a header")]
    HeaderValue(String),
    #[error("cannot use the root certificates in {path} ({CERT_FILE}): {reason}")]
    CertFile { path: String, reason: String },
}

/// The builder of the HTTP client, with the proxy variables of the process.
/// Make one client for each process.
pub fn builder() -> reqwest::ClientBuilder {
    // reqwest with `rustls-no-provider` needs a process-wide crypto provider.
    // A second call finds the provider installed, which is not an error.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
}

/// The client of `ncp`. The observer gets the time of each DNS lookup and
/// each new connection.
///
/// # Errors
///
/// Returns an error when the file in `SSL_CERT_FILE` gives no root
/// certificate, or when reqwest cannot make the client.
pub fn client(env: &Env, observer: &Observer) -> Result<reqwest::Client, HttpError> {
    Ok(configured(env, observer)?.build()?)
}

/// The builder with the settings of each client of `ncp`: the proxy, the
/// roots of `SSL_CERT_FILE`, and the observer. The OAuth requests use it
/// too, so that the same rules apply to them (requirement N2).
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

/// Adds the roots in the file that `SSL_CERT_FILE` names to the system
/// roots, for a host with no usable system store. A file that `ncp` cannot
/// use is an error, because a silent fallback gives a TLS error that does
/// not name the cause.
fn with_cert_file(
    builder: reqwest::ClientBuilder,
    env: &Env,
) -> Result<reqwest::ClientBuilder, HttpError> {
    let Some(path) = env.get(CERT_FILE).filter(|path| !path.is_empty()) else {
        return Ok(builder);
    };
    let failed = |reason: String| HttpError::CertFile {
        path: path.clone(),
        reason,
    };
    let pem = std::fs::read(path).map_err(|error| failed(error.to_string()))?;
    let roots =
        reqwest::Certificate::from_pem_bundle(&pem).map_err(|error| failed(error.to_string()))?;
    if roots.is_empty() {
        return Err(failed("the file has no PEM certificate".to_string()));
    }
    Ok(builder.tls_certs_merge(roots))
}

/// The expanded headers of the entry. A value from `${NAME}` is marked
/// sensitive, so debug output hides it.
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

/// The transport of one connection. The observer gets each HTTP request
/// and each message.
pub type Transport = Observed<StreamableHttpClientTransport<ObservedHttp<reqwest::Client>>>;

/// Makes the transport for one connection, with the expanded headers of the
/// entry.
///
/// # Errors
///
/// Returns an error for a header name or value that HTTP does not permit.
pub fn transport(
    http: &reqwest::Client,
    server: &RemoteServer,
    expanded: &BTreeMap<String, Expanded>,
    observer: &Observer,
) -> Result<Transport, HttpError> {
    let config = StreamableHttpClientTransportConfig::with_uri(server.url.as_str())
        .custom_headers(headers(expanded)?);
    let client = ObservedHttp {
        inner: http.clone(),
        observer: observer.clone(),
    };
    Ok(Observed::new(
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn empty_cert_file_variable_is_ignored() {
        assert!(client(&env(&[(CERT_FILE, "")]), &Observer::default()).is_ok());
    }

    #[test]
    fn cert_file_with_no_certificate_names_the_file_and_the_variable() {
        let dir = std::env::temp_dir().join(format!("ncp-cert-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.pem");
        std::fs::write(&path, "not a certificate\n").unwrap();
        let path = path.to_string_lossy().to_string();
        let error = client(&env(&[(CERT_FILE, &path)]), &Observer::default())
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
