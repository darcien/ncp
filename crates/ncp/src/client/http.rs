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

use crate::config::{Expanded, RemoteServer};

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
) -> Result<StreamableHttpClientTransport<reqwest::Client>, HttpError> {
    let config = StreamableHttpClientTransportConfig::with_uri(server.url.as_str())
        .custom_headers(headers(expanded)?);
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

    #[test]
    fn rejects_header_value_with_line_break() {
        let config = BTreeMap::from([("X".to_string(), expanded("a\r\nB: c", false))]);
        assert!(matches!(headers(&config), Err(HttpError::HeaderValue(_))));
    }
}
