//! The protocol version of a server, and its cache.
//!
//! The spec lets a client keep the protocol version across restarts of the
//! same server configuration (MCP 2026-07-28, basic/versioning, "Backward
//! compatibility with initialization-based versions"). The cache keys each
//! version by the server name and its transport target, so a changed target
//! probes again.
//!
//! The cache keeps an `initialize` result also when it came from a probe
//! timeout. Thus a server that does not answer `server/discover` costs the
//! timeout once, and not at each run. When the handshake with the cached
//! version fails, the client probes again, because the server can have
//! changed.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

use crate::{config::Target, server::ServerName};

/// The first version with `server/discover` and no `initialize` handshake.
const FIRST_WITH_DISCOVER: &str = "2026-07-28";
/// The newest version with the `initialize` handshake.
const LATEST_WITH_INITIALIZE: &str = "2025-11-25";

/// An MCP protocol version, in the `YYYY-MM-DD` form of the spec.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProtocolVersion(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a protocol version of the form YYYY-MM-DD")]
pub struct VersionError(String);

impl ProtocolVersion {
    /// # Errors
    ///
    /// Returns an error when the text is not of the form `YYYY-MM-DD`.
    pub fn parse(text: &str) -> Result<Self, VersionError> {
        let digits_at = |range: std::ops::Range<usize>| {
            text.get(range)
                .is_some_and(|part| part.bytes().all(|b| b.is_ascii_digit()))
        };
        let shape_ok = text.len() == 10
            && text.as_bytes()[4] == b'-'
            && text.as_bytes()[7] == b'-'
            && digits_at(0..4)
            && digits_at(5..7)
            && digits_at(8..10);
        if shape_ok {
            Ok(Self(text.to_string()))
        } else {
            Err(VersionError(text.to_string()))
        }
    }

    #[must_use]
    pub fn first_with_discover() -> Self {
        Self(FIRST_WITH_DISCOVER.to_string())
    }

    #[must_use]
    pub fn latest_with_initialize() -> Self {
        Self(LATEST_WITH_INITIALIZE.to_string())
    }

    /// True for a version that uses `server/discover` and no `initialize`.
    /// Dates in this form sort in time order as text.
    #[must_use]
    pub fn uses_discover(&self) -> bool {
        self.0.as_str() >= FIRST_WITH_DISCOVER
    }

    /// The version and the method of its handshake, for messages.
    #[must_use]
    pub fn describe(&self) -> String {
        let method = if self.uses_discover() {
            "server/discover"
        } else {
            "initialize"
        };
        format!("{} ({method})", self.0)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ProtocolVersion {
    type Error = VersionError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::parse(&text)
    }
}

impl From<ProtocolVersion> for String {
    fn from(version: ProtocolVersion) -> Self {
        version.0
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How to start a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Startup {
    /// Send `server/discover` in our own timeout. Fall back to `initialize`
    /// on a new transport.
    Probe,
    /// Send `server/discover` with this version first.
    Discover(ProtocolVersion),
    /// Send `initialize` with this version. No probe. When this handshake
    /// fails, probe.
    Initialize(ProtocolVersion),
}

/// Chooses how to start from the cached version.
#[must_use]
pub fn startup(cached: Option<&ProtocolVersion>) -> Startup {
    match cached {
        None => Startup::Probe,
        Some(version) if version.uses_discover() => Startup::Discover(version.clone()),
        Some(version) => Startup::Initialize(version.clone()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CachedVersion {
    target: Target,
    version: ProtocolVersion,
}

/// The protocol version of each server. The caller loads and saves it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionCache {
    servers: BTreeMap<ServerName, CachedVersion>,
}

impl VersionCache {
    /// The cached version, if the server still has the same target.
    #[must_use]
    pub fn lookup(&self, name: &ServerName, target: &Target) -> Option<&ProtocolVersion> {
        self.servers
            .get(name)
            .filter(|cached| &cached.target == target)
            .map(|cached| &cached.version)
    }

    #[must_use]
    pub fn record(mut self, name: ServerName, target: Target, version: ProtocolVersion) -> Self {
        self.servers.insert(name, CachedVersion { target, version });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> ProtocolVersion {
        ProtocolVersion::parse(text).unwrap()
    }

    fn name(text: &str) -> ServerName {
        ServerName::parse(text).unwrap()
    }

    fn remote(url: &str) -> Target {
        Target::Remote { url: url.into() }
    }

    #[test]
    fn accepts_only_date_form_versions() {
        assert!(ProtocolVersion::parse("2025-11-25").is_ok());
        for bad in ["2025-11-5", "2025/11/25", "x025-11-25", "", "2025-11-25x"] {
            assert!(ProtocolVersion::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn versions_from_2026_07_28_use_discover() {
        assert!(version("2026-07-28").uses_discover());
        assert!(version("2027-01-01").uses_discover());
        assert!(!version("2025-11-25").uses_discover());
        assert!(!version("2025-03-26").uses_discover());
        assert!(ProtocolVersion::first_with_discover().uses_discover());
        assert!(!ProtocolVersion::latest_with_initialize().uses_discover());
    }

    #[test]
    fn describes_the_version_with_its_handshake() {
        assert_eq!(
            version("2026-07-28").describe(),
            "2026-07-28 (server/discover)"
        );
        assert_eq!(version("2025-11-25").describe(), "2025-11-25 (initialize)");
    }

    #[test]
    fn probes_when_no_version_is_cached() {
        assert_eq!(startup(None), Startup::Probe);
    }

    #[test]
    fn discovers_with_a_cached_discover_version() {
        assert_eq!(
            startup(Some(&version("2026-07-28"))),
            Startup::Discover(version("2026-07-28"))
        );
    }

    #[test]
    fn initializes_with_a_cached_initialize_version() {
        assert_eq!(
            startup(Some(&version("2025-06-18"))),
            Startup::Initialize(version("2025-06-18"))
        );
    }

    #[test]
    fn cache_returns_the_version_only_for_the_same_target() {
        let cache = VersionCache::default().record(
            name("s"),
            remote("https://a/mcp"),
            version("2026-07-28"),
        );
        assert_eq!(
            cache.lookup(&name("s"), &remote("https://a/mcp")),
            Some(&version("2026-07-28"))
        );
        assert_eq!(cache.lookup(&name("s"), &remote("https://b/mcp")), None);
        assert_eq!(cache.lookup(&name("t"), &remote("https://a/mcp")), None);
    }

    #[test]
    fn cache_round_trips_through_json() {
        let cache = VersionCache::default()
            .record(
                name("s"),
                Target::Stdio {
                    command: "srv".into(),
                    args: vec![],
                    cwd: Some(".".into()),
                },
                version("2025-11-25"),
            )
            .record(name("r"), remote("https://a/mcp"), version("2026-07-28"));
        let text = serde_json::to_string(&cache).unwrap();
        assert_eq!(serde_json::from_str::<VersionCache>(&text).unwrap(), cache);
    }

    #[test]
    fn cache_with_bad_version_does_not_load() {
        let text = r#"{"servers": {"s": {"target": {"transport": "remote", "url": "u"},
            "version": "latest"}}}"#;
        assert!(serde_json::from_str::<VersionCache>(text).is_err());
    }
}
