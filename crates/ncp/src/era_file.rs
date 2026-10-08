//! The era cache file.
//!
//! It is a cache: `ncp` works the same when the file is removed. A file that
//! does not load is ignored, and the next save writes it again.

use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

use crate::{era::EraCache, private_file};

#[derive(Debug, thiserror::Error)]
#[error("era cache {path}: {source}")]
pub struct CacheError {
    path: PathBuf,
    source: std::io::Error,
}

/// # Errors
///
/// Returns an error when the file exists but cannot be read.
pub fn load(path: &Path) -> Result<EraCache, CacheError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_default()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(EraCache::default()),
        Err(source) => Err(CacheError {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// # Errors
///
/// Returns an error when the file cannot be written.
pub fn save(path: &Path, cache: &EraCache) -> Result<(), CacheError> {
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
    use crate::{
        config::Target,
        era::{Era, ProtocolVersion},
        server::ServerName,
    };

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ncp-era-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn saves_and_loads_the_cache() {
        let dir = scratch("round-trip");
        let path = dir.join("eras.json");
        let cache = EraCache::default().record(
            ServerName::parse("s").unwrap(),
            Target::Remote { url: "u".into() },
            Era::Modern {
                version: ProtocolVersion::first_modern(),
            },
        );
        save(&path, &cache).unwrap();
        assert_eq!(load(&path).unwrap(), cache);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_or_broken_file_gives_an_empty_cache() {
        let dir = scratch("broken");
        let path = dir.join("eras.json");
        assert_eq!(load(&path).unwrap(), EraCache::default());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(load(&path).unwrap(), EraCache::default());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
