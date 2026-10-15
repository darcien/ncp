//! File locations. `ncp` uses the XDG base directories on macOS and on
//! Linux, so one rule gives the same layout on both.

use std::path::{Path, PathBuf};

use crate::expand::Env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config_file: PathBuf,
    /// The folder of the caches: the protocol version cache and the list cache.
    pub cache_dir: PathBuf,
    pub version_cache: PathBuf,
    pub credential_file: PathBuf,
    /// The trust records of project files: one file for each path.
    pub trusted_configs: PathBuf,
    pub ignored_configs: PathBuf,
    /// The lock of the writes of `server add` and `server remove`.
    pub config_lock: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("The variable `HOME` is not set to an absolute path")]
pub struct NoHome;

impl Paths {
    /// # Errors
    ///
    /// Returns an error when an XDG variable has no value and `HOME` is not an
    /// absolute path.
    pub fn from_env(env: &Env) -> Result<Self, NoHome> {
        let home = || {
            env.get("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .ok_or(NoHome)
        };
        let base = |variable: &str, fallback: &str| -> Result<PathBuf, NoHome> {
            // The XDG specification says to ignore a relative path.
            match env.get(variable).map(Path::new) {
                Some(path) if path.is_absolute() => Ok(path.to_path_buf()),
                _ => Ok(home()?.join(fallback)),
            }
        };
        let cache_dir = base("XDG_CACHE_HOME", ".cache")?.join("ncp");
        let state_dir = base("XDG_STATE_HOME", ".local/state")?.join("ncp");
        Ok(Self {
            config_file: base("XDG_CONFIG_HOME", ".config")?.join("ncp/mcp.json"),
            version_cache: cache_dir.join("protocol-versions.json"),
            cache_dir,
            credential_file: state_dir.join("credentials.json"),
            trusted_configs: state_dir.join("trusted-configs"),
            ignored_configs: state_dir.join("ignored-configs"),
            config_lock: state_dir.join("config.lock"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Env {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_error_text_follows_the_message_rules() {
        crate::style::assert_style(&[NoHome.to_string()]);
    }

    #[test]
    fn uses_home_when_xdg_variables_are_not_set() {
        let paths = Paths::from_env(&env(&[("HOME", "/h")])).unwrap();
        assert_eq!(paths.config_file, Path::new("/h/.config/ncp/mcp.json"));
        assert_eq!(
            paths.version_cache,
            Path::new("/h/.cache/ncp/protocol-versions.json")
        );
        assert_eq!(
            paths.credential_file,
            Path::new("/h/.local/state/ncp/credentials.json")
        );
    }

    #[test]
    fn uses_xdg_variables_and_ignores_relative_ones() {
        let paths = Paths::from_env(&env(&[
            ("HOME", "/h"),
            ("XDG_CONFIG_HOME", "/c"),
            ("XDG_CACHE_HOME", "relative"),
            ("XDG_STATE_HOME", "/s"),
        ]))
        .unwrap();
        assert_eq!(paths.config_file, Path::new("/c/ncp/mcp.json"));
        assert_eq!(
            paths.version_cache,
            Path::new("/h/.cache/ncp/protocol-versions.json")
        );
        assert_eq!(paths.credential_file, Path::new("/s/ncp/credentials.json"));
        assert_eq!(paths.trusted_configs, Path::new("/s/ncp/trusted-configs"));
        assert_eq!(paths.ignored_configs, Path::new("/s/ncp/ignored-configs"));
    }

    #[test]
    fn needs_an_absolute_home_for_a_missing_xdg_variable() {
        assert_eq!(Paths::from_env(&env(&[])), Err(NoHome));
        assert_eq!(Paths::from_env(&env(&[("HOME", "h")])), Err(NoHome));
        assert!(
            Paths::from_env(&env(&[
                ("XDG_CONFIG_HOME", "/c"),
                ("XDG_CACHE_HOME", "/k"),
                ("XDG_STATE_HOME", "/s"),
            ]))
            .is_ok()
        );
    }
}
