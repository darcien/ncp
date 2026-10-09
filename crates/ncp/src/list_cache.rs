//! The cache of tool lists, one file for each server.
//!
//! From `2026-07-28` a list result can carry `ttlMs`: the time that a
//! client may treat it as fresh. Within that time `ncp` can show and use
//! the tools with no server start. A list with no `ttlMs` is not kept. The
//! cache is a cache: `ncp` works the same when it is removed, and a file
//! that does not load is ignored.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use rmcp::model::Tool;
use serde::{Deserialize, Serialize};

use crate::{config::Target, private_file, server::ServerName};

/// The tools of one server, and when they stop being fresh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedTools {
    /// The target as the file writes it. A changed target makes the list
    /// invalid.
    pub target: Target,
    /// A digest of the entry as the file writes it: a changed `env`,
    /// `headers` or `oauth.clientId` makes the list invalid.
    pub entry: u64,
    /// Milliseconds since the Unix epoch.
    pub fetched_at_ms: u64,
    pub ttl_ms: u64,
    /// The server has `instructions` in its handshake, so that a list from
    /// the cache gives the same hint as a live list.
    #[serde(default)]
    pub has_instructions: bool,
    pub tools: Vec<Tool>,
}

impl CachedTools {
    /// The tools when they are fresh for this target at this time.
    #[must_use]
    pub fn fresh_for(&self, target: &Target, entry: u64, now_ms: u64) -> Option<&Self> {
        let fresh = &self.target == target
            && self.entry == entry
            && now_ms >= self.fetched_at_ms
            && now_ms - self.fetched_at_ms < self.ttl_ms;
        fresh.then_some(self)
    }

    /// The age of the list.
    #[must_use]
    pub fn age(&self, now_ms: u64) -> Duration {
        Duration::from_millis(now_ms.saturating_sub(self.fetched_at_ms))
    }
}

/// The longest time that `ncp` keeps a list, whatever the server says. A
/// wrong `ttlMs` must not keep an old list for days.
pub const MAX_TTL_MS: u64 = 24 * 60 * 60 * 1000;

/// What a list can depend on beyond the entry as the file writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// A stdio server gets the environment of `ncp`, which can hold a
    /// credential that the entry does not show.
    Stdio,
    /// An http server, with or without a credential: an OAuth token or a
    /// `${NAME}` value.
    Http { credentials: bool },
}

/// The time to keep a list, or `None` when `ncp` must not keep it.
///
/// A private list must not be shared across authorization contexts (MCP
/// 2026-07-28, server/utilities/caching). The cache has no copy of a
/// credential, so it cannot see a changed one: a private list is kept only
/// from an http server with no credential.
#[must_use]
pub fn keep_for(ttl_ms: Option<u64>, private: bool, access: Access) -> Option<u64> {
    let ttl = ttl_ms.filter(|ttl| *ttl > 0)?;
    let shared = !private || access == Access::Http { credentials: false };
    shared.then_some(ttl.min(MAX_TTL_MS))
}

/// The digest of the entry: the target and the `env`, `headers`,
/// `oauth.clientId` and `oauth.issuer` as the file writes them. It holds no
/// expanded value.
#[must_use]
pub fn entry_digest(server: &crate::config::Server) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    format!("{:?}", server.target).hash(&mut hasher);
    match &server.transport {
        crate::config::Transport::Stdio(stdio) => format!("{:?}", stdio.env).hash(&mut hasher),
        crate::config::Transport::Http(remote) | crate::config::Transport::Sse(remote) => {
            format!("{:?}", remote.headers).hash(&mut hasher);
            // The secret is left out: a literal value is a secret.
            remote
                .oauth_client
                .as_ref()
                .map(|client| (&client.client_id, client.issuer()))
                .hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Removes the lists of all servers, for example after a change of a
/// secret: a list can depend on it.
pub fn remove_all(cache_dir: &Path) {
    let _ = std::fs::remove_dir_all(cache_dir.join("lists"));
}

/// The file of the tools of a server.
#[must_use]
pub fn path(cache_dir: &Path, server: &ServerName) -> PathBuf {
    cache_dir
        .join("lists")
        .join(format!("{}.json", server.as_str()))
}

/// Reads the file. A missing file or a file that does not load gives
/// `None`: a cache never stops a command.
#[must_use]
pub fn load(path: &Path) -> Option<CachedTools> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes the file with mode 0600.
///
/// # Errors
///
/// Returns an error when the file cannot be written.
pub fn save(path: &Path, cached: &CachedTools) -> std::io::Result<()> {
    let text = serde_json::to_string(cached).map_err(std::io::Error::other)?;
    private_file::write(path, text.as_bytes())
}

/// Removes the file, for example when a server gives no `ttlMs` now. A
/// file that stays gives an old list only until its time passes, so a
/// failure is not an error.
pub fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// Milliseconds since the Unix epoch.
#[must_use]
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| {
            u64::try_from(time.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn target(url: &str) -> Target {
        Target::Remote { url: url.into() }
    }

    fn cached() -> CachedTools {
        CachedTools {
            target: target("https://a.test/mcp"),
            entry: 7,
            fetched_at_ms: 1_000,
            ttl_ms: 500,
            has_instructions: false,
            tools: vec![Tool::new("echo", "Echo", Arc::new(serde_json::Map::new()))],
        }
    }

    #[test]
    fn list_is_fresh_only_within_its_time_and_for_its_target() {
        let cached = cached();
        let a = target("https://a.test/mcp");
        assert_eq!(
            cached
                .fresh_for(&a, 7, 1_499)
                .map(|fresh| fresh.tools.len()),
            Some(1)
        );
        assert!(cached.fresh_for(&a, 7, 1_500).is_none());
        assert!(
            cached
                .fresh_for(&target("https://b.test/mcp"), 7, 1_100)
                .is_none()
        );
        assert!(cached.fresh_for(&a, 8, 1_100).is_none());
        assert!(cached.fresh_for(&a, 7, 999).is_none());
    }

    #[test]
    fn private_list_is_kept_only_from_an_http_server_with_no_credential() {
        let open = Access::Http { credentials: false };
        let with_credential = Access::Http { credentials: true };
        assert_eq!(keep_for(Some(5), false, with_credential), Some(5));
        assert_eq!(keep_for(Some(5), false, Access::Stdio), Some(5));
        assert_eq!(keep_for(Some(5), true, open), Some(5));
        assert_eq!(keep_for(Some(5), true, with_credential), None);
        assert_eq!(keep_for(Some(5), true, Access::Stdio), None);
        assert_eq!(keep_for(None, false, open), None);
        assert_eq!(keep_for(Some(0), false, open), None);
        assert_eq!(keep_for(Some(u64::MAX), false, open), Some(MAX_TTL_MS));
    }

    #[test]
    fn saves_and_loads_the_list_and_ignores_a_broken_file() {
        let dir = std::env::temp_dir().join(format!("ncp-lists-{}", std::process::id()));
        let file = path(&dir, &ServerName::parse("web").unwrap());
        save(&file, &cached()).unwrap();
        assert_eq!(load(&file), Some(cached()));
        std::fs::write(&file, "{").unwrap();
        assert_eq!(load(&file), None);
        remove(&file);
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
