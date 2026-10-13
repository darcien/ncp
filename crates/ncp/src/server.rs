//! Value types for a server: its name, its URL and the origin of a URL.

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
/// Tokens and headers must not go over the network in plain text, so `ncp`
/// permits `http` only for a loopback host. The configuration file must not
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

    #[must_use]
    pub fn as_url(&self) -> &Url {
        &self.0
    }

    /// The URL with no query and no fragment. An OAuth resource SHOULD NOT
    /// have a query (RFC 8707 §2), and a query can hold a key.
    #[must_use]
    pub fn without_query(&self) -> String {
        let mut url = self.0.clone();
        url.set_query(None);
        url.set_fragment(None);
        url.to_string()
    }
}

impl fmt::Display for ServerUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_str())
    }
}

/// True for a loopback host: `localhost` or a loopback IP address.
#[must_use]
pub fn is_loopback(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(name) => name.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(ip) => IpAddr::V4(*ip).is_loopback(),
        Host::Ipv6(ip) => IpAddr::V6(*ip).is_loopback(),
    }
}

/// The rule for each URL that `ncp` sends a request to or opens: HTTPS,
/// or HTTP to a loopback host. This is an `ncp` rule. Tokens, headers and
/// form values must not go over the network in plain text. A loopback host
/// does not leave the machine, so local servers and tests can use HTTP.
#[must_use]
pub fn is_secure_url(url: &Url) -> bool {
    match (url.scheme(), url.host()) {
        ("https", Some(_)) => true,
        ("http", Some(host)) => is_loopback(&host),
        _ => false,
    }
}

/// The origin of a URL: its scheme, host and port (RFC 6454), in the ASCII
/// serialization of the WHATWG URL Standard. A stored value goes only to
/// the origin in its key. Thus each URL of one origin must give the same
/// text: a lower-case host, `xn--` labels, IPv6 in brackets and no default
/// port.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Origin(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OriginError {
    #[error("the scheme, host and port of {0:?} must be literal text, with no ${{NAME}}")]
    NotLiteral(String),
    #[error("{0:?} is not an https or http URL")]
    NotHttp(String),
}

impl Origin {
    /// The origin of an `https` or `http` URL. Other schemes have no
    /// destination that `ncp` sends a stored value to.
    ///
    /// # Errors
    ///
    /// Returns an error for another scheme.
    pub fn of(url: &Url) -> Result<Self, OriginError> {
        match (url.scheme(), url.origin()) {
            ("https" | "http", origin @ url::Origin::Tuple(..)) => {
                Ok(Self(origin.ascii_serialization()))
            }
            _ => Err(OriginError::NotHttp(url.to_string())),
        }
    }

    /// The origin of a URL as the file writes it, before `${NAME}` expansion. A
    /// value from the environment can change the host. Thus the text up to the
    /// first `/`, `?` or `#` after `://` must have no `${`. The origin must
    /// also be the origin of the expanded URL, so the key and the request can
    /// never disagree.
    ///
    /// # Errors
    ///
    /// Returns an error when the scheme, host or port is not literal text.
    pub fn of_written(written: &str, expanded: &Url) -> Result<Self, OriginError> {
        let not_literal = || OriginError::NotLiteral(written.to_string());
        let start = written.find("://").ok_or_else(not_literal)? + "://".len();
        let end = written[start..]
            .find(['/', '?', '#'])
            .map_or(written.len(), |offset| start + offset);
        let head = &written[..end];
        if head.contains("${") {
            return Err(not_literal());
        }
        let origin = Url::parse(head)
            .map_err(|_| not_literal())
            .and_then(|url| Self::of(&url))?;
        if Self::of(expanded)? == origin {
            Ok(origin)
        } else {
            Err(not_literal())
        }
    }

