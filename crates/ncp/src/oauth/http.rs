//! The HTTP client of the OAuth requests: discovery, registration, token
//! and refresh.
//!
//! rmcp sends each OAuth request through this client. It has the settings
//! of the MCP client (proxy, `SSL_CERT_FILE`), and the observer gets each
//! request with its secrets hidden, so that `-vv` shows OAuth traffic too.

use std::time::Duration;

use rmcp::transport::auth::{
    OAuthHttpClient, OAuthHttpClientError, OAuthHttpClientFuture, OAuthHttpRedirectPolicy,
    OAuthHttpRequest,
};
use tokio::time::Instant;

use crate::{
    client::{
        http::{HttpError, configured},
        observe::{Event, HttpExchange, Observer},
    },
    expand::Env,
    redact,
};

/// The largest response body. An OAuth response is small; a larger body is
/// an error of the server.
const MAX_BODY: usize = 1024 * 1024;

/// The limit of one OAuth request when rmcp gives none.
const REQUEST_LIMIT: Duration = Duration::from_secs(30);

#[derive(Debug, thiserror::Error)]
enum OAuthHttpError {
    #[error("the response body is larger than {MAX_BODY} bytes")]
    BodyTooLarge,
}

pub struct OAuthHttp {
    follow: reqwest::Client,
    /// The token requests must not follow a redirect: rmcp asks for this.
    stop: reqwest::Client,
    observer: Observer,
}

impl OAuthHttp {
    /// # Errors
    ///
    /// Returns an error when the file in `SSL_CERT_FILE` gives no root
    /// certificate, or when reqwest cannot make the client.
    pub fn new(env: &Env, observer: &Observer) -> Result<Self, HttpError> {
        Ok(Self {
            follow: configured(env, observer)?.build()?,
            stop: configured(env, observer)?
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            observer: observer.clone(),
        })
    }
}

/// The request body as it shows: a form or JSON with each secret hidden.
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

/// The response body as it shows: JSON with each secret hidden, else only
/// its size, because an HTML page is long and says nothing new.
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
            let text = value.to_str().unwrap_or_default();
            (
                name.as_str().to_string(),
                redact::header(name.as_str(), text, value.is_sensitive()),
            )
        })
        .collect();
    headers.sort();
    headers
}

impl OAuthHttp {
    async fn send(
        &self,
        request: OAuthHttpRequest,
    ) -> Result<http::Response<Vec<u8>>, OAuthHttpClientError> {
        let client = match request.redirect_policy {
            OAuthHttpRedirectPolicy::Stop => &self.stop,
            _ => &self.follow,
        };
        let request_body = shown_request_body(request.request.body());
        let mut outgoing = reqwest::Request::try_from(request.request)?;
        *outgoing.timeout_mut() = Some(request.timeout.unwrap_or(REQUEST_LIMIT));
        let method = outgoing.method().clone();
        let url = outgoing.url().to_string();
        let shown_headers = headers(&outgoing);
        let started = Instant::now();
        let result = receive(client, outgoing).await;
        if self.observer.is_active() {
            let (outcome, response_body) = match &result {
                Ok(response) => (
                    response.status().to_string(),
                    shown_response_body(response.body()),
                ),
                Err(error) => (format!("error: {error}"), None),
            };
            self.observer.emit(&Event::Http(&HttpExchange {
                method: static_method(&method),
                url,
                headers: shown_headers,
                outcome,
                elapsed: started.elapsed(),
                request_body,
                response_body,
            }));
        }
        result
    }
}

/// Sends the request and reads the whole body, up to the limit.
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

/// The method as a static name. OAuth uses only GET and POST.
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
        Box::pin(self.send(request))
    }
}

#[cfg(test)]
mod tests {
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
}
