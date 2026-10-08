//! The HTTP client and the Streamable HTTP transport.
//!
//! `ncp` gives rmcp its own `reqwest::Client`. The client reads the proxy
//! variables, uses the system root certificates, and keeps connections for
//! the next request.

use std::collections::{BTreeMap, HashMap};

use reqwest::header::{HeaderName, HeaderValue};
use rmcp::transport::{
    DynamicTransportError, StreamableHttpClientTransport,
    streamable_http_client::{StreamableHttpClientTransportConfig, StreamableHttpError},
};

use crate::{
    config::RemoteServer,
    credential::{self, Secret},
};

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("cannot make the HTTP client: {0}")]
    Client(#[from] reqwest::Error),
    #[error("header {0:?} is not a valid header name")]
    HeaderName(String),
    #[error("header {0:?} has a value that is not valid in a header")]
    HeaderValue(String),
}

/// The builder of the HTTP client, with the proxy variables of the process.
/// Make one client for each process.
pub fn builder() -> reqwest::ClientBuilder {
    // reqwest with `rustls-no-provider` needs a process-wide crypto provider.
    // A second call finds the provider installed, which is not an error.
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
}

/// # Errors
///
/// Returns an error when reqwest cannot make the client.
pub fn client() -> Result<reqwest::Client, HttpError> {
    Ok(builder().build()?)
}

/// The headers of the entry, then the header secrets from the credential
/// store. A secret value is marked sensitive, so debug output hides it.
fn headers(
    config: &BTreeMap<String, String>,
    secrets: &BTreeMap<credential::HeaderName, Secret>,
) -> Result<HashMap<HeaderName, HeaderValue>, HttpError> {
    let plain = config
        .iter()
        .map(|(name, value)| header(name, value, false));
    let secret = secrets
        .iter()
        .map(|(name, secret)| header(name.as_str(), secret.expose(), true));
    plain.chain(secret).collect()
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

/// Makes the transport for one connection. The headers are the headers of
/// the entry and the header secrets.
///
/// # Errors
///
/// Returns an error for a header name or value that HTTP does not permit.
pub fn transport(
    http: &reqwest::Client,
    server: &RemoteServer,
    secrets: &BTreeMap<credential::HeaderName, Secret>,
) -> Result<StreamableHttpClientTransport<reqwest::Client>, HttpError> {
    let config = StreamableHttpClientTransportConfig::with_uri(server.url.as_str())
        .custom_headers(headers(&server.headers, secrets)?);
    Ok(StreamableHttpClientTransport::with_client(
        http.clone(),
        config,
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

    #[test]
    fn secret_header_replaces_entry_header_and_is_hidden_in_debug_output() {
        let config = BTreeMap::from([
            ("X-Org".to_string(), "o".to_string()),
            ("Authorization".to_string(), "none".to_string()),
        ]);
        let secrets = BTreeMap::from([(
            credential::HeaderName::parse("Authorization").unwrap(),
            Secret::new("Bearer t0ken".into()),
        )]);
        let headers = headers(&config, &secrets).unwrap();
        let authorization = &headers[&HeaderName::from_static("authorization")];
        assert_eq!(authorization, "Bearer t0ken");
        assert!(authorization.is_sensitive());
        assert!(!format!("{headers:?}").contains("t0ken"));
        assert_eq!(headers[&HeaderName::from_static("x-org")], "o");
    }

    #[test]
    fn rejects_header_value_with_line_break() {
        let config = BTreeMap::from([("X".to_string(), "a\r\nB: c".to_string())]);
        assert!(matches!(
            headers(&config, &BTreeMap::new()),
            Err(HttpError::HeaderValue(_))
        ));
    }
}
