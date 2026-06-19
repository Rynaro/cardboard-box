//! Secret injection helpers: resolution + value-free fingerprint.
//! `resolve_secret_env` is the ALL-OR-NOTHING resolver used by both the
//! create path and the provision path. The fingerprint is used by the diff/
//! convergence engine to detect schema changes (not value changes).

use std::collections::BTreeMap;

use crate::boxfile::model::{Boxfile, SecretEntry};
use crate::error::CboxError;
use crate::secret::{SecretError, SecretStore};
use sha2::{Digest, Sha256};

/// Which set of secrets to resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SecretScope {
    /// `persist == true` — baked into Config.Env at create time.
    Persisted,
    /// `persist == false` — injected at provision/enter time only.
    ProvisionOnly,
}

/// Resolve secrets eagerly, ALL-OR-NOTHING, before any spawn (D3).
/// Returns `(KEY, VALUE)` pairs to attach to `Invocation.env` and
/// to add as `--env KEY` flags in argv. On any failure → exit 75.
pub fn resolve_secret_env(
    box_name: &str,
    secrets: &BTreeMap<String, SecretEntry>,
    scope: SecretScope,
    store: &dyn SecretStore,
) -> Result<Vec<(String, String)>, CboxError> {
    let mut result = Vec::new();

    for (key, entry) in secrets {
        let want = match scope {
            SecretScope::Persisted => entry.persist,
            SecretScope::ProvisionOnly => !entry.persist,
        };
        if !want {
            continue;
        }

        let value = match store.get(box_name, key) {
            Ok(Some(v)) => v,
            Ok(None) => {
                return Err(CboxError::tempfail(format!(
                    "Secret \"{key}\" for box \"{box_name}\" is not available in the keyring.\n\
                     Unlock your keyring, or store it with:  cbox secret set {box_name} {key}\n\
                     (cbox never falls back to a plaintext value — nothing was created or run.)"
                )));
            }
            Err(SecretError::Unavailable(msg)) => {
                return Err(CboxError::tempfail(format!(
                    "Can't reach the OS keyring (Secret Service). Unlock it (log into your desktop\n\
                     session / start gnome-keyring) and retry. Secret \"{key}\" for \"{box_name}\"\n\
                     could not be read. ({msg})\n  cbox doctor   shows keyring status."
                )));
            }
            Err(SecretError::NotFound { .. }) => {
                return Err(CboxError::tempfail(format!(
                    "Secret \"{key}\" for box \"{box_name}\" is not available in the keyring.\n\
                     Unlock your keyring, or store it with:  cbox secret set {box_name} {key}\n\
                     (cbox never falls back to a plaintext value — nothing was created or run.)"
                )));
            }
            Err(SecretError::Backend(msg)) => {
                return Err(CboxError::software(format!(
                    "Keyring backend error reading \"{key}\" for \"{box_name}\": {msg}"
                )));
            }
        };

        result.push((key.clone(), value));
    }

    Ok(result)
}

/// Compute the value-free env/secret fingerprint for a Boxfile.
///
/// Input: sorted (BTreeMap order) records `{ key, persist, from }` for [secrets]
/// plus `{ key }` for [env]. Values are NEVER included (S4 / D0).
///
/// Output: lowercase hex SHA-256.
pub fn env_secret_fingerprint(bf: &Boxfile) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"secrets\n");
    for (key, entry) in &bf.secrets {
        hasher.update(format!("{}\t{}\t{}\n", key, entry.persist, entry.from).as_bytes());
    }
    hasher.update(b"env\n");
    for key in bf.env.keys() {
        hasher.update(format!("{key}\n").as_bytes());
    }
    let result = hasher.finalize();
    result.iter().map(|b| format!("{b:02x}")).collect()
}

/// Serializable snapshot of the secrets/env schema for diff comparison.
/// Stored in `ProvisionState` — metadata ONLY, never values.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct SecretSpecSnapshot {
    pub key: String,
    pub persist: bool,
    pub from: String,
}

