//! The credential model: the secrets, and the key of each item in a store.
//!
//! A secret is a named value for one destination. The configuration names
//! it with `${NAME}`, and the destination comes from the entry: a
//! stored value goes only to the destination in its key. OAuth tokens and
//! client credentials are separate items for each server. No secret goes to
//! the configuration file.

use std::fmt;

use crate::{
    expand::is_variable_name,
    server::{Origin, ServerName},
};

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
#[error("The name {} is not a secret name", crate::text::code(.0))]
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

/// The party that can get a stored value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Destination {
    /// A `headers` value goes to the origin of its entry, and a client
    /// secret to the origin of its issuer.
    Origin(Origin),
    /// An `env` value of a stdio server. The value goes to a local process,
    /// so `ncp` has no destination to compare.
    Stdio,
}

impl Destination {
    #[must_use]
    pub fn origin(&self) -> Option<&Origin> {
        match self {
            Self::Origin(origin) => Some(origin),
            Self::Stdio => None,
        }
    }
}

impl fmt::Display for Destination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Origin(origin) => f.write_str(origin.as_str()),
            Self::Stdio => f.write_str("stdio servers"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CredentialKey {
    /// A named value for `${NAME}`. The key has no form with only a name,
    /// so no code can read a value with no destination.
    Secret {
        destination: Destination,
        name: SecretName,
    },
    /// The OAuth access token and refresh token of a server.
    OAuthTokens(ServerName),
    /// The OAuth client credentials of a server, with the issuer that
    /// registered them.
    OAuthClient(ServerName),
}

impl CredentialKey {
    /// The account of the item in the store: `secret/NAME` for stdio,
    /// `secret/ORIGIN/NAME` for an origin. Only this function and
    /// [`Self::from_account`] know the text form.
    #[must_use]
    pub fn account(&self) -> String {
        match self {
            Self::Secret {
                destination: Destination::Stdio,
                name,
            } => format!("{SECRET_PREFIX}{name}"),
            Self::Secret {
                destination: Destination::Origin(origin),
                name,
            } => format!("{SECRET_PREFIX}{origin}/{name}"),
            Self::OAuthTokens(server) => format!("{OAUTH_PREFIX}{}/tokens", server.as_str()),
            Self::OAuthClient(server) => format!("{OAUTH_PREFIX}{}/client", server.as_str()),
        }
    }

    /// Gives `None` for an account that `ncp` does not make, for example
    /// from an older version, or a part of a long value.
    ///
    /// A name has no `/` and no `:`, and an origin in normal form has no
    /// path. Thus the first `/` after `://` ends the origin, and a stdio
    /// item `secret/https` is not a prefix of an origin item.
    #[must_use]
    pub fn from_account(account: &str) -> Option<Self> {
        if let Some(rest) = account.strip_prefix(SECRET_PREFIX) {
            return secret_key(rest);
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

fn secret_key(rest: &str) -> Option<CredentialKey> {
    let Some(scheme_end) = rest.find("://") else {
        return SecretName::parse(rest)
            .ok()
            .map(|name| CredentialKey::Secret {
                destination: Destination::Stdio,
                name,
            });
    };
    let host_start = scheme_end + "://".len();
    let split = host_start + rest[host_start..].find('/')?;
    let origin = Origin::parse(&rest[..split])?;
    let name = SecretName::parse(&rest[split + 1..]).ok()?;
    Some(CredentialKey::Secret {
        destination: Destination::Origin(origin),
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_texts_follow_the_message_rules() {
        crate::style::assert_style(&[SecretName::parse("1a").unwrap_err().to_string()]);
    }

    fn server() -> ServerName {
        ServerName::parse("gh").unwrap()
    }

    #[test]
    fn debug_output_hides_secret_value() {
        let secret = Secret::new("s3cr3t".into());
        assert_eq!(format!("{secret:?}"), "Secret(<hidden>)");
        assert_eq!(secret.expose(), "s3cr3t");
    }

    fn secret(destination: Destination, name: &str) -> CredentialKey {
        CredentialKey::Secret {
            destination,
            name: SecretName::parse(name).unwrap(),
        }
    }

    fn origin(text: &str) -> Destination {
        Destination::Origin(Origin::parse(text).unwrap())
    }

    #[test]
    fn gives_a_different_account_for_each_key() {
        let keys = [
            secret(Destination::Stdio, "GITHUB_TOKEN"),
            secret(origin("https://mcp.linear.app"), "LINEAR_KEY"),
            secret(origin("http://127.0.0.1:8080"), "K"),
            secret(origin("https://[::1]"), "K"),
            secret(origin("https://xn--bcher-kva.example"), "K"),
            secret(origin("https://h."), "K"),
            secret(Destination::Stdio, "https"),
            CredentialKey::OAuthTokens(server()),
            CredentialKey::OAuthClient(server()),
        ];
        let accounts: Vec<String> = keys.iter().map(CredentialKey::account).collect();
        assert_eq!(
            accounts,
            [
                "secret/GITHUB_TOKEN",
                "secret/https://mcp.linear.app/LINEAR_KEY",
                "secret/http://127.0.0.1:8080/K",
                "secret/https://[::1]/K",
                "secret/https://xn--bcher-kva.example/K",
                "secret/https://h./K",
                "secret/https",
                "oauth/gh/tokens",
                "oauth/gh/client"
            ]
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
            "secret/HTTPS://MCP.LINEAR.APP/K",
            "secret/https://h:443/K",
            "secret/https://h/mcp/K",
            "secret/https://h/",
            "secret/https://h",
            "secret/file:///x/K",
            "secret/https://h/K/gen/0",
            "secret/A/gen/0",
            "oauth/gh/other",
            "oauth/a.b/tokens",
            "",
        ] {
            assert_eq!(CredentialKey::from_account(account), None, "{account}");
        }
    }

    #[test]
    fn no_account_is_another_account_and_a_slash() {
        // The clean-up of the parts of a long value removes the accounts under
        // `ACCOUNT/`. It must never find another item there.
        let accounts = [
            secret(Destination::Stdio, "https"),
            secret(Destination::Stdio, "A"),
            secret(origin("https://h"), "A"),
        ]
        .map(|key| key.account());
        for account in &accounts {
            for other in &accounts {
                assert!(
                    !other.starts_with(&format!("{account}/")),
                    "{account} {other}"
                );
            }
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
