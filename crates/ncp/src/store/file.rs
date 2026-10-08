//! The opt-in file store: one JSON file with mode `0600`.
//!
//! Use it only after an explicit opt-in, for a system with no native store.
//! The store does not read a file that other users can open, as ssh does for
//! private keys.

use std::{collections::BTreeMap, fs, io::ErrorKind, os::unix::fs::PermissionsExt, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::{CredentialStore, StoreError};
use crate::{
    credential::{CredentialKey, Secret},
    private_file,
};

const FORMAT_VERSION: u32 = 1;

/// Mode bits for group and other users.
const OPEN_BITS: u32 = 0o077;

pub struct FileStore {
    path: PathBuf,
}

/// The content of the file: the account of each key, and its value.
#[derive(Default, Serialize, Deserialize)]
struct Content {
    version: u32,
    secrets: BTreeMap<String, String>,
}

impl FileStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn read(&self) -> Result<Content, StoreError> {
        let metadata = match fs::metadata(&self.path) {
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Content::default()),
            other => other.map_err(|source| self.io(source))?,
        };
        if metadata.permissions().mode() & OPEN_BITS != 0 {
            return Err(StoreError::OpenPermissions(self.path.clone()));
        }
        let text = fs::read_to_string(&self.path).map_err(|source| self.io(source))?;
        decode(&text).ok_or_else(|| StoreError::Corrupt(self.path.clone()))
    }

    fn write(&self, content: &Content) -> Result<(), StoreError> {
        private_file::write(&self.path, encode(content).as_bytes())
            .map_err(|source| self.io(source))
    }

    fn change(&self, edit: impl FnOnce(&mut BTreeMap<String, String>)) -> Result<(), StoreError> {
        let mut content = self.read()?;
        edit(&mut content.secrets);
        content.version = FORMAT_VERSION;
        self.write(&content)
    }

    fn io(&self, source: std::io::Error) -> StoreError {
        StoreError::Io {
            path: self.path.clone(),
            source,
        }
    }
}

impl CredentialStore for FileStore {
    fn load(&self, key: &CredentialKey) -> Result<Option<Secret>, StoreError> {
        Ok(self.read()?.secrets.remove(&key.account()).map(Secret::new))
    }

    fn save(&self, key: &CredentialKey, secret: &Secret) -> Result<(), StoreError> {
        self.change(|secrets| {
            secrets.insert(key.account(), secret.expose().to_string());
        })
    }

    fn delete(&self, key: &CredentialKey) -> Result<bool, StoreError> {
        // The file is opt-in: a delete of a name that it does not have
        // must not make the file.
        if !self.read()?.secrets.contains_key(&key.account()) {
            return Ok(false);
        }
        let mut removed = false;
        self.change(|secrets| removed = secrets.remove(&key.account()).is_some())?;
        Ok(removed)
    }

    fn keys(&self) -> Result<Vec<CredentialKey>, StoreError> {
        let content = self.read()?;
        Ok(super::keys_of(content.secrets.keys().map(String::as_str)))
    }
}

fn decode(text: &str) -> Option<Content> {
    serde_json::from_str::<Content>(text)
        .ok()
        .filter(|content| content.version == FORMAT_VERSION)
}

fn encode(content: &Content) -> String {
    // Serializing a map of strings cannot fail.
    serde_json::to_string_pretty(content).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::server::ServerName;

    /// A new, empty directory for one test.
    fn scratch_dir(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ncp-test-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn key() -> CredentialKey {
        CredentialKey::OAuthTokens(ServerName::parse("s").unwrap())
    }

    #[test]
    fn file_store_keeps_secrets_by_key() {
        let dir = scratch_dir("contract");
        super::super::contract::check(&FileStore::new(dir.join("credentials.json")));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn creates_file_with_mode_0600_in_directory_with_mode_0700() {
        let dir = scratch_dir("modes");
        let path = dir.join("sub").join("credentials.json");
        FileStore::new(path.clone())
            .save(&key(), &Secret::new("x".into()))
            .unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_file_that_other_users_can_read() {
        let dir = scratch_dir("open");
        let path = dir.join("credentials.json");
        let store = FileStore::new(path.clone());
        store.save(&key(), &Secret::new("x".into())).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            store.load(&key()),
            Err(StoreError::OpenPermissions(_))
        ));
        assert!(matches!(
            store.save(&key(), &Secret::new("y".into())),
            Err(StoreError::OpenPermissions(_))
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reports_corrupt_file_without_its_content() {
        let dir = scratch_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("credentials.json");
        fs::write(&path, "{\"secret-value\"").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let error = FileStore::new(path).load(&key()).unwrap_err();
        assert!(matches!(error, StoreError::Corrupt(_)));
        assert!(!error.to_string().contains("secret-value"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_file_of_another_format_version() {
        assert!(decode(r#"{"version": 2, "secrets": {}}"#).is_none());
        assert!(decode(r#"{"version": 1, "secrets": {"a": "b"}}"#).is_some());
    }

    #[test]
    fn leaves_no_temporary_file_after_write() {
        let dir = scratch_dir("temporary");
        let store = FileStore::new(dir.join("credentials.json"));
        store.save(&key(), &Secret::new("x".into())).unwrap();
        store.delete(&key()).unwrap();
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["credentials.json"]);
        fs::remove_dir_all(dir).unwrap();
    }
}
