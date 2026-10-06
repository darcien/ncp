//! Value types for a server: its name and its URL.

use std::{fmt, net::IpAddr};

use serde::{Deserialize, Serialize};
use url::{Host, Url};

const MAX_NAME_LEN: usize = 64;

/// The name of a server in the configuration.
///
/// The name is a word in commands and a part of credential keys. A tool name
/// can contain `.` and `/` (SEP-986), so the name allows only ASCII letters,
/// digits, `-` and `_`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ServerName(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("server name is empty")]
    Empty,
    #[error("server name is longer than {MAX_NAME_LEN} characters")]
    TooLong,
    #[error("server name {0:?} has a character that is not a letter, digit, '-' or '_'")]
    BadCharacter(String),
}

impl ServerName {
    /// # Errors
    ///
    /// Returns an error when the text breaks the name rules.
    pub fn parse(text: &str) -> Result<Self, NameError> {
        if text.is_empty() {
            Err(NameError::Empty)
        } else if text.len() > MAX_NAME_LEN {
            Err(NameError::TooLong)
        } else if !text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            Err(NameError::BadCharacter(text.to_string()))
        } else {
            Ok(Self(text.to_string()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ServerName {
    type Error = NameError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

impl From<ServerName> for String {
    fn from(name: ServerName) -> Self {
        name.0
    }
}

impl fmt::Display for ServerName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The URL of a remote server.
///
/// Tokens and headers must not go over the network in plain text, so `http`
/// is permitted only for a loopback host. The configuration file must not
/// hold a secret, so the URL has no user name or password.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerUrl(Url);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UrlError {
    #[error("invalid URL: {0}")]
    Invalid(#[from] url::ParseError),
    #[error("URL scheme {0:?} is not https or http")]
    Scheme(String),
    #[error("URL has no host")]
    NoHost,
    #[error("plain http is permitted only for a loopback host; use https")]
    PlainHttp,
    #[error("URL has a user name or password; keep secrets in the credential store")]
    Credentials,
}

impl ServerUrl {
    /// # Errors
    ///
    /// Returns an error when the text is not a URL, or breaks the URL rules.
    pub fn parse(text: &str) -> Result<Self, UrlError> {
        let url = Url::parse(text)?;
        let host = url.host().ok_or(UrlError::NoHost)?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(UrlError::Credentials);
        }
        match url.scheme() {
            "https" => Ok(Self(url)),
            "http" if is_loopback(&host) => Ok(Self(url)),
            "http" => Err(UrlError::PlainHttp),
            other => Err(UrlError::Scheme(other.to_string())),
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for ServerUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_str())
    }
}

fn is_loopback(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(name) => name.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(ip) => IpAddr::V4(*ip).is_loopback(),
        Host::Ipv6(ip) => IpAddr::V6(*ip).is_loopback(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_letters_digits_dash_and_underscore_in_name() {
        assert_eq!(
            ServerName::parse("git-hub_2").unwrap().as_str(),
            "git-hub_2"
        );
    }

    #[test]
    fn rejects_dot_in_name_because_tool_names_can_contain_dots() {
        assert!(matches!(
            ServerName::parse("a.b"),
            Err(NameError::BadCharacter(_))
        ));
    }

    #[test]
    fn rejects_empty_and_long_names() {
        assert_eq!(ServerName::parse(""), Err(NameError::Empty));
        assert_eq!(ServerName::parse(&"a".repeat(65)), Err(NameError::TooLong));
        assert!(ServerName::parse(&"a".repeat(64)).is_ok());
    }

    #[test]
    fn accepts_https_url() {
        let url = ServerUrl::parse("https://example.com/mcp").unwrap();
        assert_eq!(url.as_str(), "https://example.com/mcp");
    }

    #[test]
    fn accepts_plain_http_to_loopback_hosts() {
        for text in [
            "http://localhost:8080/mcp",
            "http://LOCALHOST/mcp",
            "http://127.0.0.1/mcp",
            "http://127.1.2.3/mcp",
            "http://[::1]:9000/mcp",
        ] {
            assert!(ServerUrl::parse(text).is_ok(), "{text}");
        }
    }

    #[test]
    fn rejects_plain_http_to_other_hosts() {
        for text in [
            "http://example.com/mcp",
            "http://10.0.0.1/mcp",
            "http://localhost.example.com/",
        ] {
            assert_eq!(ServerUrl::parse(text), Err(UrlError::PlainHttp), "{text}");
        }
    }

    #[test]
    fn rejects_credentials_in_url() {
        assert_eq!(
            ServerUrl::parse("https://user:pass@example.com/mcp"),
            Err(UrlError::Credentials)
        );
        assert_eq!(
            ServerUrl::parse("https://token@example.com/mcp"),
            Err(UrlError::Credentials)
        );
    }

    #[test]
    fn rejects_other_schemes_and_text_that_is_not_a_url() {
        assert_eq!(
            ServerUrl::parse("ws://localhost/mcp"),
            Err(UrlError::Scheme("ws".into()))
        );
        assert!(matches!(
            ServerUrl::parse("example.com"),
            Err(UrlError::Invalid(_))
        ));
    }

    #[test]
    fn server_name_round_trips_through_json_and_rejects_bad_names() {
        let name: ServerName = serde_json::from_str("\"abc\"").unwrap();
        assert_eq!(serde_json::to_string(&name).unwrap(), "\"abc\"");
        assert!(serde_json::from_str::<ServerName>("\"a.b\"").is_err());
    }
}
