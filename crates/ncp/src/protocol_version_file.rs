//! The protocol version cache file.
//!
//! It is a cache: `ncp` works the same with no file. `ncp` ignores a file that
//! does not load, and the next save writes it again.

use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use crate::{private_file, protocol_version::VersionCache};

#[derive(Debug, thiserror::Error)]
#[error(
    "ncp cannot use the protocol version cache {}. ncp works the same with no cache.",
    crate::text::code(&.path.display().to_string())
)]
pub struct CacheError {
    path: PathBuf,
    source: std::io::Error,
}

/// # Errors
///
/// Returns an error when the file exists but `ncp` cannot read it.
pub fn load(path: &Path) -> Result<VersionCache, CacheError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_default()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(VersionCache::default()),
        Err(source) => Err(CacheError {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// # Errors
///
/// Returns an error when `ncp` cannot write the file.
pub fn save(path: &Path, cache: &VersionCache) -> Result<(), CacheError> {
    // Serializing a map of strings cannot fail.
    let text = serde_json::to_string_pretty(cache).unwrap_or_default();
    private_file::write(path, text.as_bytes()).map_err(|source| CacheError {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_texts_follow_the_message_rules() {
        let error = CacheError {
            path: "/c/versions.json".into(),
            source: std::io::Error::other("x"),
        };
        crate::style::assert_style(&[error.to_string()]);
    }
    use crate::{config::Target, protocol_version::ProtocolVersion, server::ServerName};

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ncp-protocol-version-{}-{test}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn saves_and_loads_the_cache() {
        let dir = scratch("round-trip");
        let path = dir.join("protocol-versions.json");
        let cache = VersionCache::default().record(
            ServerName::parse("s").unwrap(),
            Target::Remote { url: "u".into() },
            ProtocolVersion::first_with_discover(),
        );
        save(&path, &cache).unwrap();
        assert_eq!(load(&path).unwrap(), cache);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_or_broken_file_gives_an_empty_cache() {
        let dir = scratch("broken");
        let path = dir.join("protocol-versions.json");
        assert_eq!(load(&path).unwrap(), VersionCache::default());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(load(&path).unwrap(), VersionCache::default());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
