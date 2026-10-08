//! The credential model: the secrets, and the key of each item in a store.
//!
//! A secret is a named value, the same as an environment variable. The
//! configuration names it with `${NAME}`. OAuth tokens and client
//! credentials are separate items for each server. No secret goes to the
//! configuration file.

use std::fmt;

use crate::{expand::is_variable_name, server::ServerName};

/// The service name of all `ncp` items in the credential store.
pub const SERVICE: &str = "ncp";

const SECRET_PREFIX: &str = "secret/";
const OAUTH_PREFIX: &str = "oauth/";

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

/// The name of a secret: a POSIX environment variable name, because
/// `${NAME}` in the configuration names it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SecretName(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a secret name; use letters, digits and '_', and do not start with a digit")]
pub struct SecretNameError(String);

impl SecretName {
    /// # Errors
    ///
    /// Returns an error when the text is not an environment variable name.
    pub fn parse(text: &str) -> Result<Self, SecretNameError> {
        if is_variable_name(text) {
            Ok(Self(text.to_string()))
        } else {
            Err(SecretNameError(text.to_string()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SecretName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The key of one item in a store.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CredentialKey {
    /// A named value for `${NAME}`.
    Secret(SecretName),
    /// The OAuth access token and refresh token of a server.
    OAuthTokens(ServerName),
    /// The OAuth client credentials of a server, with the issuer that
    /// registered them.
    OAuthClient(ServerName),
}

impl CredentialKey {
    /// The account of the item in the store. A secret name and a server
    /// name have no `/`, so each key gives a different account.
    #[must_use]
    pub fn account(&self) -> String {
        match self {
            Self::Secret(name) => format!("{SECRET_PREFIX}{}", name.as_str()),
            Self::OAuthTokens(server) => format!("{OAUTH_PREFIX}{}/tokens", server.as_str()),
            Self::OAuthClient(server) => format!("{OAUTH_PREFIX}{}/client", server.as_str()),
        }
    }

    /// The key of an account, or `None` for an account that `ncp` does not
    /// make, for example from an older version.
    #[must_use]
    pub fn from_account(account: &str) -> Option<Self> {
        if let Some(name) = account.strip_prefix(SECRET_PREFIX) {
            return SecretName::parse(name).ok().map(Self::Secret);
        }
        let (server, item) = account.strip_prefix(OAUTH_PREFIX)?.split_once('/')?;
        let server = ServerName::parse(server).ok()?;
        match item {
            "tokens" => Some(Self::OAuthTokens(server)),
            "client" => Some(Self::OAuthClient(server)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> ServerName {
        ServerName::parse("gh").unwrap()
    }

    #[test]
    fn debug_output_hides_secret_value() {
        let secret = Secret::new("s3cr3t".into());
        assert_eq!(format!("{secret:?}"), "Secret(<hidden>)");
        assert_eq!(secret.expose(), "s3cr3t");
    }

    #[test]
    fn gives_a_different_account_for_each_key() {
        let keys = [
            CredentialKey::Secret(SecretName::parse("GITHUB_TOKEN").unwrap()),
            CredentialKey::OAuthTokens(server()),
            CredentialKey::OAuthClient(server()),
        ];
        let accounts: Vec<String> = keys.iter().map(CredentialKey::account).collect();
        assert_eq!(
            accounts,
            ["secret/GITHUB_TOKEN", "oauth/gh/tokens", "oauth/gh/client"]
        );
        for (key, account) in keys.iter().zip(&accounts) {
            assert_eq!(CredentialKey::from_account(account).as_ref(), Some(key));
        }
    }

    #[test]
    fn ignores_accounts_that_ncp_does_not_make() {
        for account in [
            "gh/env/API_KEY",
            "secret/1A",
            "oauth/gh/other",
            "oauth/a.b/tokens",
            "",
        ] {
            assert_eq!(CredentialKey::from_account(account), None, "{account}");
        }
    }

    #[test]
    fn secret_names_are_variable_names() {
        assert!(SecretName::parse("API_KEY_2").is_ok());
        assert!(SecretName::parse("_X").is_ok());
        assert!(SecretName::parse("1A").is_err());
        assert!(SecretName::parse("A-B").is_err());
        assert!(SecretName::parse("").is_err());
    }
}
