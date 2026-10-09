//! The redirect of the authorization server back to `ncp`.
//!
//! The redirect URI is `http://127.0.0.1:<port>/callback` with a random
//! port. RFC 8252 §7.3: the authorization server "MUST allow any port to be
//! specified at the time of the request for loopback IP redirect URIs".
//! RFC 8252 §8.3: use the loopback IP literal, not `localhost`.

use std::fmt;

const HOST: &str = "127.0.0.1";
const PATH: &str = "/callback";

/// The redirect URI for a port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectUri {
    port: u16,
}

impl RedirectUri {
    #[must_use]
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl fmt::Display for RedirectUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "http://{HOST}:{}{PATH}", self.port)
    }
}

/// The parts of a redirect that the token request needs. rmcp checks
/// `state` against the stored PKCE verifier and `iss` against the metadata
/// (RFC 9207 §2.4). `ncp` checks `state` too, so that a pasted URL of
/// another login gives a clear error.
#[derive(Clone, PartialEq, Eq)]
pub struct Callback {
    pub code: String,
    pub state: String,
    pub iss: Option<String>,
}

/// What a redirect of this login must have: the state of the login, and
/// the issuer of the metadata in `iss` (RFC 9207 §2.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected {
    pub state: String,
    pub issuer: String,
    /// The metadata says `authorization_response_iss_parameter_supported`,
    /// so a redirect with no `iss` is not valid.
    pub iss_required: bool,
}

impl CallbackError {
    /// True when the redirect is for this login: it has the state of the
    /// login. The listener ignores a redirect that is not, because any
    /// local program or web page can send one.
    #[must_use]
    pub fn is_for_this_login(&self) -> bool {
        matches!(
            self,
            Self::Refused { .. } | Self::Missing("code" | "iss") | Self::WrongIssuer(_)
        )
    }
}

impl fmt::Debug for Callback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Callback")
            .field("code", &"<hidden>")
            .field("state", &self.state)
            .field("iss", &self.iss)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallbackError {
    #[error("the text is not a URL")]
    NotUrl,
    #[error("the URL does not start with {0}")]
    WrongTarget(String),
    #[error("the authorization server refused the login: {}", describe(.error, .description.as_deref()))]
    Refused {
        error: String,
        description: Option<String>,
    },
    #[error("the redirect URL has no {0} parameter")]
    Missing(&'static str),
    #[error("the redirect URL is from another login: its state does not agree")]
    WrongState,
    #[error(
        "the redirect URL names the issuer {0:?}, which is not the authorization server of the login"
    )]
    WrongIssuer(String),
}

fn describe(error: &str, description: Option<&str>) -> String {
    match description {
        Some(description) => format!("{error} ({description})"),
        None => error.to_string(),
    }
}

/// Reads a redirect URL: from the listener, or pasted by the user.
///
/// # Errors
///
/// Returns an error when the URL is not for this redirect URI, when the
/// authorization server sent an error (RFC 6749 §4.1.2.1), when a part is
/// missing, or when `state` is not the state of this login.
pub fn parse(
    text: &str,
    redirect: &RedirectUri,
    expected: &Expected,
) -> Result<Callback, CallbackError> {
    let url = url::Url::parse(text.trim()).map_err(|_| CallbackError::NotUrl)?;
    let target_agrees = url.scheme() == "http"
        && url.host_str() == Some(HOST)
        && url.port() == Some(redirect.port())
        && url.path() == PATH;
    if !target_agrees {
        return Err(CallbackError::WrongTarget(redirect.to_string()));
    }
    let find = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    // The state comes first: an error response has the state too (RFC 6749
    // §4.1.2.1), and a redirect of another login must not end this one.
    let found_state = find("state").ok_or(CallbackError::Missing("state"))?;
    if found_state != expected.state {
        return Err(CallbackError::WrongState);
    }
    if let Some(error) = find("error") {
        // rmcp checks `iss` only on a code. The text of an error response
        // shows only after the check, because a mix-up attack can send
        // an error with a false description (MCP 2026-07-28,
        // authorization).
        check_issuer(find("iss"), expected)?;
        return Err(CallbackError::Refused {
            error,
            description: find("error_description"),
        });
    }
    Ok(Callback {
        code: find("code").ok_or(CallbackError::Missing("code"))?,
        state: found_state,
        iss: find("iss"),
    })
}

