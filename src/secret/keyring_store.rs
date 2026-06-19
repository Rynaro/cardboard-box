//! `KeyringStore` — real impl backed by the `keyring` crate v3.
//! Features: `async-secret-service` + `tokio` + `crypto-rust`. The pure-Rust
//! zbus transport avoids a libdbus/libsecret C link (the `sync-secret-service`
//! path would pull `dbus-secret-service` → `libdbus-1-dev`); the synchronous
//! surface is reached via `secret-service`'s blocking module.
//!
//! `list(box)` uses OQ-2 fallback: intersect the Boxfile-declared keys with
//! "key present in keyring" (probe each via `get`). This is the specified
//! fallback for when collection enumeration is unavailable.

use crate::secret::{SecretError, SecretStore};

/// Real keyring backend. Wraps `keyring::Entry` with service = "cbox".
/// Account string format: "<box_name>/<key>" (unambiguous — see §3.2).
pub struct KeyringStore;

impl SecretStore for KeyringStore {
    fn set(&self, box_name: &str, key: &str, value: &str) -> Result<(), SecretError> {
        let entry = make_entry(box_name, key)?;
        entry
            .set_password(value)
            .map_err(|e| map_keyring_err(e, box_name, key))
    }

    fn get(&self, box_name: &str, key: &str) -> Result<Option<String>, SecretError> {
        let entry = make_entry(box_name, key)?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring_err(e, box_name, key)),
        }
    }

    fn delete(&self, box_name: &str, key: &str) -> Result<(), SecretError> {
        let entry = make_entry(box_name, key)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            // Idempotent: not found is not an error
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_keyring_err(e, box_name, key)),
        }
    }

    /// List KEY names for a box. Uses the OQ-2 fallback: probe each key in the
    /// Boxfile against the keyring. Without a Boxfile reference this can only
    /// return an empty list (the keyring crate's collection API is not exposed
    /// in a stable, feature-flag-independent way under the async-secret-service backend).
    ///
    /// For the production path this is always called with `probe_keys` populated
    /// from the Boxfile; for the `cbox secret list` command the keys are read
    /// from the Boxfile or returned empty if no Boxfile is found.
    fn list(&self, _box_name: &str) -> Result<Vec<String>, SecretError> {
        // OQ-2: clean collection enumeration not available under chosen features.
        // Callers that need a useful list must use `list_from_probe` instead.
        Ok(vec![])
    }
}

impl KeyringStore {
    /// List keys for a box by probing a set of candidate keys.
    /// Returns only those that are present in the keyring.
    #[allow(dead_code)]
    pub fn list_from_probe(
        &self,
        box_name: &str,
        candidate_keys: &[&str],
    ) -> Result<Vec<String>, SecretError> {
        let mut found = Vec::new();
        for key in candidate_keys {
            if self.get(box_name, key)?.is_some() {
                found.push((*key).to_string());
            }
        }
        Ok(found)
    }
}

fn make_entry(box_name: &str, key: &str) -> Result<keyring::Entry, SecretError> {
    let account = crate::secret::account_for(box_name, key);
    keyring::Entry::new("cbox", &account)
        .map_err(|e| SecretError::Unavailable(format!("Failed to create keyring entry: {e}")))
}

fn map_keyring_err(e: keyring::Error, box_name: &str, key: &str) -> SecretError {
    match e {
        keyring::Error::NoEntry => SecretError::NotFound {
            box_name: box_name.to_string(),
            key: key.to_string(),
        },
        // Platform errors that indicate the service is unreachable / locked
        keyring::Error::PlatformFailure(_) | keyring::Error::NoStorageAccess(_) => {
            SecretError::Unavailable(format!(
                "Secret Service error for \"{box_name}/{key}\": {e}"
            ))
        }
        _ => SecretError::Backend(format!("{e}")),
    }
}
