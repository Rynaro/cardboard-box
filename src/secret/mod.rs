//! Secret storage seam: `SecretStore` trait + error type.
//! Real impl: `KeyringStore` (keyring crate, added in milestone 9).
//! Test seam: `MockSecretStore` (cfg(any(test, feature = "testkit"))).

pub mod keyring_store;
/// MockSecretStore — in-memory double for testing. Mirroring `dbox::mock` (always exported).
pub mod mock;

/// Error type for secret-store operations.
/// Maps to CboxError at the call boundary (see `into_cbox_error` helpers).
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// Secret Service unreachable / locked / no provider (D3).
    #[error("{0}")]
    Unavailable(String),

    /// A referenced KEY has no stored value.
    #[error("Secret \"{key}\" for box \"{box_name}\" not found in the keyring.")]
    NotFound { box_name: String, key: String },

    /// Any other backend failure.
    #[error("keyring backend error: {0}")]
    Backend(String),
}

/// Test seam over the OS keyring.
/// Exactly ONE real impl ships (KeyringStore); the mock exists for tests only (D1).
pub trait SecretStore: Send + Sync {
    /// Store a secret value. Overwrites if already present.
    fn set(&self, box_name: &str, key: &str, value: &str) -> Result<(), SecretError>;

    /// Retrieve a secret value. Returns None if the key does not exist (not an error).
    fn get(&self, box_name: &str, key: &str) -> Result<Option<String>, SecretError>;

    /// Delete a secret. Idempotent: deleting an absent key is success.
    fn delete(&self, box_name: &str, key: &str) -> Result<(), SecretError>;

    /// List KEY names stored for a given box. Returns names only, never values.
    fn list(&self, box_name: &str) -> Result<Vec<String>, SecretError>;
}

/// Namespace helper: convert (box_name, key) to the keyring account string.
/// Format: "<box_name>/<key>" — unambiguous because '/' is invalid in both name and key.
pub fn account_for(box_name: &str, key: &str) -> String {
    format!("{box_name}/{key}")
}

/// Parse a keyring account string back to (box_name, key).
#[allow(dead_code)]
pub fn parse_account(account: &str) -> Option<(&str, &str)> {
    account.split_once('/')
}