    /// The origin in the account of a stored item. Only the normal form is
    /// an origin, so that one origin cannot have two items.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Url::parse(text)
            .ok()
            .and_then(|url| Self::of(&url).ok())
            .filter(|origin| origin.0 == text)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secure_url_is_https_or_http_to_a_loopback_host() {
        let secure = |text: &str| is_secure_url(&Url::parse(text).unwrap());
        assert!(secure("https://example.com/x"));
        assert!(secure("http://127.0.0.1:8080/x"));
        assert!(secure("http://[::1]/x"));
        assert!(secure("http://LOCALHOST/x"));
        assert!(!secure("http://example.com/x"));
        assert!(!secure("http://127.0.0.1.example.com/x"));
        assert!(!secure("file:///etc/passwd"));
        assert!(!secure("myapp://open"));
    }

    #[test]
    fn url_without_query_keeps_the_path() {
        let url = ServerUrl::parse("https://a.test/mcp/x?api_key=k#f").unwrap();
        assert_eq!(url.without_query(), "https://a.test/mcp/x");
    }

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

    fn origin(text: &str) -> Result<Origin, OriginError> {
        Origin::of_written(text, &Url::parse(text).unwrap())
    }

    #[test]
    fn origin_has_one_normal_form_for_each_scheme_host_and_port() {
        for (text, expected) in [
            ("https://MCP.Example.COM/mcp", "https://mcp.example.com"),
            ("https://h:443/mcp", "https://h"),
            ("https://h:8443/mcp", "https://h:8443"),
            ("http://localhost:80/", "http://localhost"),
            ("http://127.0.0.1:9000/x", "http://127.0.0.1:9000"),
            ("https://[0:0::1]/mcp", "https://[::1]"),
            (
                "https://bücher.example/mcp",
                "https://xn--bcher-kva.example",
            ),
            ("https://h./mcp", "https://h."),
            ("https://h?q=1", "https://h"),
            ("https://h#f", "https://h"),
        ] {
            assert_eq!(origin(text).unwrap().as_str(), expected, "{text}");
            assert_eq!(Origin::parse(expected).unwrap().as_str(), expected);
        }
    }

    #[test]
    fn origin_needs_literal_scheme_host_and_port() {
        let expanded = Url::parse("https://h:8443/mcp").unwrap();
        for written in [
            "https://h${SFX}/mcp",
            "https://h:${PORT}/mcp",
            "${SCHEME}://h/mcp",
            "${URL}",
            "https://u${X}@h/mcp",
        ] {
            assert!(
                matches!(
                    Origin::of_written(written, &expanded),
                    Err(OriginError::NotLiteral(_))
                ),
                "{written}"
            );
        }
        let path = Origin::of_written("https://h:8443/${PATH}", &expanded).unwrap();
        assert_eq!(path.as_str(), "https://h:8443");
    }

    #[test]
    fn literal_origin_is_the_origin_of_the_expanded_url() {
        for (written, value) in [
            ("https://h/${P}", "@evil.example/x"),
            ("https://h/${P}", "/../../x"),
            ("https://h?${P}", "a#b"),
            ("https://h#${P}", "x"),
            ("https://h/a${P}", "\\\\evil.example"),
        ] {
            let env = crate::expand::Env::from([("P".to_string(), value.to_string())]);
            let expanded = Url::parse(&crate::expand::expand(written, &env).unwrap()).unwrap();
            let literal = Origin::of_written(written, &expanded).unwrap();
            assert_eq!(literal, Origin::of(&expanded).unwrap(), "{written} {value}");
        }
        // The origin refuses a text that the URL parser reads with another
        // host.
        let other = Url::parse("https://other/").unwrap();
        assert!(Origin::of_written("https://h/", &other).is_err());
    }

    #[test]
    fn origin_of_an_account_must_be_in_normal_form() {
        for text in [
            "HTTPS://H",
            "https://H",
            "https://h:443",
            "https://h/",
            "https://h/mcp",
            "file:///x",
            "https",
            "",
        ] {
            assert_eq!(Origin::parse(text), None, "{text}");
        }
    }
}