fn check_issuer(iss: Option<String>, expected: &Expected) -> Result<(), CallbackError> {
    match iss {
        Some(iss) if iss != expected.issuer => Err(CallbackError::WrongIssuer(iss)),
        None if expected.iss_required => Err(CallbackError::Missing("iss")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redirect() -> RedirectUri {
        RedirectUri::new(4711)
    }

    fn login() -> Expected {
        Expected {
            state: "s1".into(),
            issuer: "https://as.test".into(),
            iss_required: false,
        }
    }

    #[test]
    fn redirect_uri_uses_the_loopback_ip_literal_and_the_port() {
        assert_eq!(redirect().to_string(), "http://127.0.0.1:4711/callback");
    }

    #[test]
    fn reads_code_state_and_issuer() {
        let callback = parse(
            " http://127.0.0.1:4711/callback?code=c1&state=s1&iss=https%3A%2F%2Fas.test\n",
            &redirect(),
            &login(),
        )
        .unwrap();
        assert_eq!(callback.code, "c1");
        assert_eq!(callback.iss.as_deref(), Some("https://as.test"));
        assert!(!format!("{callback:?}").contains("c1"));
    }

    #[test]
    fn rejects_redirect_with_wrong_state() {
        assert_eq!(
            parse(
                "http://127.0.0.1:4711/callback?code=c&state=x",
                &redirect(),
                &login()
            ),
            Err(CallbackError::WrongState)
        );
    }

    #[test]
    fn rejects_redirect_to_another_target() {
        for text in [
            "http://localhost:4711/callback?code=c&state=s1",
            "http://127.0.0.1:4712/callback?code=c&state=s1",
            "https://127.0.0.1:4711/callback?code=c&state=s1",
            "http://127.0.0.1:4711/other?code=c&state=s1",
        ] {
            assert!(
                matches!(
                    parse(text, &redirect(), &login()),
                    Err(CallbackError::WrongTarget(_))
                ),
                "{text}"
            );
        }
        assert_eq!(
            parse("code=c", &redirect(), &login()),
            Err(CallbackError::NotUrl)
        );
    }

    #[test]
    fn reports_access_denied_from_the_authorization_server() {
        let error = parse(
            "http://127.0.0.1:4711/callback?error=access_denied&error_description=no+consent&state=s1",
            &redirect(),
            &login(),
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "the authorization server refused the login: access_denied (no consent)"
        );
    }

    #[test]
    fn error_with_another_issuer_shows_no_description() {
        let error = parse(
            "http://127.0.0.1:4711/callback?error=access_denied&error_description=call+us&state=s1&iss=https%3A%2F%2Fevil.test",
            &redirect(),
            &login(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            CallbackError::WrongIssuer("https://evil.test".into())
        );
        assert!(!error.to_string().contains("call us"));
        assert!(error.is_for_this_login());
        let required = Expected {
            iss_required: true,
            ..login()
        };
        assert_eq!(
            parse(
                "http://127.0.0.1:4711/callback?error=access_denied&error_description=x&state=s1",
                &redirect(),
                &required
            ),
            Err(CallbackError::Missing("iss"))
        );
    }

    #[test]
    fn error_of_another_login_is_not_for_this_login() {
        let forged = parse(
            "http://127.0.0.1:4711/callback?error=access_denied",
            &redirect(),
            &login(),
        )
        .unwrap_err();
        assert!(!forged.is_for_this_login(), "{forged:?}");
        let refused = parse(
            "http://127.0.0.1:4711/callback?error=access_denied&state=s1",
            &redirect(),
            &login(),
        )
        .unwrap_err();
        assert!(refused.is_for_this_login(), "{refused:?}");
    }

    #[test]
    fn missing_code_or_state_is_an_error() {
        assert_eq!(
            parse(
                "http://127.0.0.1:4711/callback?state=s1",
                &redirect(),
                &login()
            ),
            Err(CallbackError::Missing("code"))
        );
        assert_eq!(
            parse(
                "http://127.0.0.1:4711/callback?code=c",
                &redirect(),
                &login()
            ),
            Err(CallbackError::Missing("state"))
        );
    }
}
