//! The protocol era of a server, and the cache of eras.
//!
//! The spec lets a client keep the era across restarts of the same server
//! configuration (MCP 2026-07-28, basic/versioning, "Backward compatibility
//! with initialization-based versions"). The cache keys each era by the
//! server name and its transport target, so a changed target probes again.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

use crate::{
    config::{Server, Transport},
    server::ServerName,
};

/// The first version with `server/discover` and no `initialize` handshake.
const FIRST_MODERN: &str = "2026-07-28";
/// The newest version with the `initialize` handshake.
const LATEST_LEGACY: &str = "2025-11-25";

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
    pub fn first_modern() -> Self {
        Self(FIRST_MODERN.to_string())
    }

    #[must_use]
    pub fn latest_legacy() -> Self {
        Self(LATEST_LEGACY.to_string())
    }

    /// True for a version that uses `server/discover` and no `initialize`.
    /// Dates in this form sort in time order as text.
    #[must_use]
    pub fn is_modern(&self) -> bool {
        self.0.as_str() >= FIRST_MODERN
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

/// The era that a connection found. "Unknown" is the absence of an era.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "era", rename_all = "snake_case")]
pub enum Era {
    Modern {
        version: ProtocolVersion,
    },
    Legacy {
        version: ProtocolVersion,
        cause: LegacyCause,
    },
}

/// Why the client used the `initialize` handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyCause {
    /// The server answered `server/discover` with a JSON-RPC error. This is
    /// final until the target changes.
    ServerError,
    /// The server did not answer `server/discover` in time. A slow modern
    /// server also accepts `initialize`, so this result is not final.
    ProbeTimeout,
}

/// How to start a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Startup {
    /// Send `server/discover` in our own timeout. Fall back to `initialize`
    /// on a new transport.
    Probe,
    /// Send `server/discover` with this version first.
    Discover(ProtocolVersion),
    /// Send `initialize` with this version. No probe.
    Initialize(ProtocolVersion),
}

/// Chooses how to start from the cached era.
#[must_use]
pub fn startup(cached: Option<&Era>) -> Startup {
    match cached {
        None
        | Some(Era::Legacy {
            cause: LegacyCause::ProbeTimeout,
            ..
        }) => Startup::Probe,
        Some(Era::Modern { version }) => Startup::Discover(version.clone()),
        Some(Era::Legacy {
            version,
            cause: LegacyCause::ServerError,
        }) => Startup::Initialize(version.clone()),
    }
}

/// The parts of a server configuration that identify the server process or
/// origin. Environment values and headers are not part of it: they can hold
/// secrets, and they do not change the server program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case")]
pub enum Target {
    Stdio {
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
    },
    Remote {
        url: String,
    },
}

impl Target {
    #[must_use]
    pub fn of(server: &Server) -> Self {
        match &server.transport {
            Transport::Stdio(stdio) => Self::Stdio {
                command: stdio.command.clone(),
                args: stdio.args.clone(),
                cwd: stdio.cwd.clone(),
            },
            Transport::Http(remote) | Transport::Sse(remote) => Self::Remote {
                url: remote.url.as_str().to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CachedEra {
    target: Target,
    era: Era,
}

/// The era of each server. The caller loads and saves it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EraCache {
    servers: BTreeMap<ServerName, CachedEra>,
}

impl EraCache {
    /// The cached era, if the server still has the same target.
    #[must_use]
    pub fn lookup(&self, name: &ServerName, target: &Target) -> Option<&Era> {
        self.servers
            .get(name)
            .filter(|cached| &cached.target == target)
            .map(|cached| &cached.era)
    }

    #[must_use]
    pub fn record(mut self, name: ServerName, target: Target, era: Era) -> Self {
        self.servers.insert(name, CachedEra { target, era });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StdioServer;

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
    fn versions_from_2026_07_28_are_modern() {
        assert!(version("2026-07-28").is_modern());
        assert!(version("2027-01-01").is_modern());
        assert!(!version("2025-11-25").is_modern());
        assert!(!version("2025-03-26").is_modern());
        assert!(ProtocolVersion::first_modern().is_modern());
        assert!(!ProtocolVersion::latest_legacy().is_modern());
    }

    #[test]
    fn probes_when_era_is_unknown() {
        assert_eq!(startup(None), Startup::Probe);
    }

    #[test]
    fn discovers_with_cached_modern_version() {
        let era = Era::Modern {
            version: version("2026-07-28"),
        };
        assert_eq!(
            startup(Some(&era)),
            Startup::Discover(version("2026-07-28"))
        );
    }

    #[test]
    fn initializes_after_legacy_server_error() {
        let era = Era::Legacy {
            version: version("2025-06-18"),
            cause: LegacyCause::ServerError,
        };
        assert_eq!(
            startup(Some(&era)),
            Startup::Initialize(version("2025-06-18"))
        );
    }

    #[test]
    fn probes_again_after_legacy_result_from_a_timeout() {
        let era = Era::Legacy {
            version: version("2025-11-25"),
            cause: LegacyCause::ProbeTimeout,
        };
        assert_eq!(startup(Some(&era)), Startup::Probe);
    }

    #[test]
    fn cache_returns_era_only_for_the_same_target() {
        let era = Era::Modern {
            version: version("2026-07-28"),
        };
        let cache = EraCache::default().record(name("s"), remote("https://a/mcp"), era.clone());
        assert_eq!(
            cache.lookup(&name("s"), &remote("https://a/mcp")),
            Some(&era)
        );
        assert_eq!(cache.lookup(&name("s"), &remote("https://b/mcp")), None);
        assert_eq!(cache.lookup(&name("t"), &remote("https://a/mcp")), None);
    }

    #[test]
    fn target_ignores_env_and_headers() {
        let server = |env_value: &str| Server {
            name: name("s"),
            transport: Transport::Stdio(StdioServer {
                command: "srv".into(),
                args: vec!["-v".into()],
                env: BTreeMap::from([("TOKEN".into(), env_value.into())]),
                cwd: None,
            }),
        };
        assert_eq!(Target::of(&server("a")), Target::of(&server("b")));
    }

    #[test]
    fn cache_round_trips_through_json() {
        let cache = EraCache::default()
            .record(
                name("s"),
                Target::Stdio {
                    command: "srv".into(),
                    args: vec![],
                    cwd: Some(".".into()),
                },
                Era::Legacy {
                    version: version("2025-11-25"),
                    cause: LegacyCause::ServerError,
                },
            )
            .record(
                name("r"),
                remote("https://a/mcp"),
                Era::Modern {
                    version: version("2026-07-28"),
                },
            );
        let text = serde_json::to_string(&cache).unwrap();
        assert_eq!(serde_json::from_str::<EraCache>(&text).unwrap(), cache);
    }

    #[test]
    fn cache_with_bad_version_does_not_load() {
        let text = r#"{"servers": {"s": {"target": {"transport": "remote", "url": "u"},
            "era": {"era": "modern", "version": "latest"}}}}"#;
        assert!(serde_json::from_str::<EraCache>(text).is_err());
    }
}
