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
    /// Milliseconds since the Unix epoch.
    pub fetched_at_ms: u64,
    pub ttl_ms: u64,
    pub tools: Vec<Tool>,
}

impl CachedTools {
    /// The tools when they are fresh for this target at this time.
    #[must_use]
    pub fn fresh_for(&self, target: &Target, now_ms: u64) -> Option<&[Tool]> {
        let fresh = &self.target == target
            && now_ms >= self.fetched_at_ms
            && now_ms - self.fetched_at_ms < self.ttl_ms;
        fresh.then_some(self.tools.as_slice())
    }

    /// The age of the list.
    #[must_use]
    pub fn age(&self, now_ms: u64) -> Duration {
        Duration::from_millis(now_ms.saturating_sub(self.fetched_at_ms))
    }
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
            fetched_at_ms: 1_000,
            ttl_ms: 500,
            tools: vec![Tool::new("echo", "Echo", Arc::new(serde_json::Map::new()))],
        }
    }

    #[test]
    fn list_is_fresh_only_within_its_time_and_for_its_target() {
        let cached = cached();
        assert_eq!(
            cached
                .fresh_for(&target("https://a.test/mcp"), 1_499)
                .map(<[Tool]>::len),
            Some(1)
        );
        assert!(
            cached
                .fresh_for(&target("https://a.test/mcp"), 1_500)
                .is_none()
        );
        assert!(
            cached
                .fresh_for(&target("https://b.test/mcp"), 1_100)
                .is_none()
        );
        assert!(
            cached
                .fresh_for(&target("https://a.test/mcp"), 999)
                .is_none()
        );
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
