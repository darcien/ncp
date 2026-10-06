//! The credential model: the kinds of secrets, and the key of each secret.
//!
//! All secrets go to the credential store, never to the configuration file.
//! A key names one secret of one server.

use std::fmt;

use crate::{expand::is_variable_name, server::ServerName};

/// The service name of all `ncp` items in the credential store.
pub const SERVICE: &str = "ncp";

/// A secret value. `Debug` hides the value, so that a secret cannot go to a
/// log by accident. There is no `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    #[must_use]
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// The value, for the one place that sends it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<hidden>)")
    }
}

/// The name of an environment variable for a stdio server.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnvName(String);

/// The name of an HTTP header. Header names are not case-sensitive
/// (RFC 9110 §5.1), so the name is kept in lower case.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeaderName(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretNameError {
    #[error("{0:?} is not an environment variable name")]
    Env(String),
    #[error("{0:?} is not an HTTP header name")]
    Header(String),
}

impl EnvName {
    /// # Errors
    ///
    /// Returns an error when the text is not an environment variable name.
    pub fn parse(text: &str) -> Result<Self, SecretNameError> {
        if is_variable_name(text) {
            Ok(Self(text.to_string()))
        } else {
            Err(SecretNameError::Env(text.to_string()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl HeaderName {
    /// # Errors
    ///
    /// Returns an error when the text is not a token (RFC 9110 §5.6.2).
    pub fn parse(text: &str) -> Result<Self, SecretNameError> {
        let is_token_char = |c: char| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c);
        if !text.is_empty() && text.chars().all(is_token_char) {
            Ok(Self(text.to_ascii_lowercase()))
        } else {
            Err(SecretNameError::Header(text.to_string()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The kinds of secrets.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretKind {
    /// A static value of an environment variable for a stdio server.
    Env(EnvName),
    /// A static header value, for example a bearer token or an API key.
    Header(HeaderName),
    /// The OAuth access token and refresh token.
    OAuthTokens,
    /// The OAuth client credentials, with the issuer that registered them.
    OAuthClient,
}

/// The key of one secret of one server.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CredentialKey {
    pub server: ServerName,
    pub kind: SecretKind,
}

impl CredentialKey {
    #[must_use]
    pub fn new(server: ServerName, kind: SecretKind) -> Self {
        Self { server, kind }
    }

    /// The account of the item in the credential store. A server name has no
    /// `/`, so each key gives a different account.
    #[must_use]
    pub fn account(&self) -> String {
        let server = self.server.as_str();
        match &self.kind {
            SecretKind::Env(name) => format!("{server}/env/{}", name.as_str()),
            SecretKind::Header(name) => format!("{server}/header/{}", name.as_str()),
            SecretKind::OAuthTokens => format!("{server}/oauth/tokens"),
            SecretKind::OAuthClient => format!("{server}/oauth/client"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(kind: SecretKind) -> CredentialKey {
        CredentialKey::new(ServerName::parse("gh").unwrap(), kind)
    }

    #[test]
    fn debug_output_hides_secret_value() {
        let secret = Secret::new("s3cr3t".into());
        assert_eq!(format!("{secret:?}"), "Secret(<hidden>)");
        assert_eq!(secret.expose(), "s3cr3t");
    }

    #[test]
    fn gives_a_different_account_for_each_kind() {
        let accounts = [
            key(SecretKind::Env(EnvName::parse("API_KEY").unwrap())).account(),
            key(SecretKind::Header(
                HeaderName::parse("Authorization").unwrap(),
            ))
            .account(),
            key(SecretKind::OAuthTokens).account(),
            key(SecretKind::OAuthClient).account(),
        ];
        assert_eq!(
            accounts,
            [
                "gh/env/API_KEY",
                "gh/header/authorization",
                "gh/oauth/tokens",
                "gh/oauth/client"
            ]
        );
    }

    #[test]
    fn header_names_that_differ_only_in_case_give_one_key() {
        assert_eq!(
            HeaderName::parse("X-Api-Key").unwrap(),
            HeaderName::parse("x-api-key").unwrap()
        );
    }

    #[test]
    fn rejects_bad_env_and_header_names() {
        assert!(EnvName::parse("1A").is_err());
        assert!(EnvName::parse("A-B").is_err());
        assert!(HeaderName::parse("").is_err());
        assert!(HeaderName::parse("X Key").is_err());
        assert!(HeaderName::parse("X:Key").is_err());
    }
}