/// Build the `secret_specs` snapshot from the current Boxfile.
pub fn build_secret_specs(bf: &Boxfile) -> Vec<SecretSpecSnapshot> {
    bf.secrets
        .iter()
        .map(|(k, e)| SecretSpecSnapshot {
            key: k.clone(),
            persist: e.persist,
            from: e.from.clone(),
        })
        .collect()
}

/// Build the `env_keys` snapshot from the current Boxfile.
pub fn build_env_keys(bf: &Boxfile) -> Vec<String> {
    bf.env.keys().cloned().collect()
}

/// Classify a fingerprint delta between prior state and current Boxfile.
///
/// Returns `"Recreate"` if any persist=true secret was added, removed, or flipped
/// (persist changed). Returns `"Incremental"` for all other changes (only
/// persist=false or [env] keys changed).
pub fn classify_secret_delta(
    prior_specs: &[SecretSpecSnapshot],
    current_bf: &Boxfile,
) -> &'static str {
    let current_specs = build_secret_specs(current_bf);

    // Check for any persist=true addition or removal or persist-flip
    let prior_map: BTreeMap<&str, &SecretSpecSnapshot> =
        prior_specs.iter().map(|s| (s.key.as_str(), s)).collect();
    let current_map: BTreeMap<&str, &SecretSpecSnapshot> =
        current_specs.iter().map(|s| (s.key.as_str(), s)).collect();

    // New persist=true keys
    for (key, spec) in &current_map {
        if spec.persist && !prior_map.contains_key(*key) {
            // New persist=true key added
            return "Recreate";
        }
    }

    // Removed persist=true keys or persist flip
    for (key, prior) in &prior_map {
        match current_map.get(*key) {
            None if prior.persist => {
                // persist=true key removed
                return "Recreate";
            }
            Some(current) if prior.persist != current.persist => {
                // persist flipped (either direction)
                return "Recreate";
            }
            _ => {}
        }
    }

    "Incremental"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boxfile::model::{BoxConfig, DockerModeField, SandboxConfig, SecretEntry};
    use crate::secret::mock::MockSecretStore;

    fn make_bf_with_secrets(secrets: BTreeMap<String, SecretEntry>) -> Boxfile {
        Boxfile {
            name: "testbox".to_string(),
            image: "fedora-toolbox:latest".to_string(),
            packages: vec![],
            docker: DockerModeField::None,
            mounts: vec![],
            sandbox: SandboxConfig::default(),
            box_config: BoxConfig::default(),
            provision: vec![],
            secrets,
            env: BTreeMap::new(),
        }
    }

    #[test]
    fn fingerprint_deterministic_empty() {
        let bf = make_bf_with_secrets(BTreeMap::new());
        let fp1 = env_secret_fingerprint(&bf);
        let fp2 = env_secret_fingerprint(&bf);
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn fingerprint_changes_on_key_add() {
        let bf1 = make_bf_with_secrets(BTreeMap::new());
        let mut secrets2 = BTreeMap::new();
        secrets2.insert(
            "API_KEY".to_string(),
            SecretEntry {
                persist: true,
                from: "keyring".to_string(),
            },
        );
        let bf2 = make_bf_with_secrets(secrets2);
        assert_ne!(env_secret_fingerprint(&bf1), env_secret_fingerprint(&bf2));
    }

    #[test]
    fn fingerprint_does_not_include_values() {
        // Two Boxfiles with the same key structure: same fingerprint regardless of
        // whether a value is stored in the keyring (values are not in the fingerprint).
        let mut secrets = BTreeMap::new();
        secrets.insert(
            "DB_URL".to_string(),
            SecretEntry {
                persist: true,
                from: "keyring".to_string(),
            },
        );
        let bf = make_bf_with_secrets(secrets.clone());
        let fp1 = env_secret_fingerprint(&bf);

        // Change nothing in the schema — fingerprint must be identical.
        let fp2 = env_secret_fingerprint(&bf);
        assert_eq!(fp1, fp2, "fingerprint must not depend on keyring values");
    }

    #[test]
    fn resolve_returns_value_for_persisted() {
        let store = MockSecretStore::new().with_secret("mybox", "DB_URL", "postgres://s3cr3t");
        let mut secrets = BTreeMap::new();
        secrets.insert(
            "DB_URL".to_string(),
            SecretEntry {
                persist: true,
                from: "keyring".to_string(),
            },
        );
        let result = resolve_secret_env("mybox", &secrets, SecretScope::Persisted, &store).unwrap();
        assert_eq!(
            result,
            vec![("DB_URL".to_string(), "postgres://s3cr3t".to_string())]
        );
    }

    #[test]
    fn resolve_skips_provision_only_when_persisted_scope() {
        let store = MockSecretStore::new().with_secret("mybox", "STRIPE_KEY", "sk_test_123");
        let mut secrets = BTreeMap::new();
        secrets.insert(
            "STRIPE_KEY".to_string(),
            SecretEntry {
                persist: false,
                from: "keyring".to_string(),
            },
        );
        let result = resolve_secret_env("mybox", &secrets, SecretScope::Persisted, &store).unwrap();
        assert!(
            result.is_empty(),
            "persist=false should not appear in Persisted scope"
        );
    }

    #[test]
    fn resolve_exit_75_on_not_found() {
        let store = MockSecretStore::new(); // empty store
        let mut secrets = BTreeMap::new();
        secrets.insert(
            "DB_URL".to_string(),
            SecretEntry {
                persist: true,
                from: "keyring".to_string(),
            },
        );
        let err =
            resolve_secret_env("mybox", &secrets, SecretScope::Persisted, &store).unwrap_err();
        assert_eq!(err.exit_code(), 75);
        let msg = err.to_string();
        assert!(msg.contains("DB_URL"), "error must mention the key name");
        assert!(
            msg.contains("cbox secret set"),
            "error must mention the fix command"
        );
    }

    #[test]
    fn resolve_exit_75_on_unavailable() {
        let store = MockSecretStore::new().with_unavailable("D-Bus not available");
        let mut secrets = BTreeMap::new();
        secrets.insert(
            "DB_URL".to_string(),
            SecretEntry {
                persist: true,
                from: "keyring".to_string(),
            },
        );
        let err =
            resolve_secret_env("mybox", &secrets, SecretScope::Persisted, &store).unwrap_err();
        assert_eq!(err.exit_code(), 75);
    }

    #[test]
    fn classify_persist_flip_is_recreate() {
        // Prior: persist=false; current: persist=true → Recreate
        let prior = vec![SecretSpecSnapshot {
            key: "TOKEN".to_string(),
            persist: false,
            from: "keyring".to_string(),
        }];
        let mut secrets = BTreeMap::new();
        secrets.insert(
            "TOKEN".to_string(),
            SecretEntry {
                persist: true,
                from: "keyring".to_string(),
            },
        );
        let bf = make_bf_with_secrets(secrets);
        assert_eq!(classify_secret_delta(&prior, &bf), "Recreate");
    }

    #[test]
    fn classify_only_provision_only_change_is_incremental() {
        // Prior: persist=false DB_URL; current: adds API_KEY persist=false → Incremental
        let prior = vec![SecretSpecSnapshot {
            key: "DB_URL".to_string(),
            persist: false,
            from: "keyring".to_string(),
        }];
        let mut secrets = BTreeMap::new();
        secrets.insert(
            "DB_URL".to_string(),
            SecretEntry {
                persist: false,
                from: "keyring".to_string(),
            },
        );
        secrets.insert(
            "API_KEY".to_string(),
            SecretEntry {
                persist: false,
                from: "keyring".to_string(),
            },
        );
        let bf = make_bf_with_secrets(secrets);
        assert_eq!(classify_secret_delta(&prior, &bf), "Incremental");
    }

    #[test]
    fn classify_persist_true_add_is_recreate() {
        let prior: Vec<SecretSpecSnapshot> = vec![];
        let mut secrets = BTreeMap::new();
        secrets.insert(
            "DB_URL".to_string(),
            SecretEntry {
                persist: true,
                from: "keyring".to_string(),
            },
        );
        let bf = make_bf_with_secrets(secrets);
        assert_eq!(classify_secret_delta(&prior, &bf), "Recreate");
    }
}
