//! A store in memory, for tests.

use std::{collections::BTreeMap, sync::Mutex};

use super::{CredentialStore, StoreError};
use crate::credential::{CredentialKey, Secret};

#[derive(Default)]
pub struct MemoryStore {
    secrets: Mutex<BTreeMap<CredentialKey, Secret>>,
}

impl MemoryStore {
    fn with<T>(
        &self,
        change: impl FnOnce(&mut BTreeMap<CredentialKey, Secret>) -> T,
    ) -> Result<T, StoreError> {
        self.secrets
            .lock()
            .map(|mut secrets| change(&mut secrets))
            .map_err(|_| StoreError::Failure("The lock of the memory store is poisoned".into()))
    }
}

impl CredentialStore for MemoryStore {
    fn load(&self, key: &CredentialKey) -> Result<Option<Secret>, StoreError> {
        self.with(|secrets| secrets.get(key).cloned())
    }

    fn save(&self, key: &CredentialKey, secret: &Secret) -> Result<(), StoreError> {
        self.with(|secrets| {
            secrets.insert(key.clone(), secret.clone());
        })
    }

    fn delete(&self, key: &CredentialKey) -> Result<bool, StoreError> {
        self.with(|secrets| secrets.remove(key).is_some())
    }

    fn keys(&self) -> Result<Vec<CredentialKey>, StoreError> {
        self.with(|secrets| secrets.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_keeps_secrets_by_key() {
        super::super::contract::check(&MemoryStore::default());
    }
}
