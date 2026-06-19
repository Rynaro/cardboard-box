//! MockSecretStore — in-memory double for testing.
//! Available under `cfg(any(test, feature = "testkit"))`.

#![allow(dead_code)]

use super::{SecretError, SecretStore};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Fail mode for the mock — lets tests simulate keyring failures.
#[derive(Debug, Clone)]
pub enum FailMode {
    /// Simulate Secret Service unreachable.
    Unavailable(String),
    /// Return NotFound for every `get` call.
    AllNotFound,
}

/// In-memory `SecretStore` for unit and integration tests.
pub struct MockSecretStore {
    data: Mutex<BTreeMap<(String, String), String>>,
    /// When set, every operation returns this error.
    fail_mode: Option<FailMode>,
}

impl MockSecretStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(BTreeMap::new()),
            fail_mode: None,
        }
    }

    /// Pre-populate the store with a value.
    pub fn with_secret(self, box_name: &str, key: &str, value: &str) -> Self {
        self.data
            .lock()
            .unwrap()
            .insert((box_name.to_string(), key.to_string()), value.to_string());
        self
    }

    /// Make every operation return `Unavailable`.
    pub fn with_unavailable(mut self, msg: impl Into<String>) -> Self {
        self.fail_mode = Some(FailMode::Unavailable(msg.into()));
        self
    }

    /// Make every `get` return `NotFound` for the given key.
    pub fn with_all_not_found(mut self) -> Self {
        self.fail_mode = Some(FailMode::AllNotFound);
        self
    }
}

impl Default for MockSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for MockSecretStore {
    fn set(&self, box_name: &str, key: &str, value: &str) -> Result<(), SecretError> {
        if let Some(FailMode::Unavailable(msg)) = &self.fail_mode {
            return Err(SecretError::Unavailable(msg.clone()));
        }
        self.data
            .lock()
            .unwrap()
            .insert((box_name.to_string(), key.to_string()), value.to_string());
        Ok(())
    }

    fn get(&self, box_name: &str, key: &str) -> Result<Option<String>, SecretError> {
        match &self.fail_mode {
            Some(FailMode::Unavailable(msg)) => Err(SecretError::Unavailable(msg.clone())),
            Some(FailMode::AllNotFound) => Ok(None),
            None => {
                let guard = self.data.lock().unwrap();
                Ok(guard.get(&(box_name.to_string(), key.to_string())).cloned())
            }
        }
    }

    fn delete(&self, box_name: &str, key: &str) -> Result<(), SecretError> {
        if let Some(FailMode::Unavailable(msg)) = &self.fail_mode {
            return Err(SecretError::Unavailable(msg.clone()));
        }
        self.data
            .lock()
            .unwrap()
            .remove(&(box_name.to_string(), key.to_string()));
        Ok(())
    }

    fn list(&self, box_name: &str) -> Result<Vec<String>, SecretError> {
        if let Some(FailMode::Unavailable(msg)) = &self.fail_mode {
            return Err(SecretError::Unavailable(msg.clone()));
        }
        let guard = self.data.lock().unwrap();
        let keys: Vec<String> = guard
            .keys()
            .filter(|(bname, _)| bname == box_name)
            .map(|(_, k)| k.clone())
            .collect();
        Ok(keys)
    }
}
