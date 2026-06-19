//! AC-DIFF-1/2/3 — secret fingerprint in convergence (anti-blind-spot).
//! Tests that the fingerprint is WRITTEN and READ BACK, and classifications work.

use cbox::core::{
    self,
    diff::diff_secrets,
    secret_inject::{
        build_env_keys, build_secret_specs, classify_secret_delta, env_secret_fingerprint,
        SecretSpecSnapshot,
    },
    spec::ApplySpec,
    state_store::{ProvisionState, ProvisionStateStore},
};
use cbox::dbox::{
    backend::Backend,
    mock::{MockMatcher, MockResponse, MockRunner},
    runner::DistroboxRunner,
};
use cbox::error::CboxError;
use std::collections::BTreeMap;
use tempfile::TempDir;

// ─── shared helpers ───────────────────────────────────────────────────────────

struct MemoryStore {
    state: std::sync::Mutex<ProvisionState>,
}

impl MemoryStore {
    #[allow(dead_code)]
    fn empty() -> Self {
        MemoryStore {
            state: std::sync::Mutex::new(ProvisionState::new()),
        }
    }

    fn with_state(s: ProvisionState) -> Self {
        MemoryStore {
            state: std::sync::Mutex::new(s),
        }
    }
}

impl ProvisionStateStore for MemoryStore {
    fn read(
        &self,
        _name: &str,
        _runner: &dyn DistroboxRunner,
    ) -> Result<ProvisionState, CboxError> {
        Ok(self.state.lock().unwrap().clone())
    }

    fn write(
        &self,
        _name: &str,
        state: &ProvisionState,
        _runner: &dyn DistroboxRunner,
    ) -> Result<(), CboxError> {
        *self.state.lock().unwrap() = state.clone();
        Ok(())
    }
}

fn mock_inspect_json(name: &str, image: &str) -> String {
    serde_json::json!([{
        "Id": "abc123",
        "State": { "Status": "running" },
        "Config": {
            "Image": image,
            "Labels": {
                "manager": "distrobox",
                "cbox.managed": "true",
                "cbox.docker_mode": "none",
                "cbox.boxfile_path": "",
                "cbox.version": "0.6.0",
                "cbox.image": image,
                "cbox.packages": ""
            }
        },
        "Mounts": [],
        "Created": "2026-06-19T00:00:00Z",
        "Name": name
    }])
    .to_string()
}

fn write_boxfile(dir: &TempDir, content: &str) -> String {
    let path = dir.path().join("Boxfile.toml");
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().to_string()
}

fn make_apply_spec(name: &str, boxfile_path: &str) -> ApplySpec {
    ApplySpec::new(name, boxfile_path, Backend::Podman)
}

// ─── Unit tests for diff_secrets and classify_secret_delta ───────────────────

#[test]
fn diff_secrets_returns_none_when_fingerprints_match() {
    use cbox::boxfile::model::{BoxConfig, Boxfile, DockerModeField, SandboxConfig};

    let bf = Boxfile {
        name: "mybox".to_string(),
        image: "fedora-toolbox:latest".to_string(),
        packages: vec![],
        docker: DockerModeField::None,
        mounts: vec![],
        sandbox: SandboxConfig::default(),
        box_config: BoxConfig::default(),
        provision: vec![],
        secrets: BTreeMap::new(),
        env: BTreeMap::new(),
    };

    let current_fp = env_secret_fingerprint(&bf);
    let prior_specs = build_secret_specs(&bf);

    // Same fingerprint → None
    let result = diff_secrets(&current_fp, &prior_specs, &bf);
    assert!(
        result.is_none(),
        "identical fingerprint must return None (no change)"
    );
}

#[test]
fn diff_secrets_returns_incremental_for_provision_only_add() {
    use cbox::boxfile::model::{BoxConfig, Boxfile, DockerModeField, SandboxConfig, SecretEntry};

    // Prior: one persist=false key
    let prior_specs = vec![SecretSpecSnapshot {
        key: "DB_URL".to_string(),
        persist: false,
        from: "keyring".to_string(),
    }];
    let prior_fp = {
        let mut prior_secrets = BTreeMap::new();
        prior_secrets.insert(
            "DB_URL".to_string(),
            SecretEntry {
                persist: false,
                from: "keyring".to_string(),
            },
        );
        let prior_bf = Boxfile {
            name: "mybox".to_string(),
            image: "fedora-toolbox:latest".to_string(),
            packages: vec![],
            docker: DockerModeField::None,
            mounts: vec![],
            sandbox: SandboxConfig::default(),
            box_config: BoxConfig::default(),
            provision: vec![],
            secrets: prior_secrets,
            env: BTreeMap::new(),
        };
        env_secret_fingerprint(&prior_bf)
    };

    // Current: adds another persist=false key
    let mut current_secrets = BTreeMap::new();
    current_secrets.insert(
        "DB_URL".to_string(),
        SecretEntry {
            persist: false,
            from: "keyring".to_string(),
        },
    );
    current_secrets.insert(
        "API_KEY".to_string(),
        SecretEntry {
            persist: false,
            from: "keyring".to_string(),
        },
    );
    let current_bf = Boxfile {
        name: "mybox".to_string(),
        image: "fedora-toolbox:latest".to_string(),
        packages: vec![],
        docker: DockerModeField::None,
        mounts: vec![],
        sandbox: SandboxConfig::default(),
        box_config: BoxConfig::default(),
        provision: vec![],
        secrets: current_secrets,
        env: BTreeMap::new(),
    };

    let result = diff_secrets(&prior_fp, &prior_specs, &current_bf);
    let field = result.expect("fingerprint changed → must return Some(DiffField)");
    assert_eq!(
        field.class, "Incremental",
        "only persist=false added → Incremental, got: {}",
        field.class
    );
}

#[test]
fn diff_secrets_returns_recreate_for_persist_true_add() {
    use cbox::boxfile::model::{BoxConfig, Boxfile, DockerModeField, SandboxConfig, SecretEntry};

    let prior_specs: Vec<SecretSpecSnapshot> = vec![];
    let prior_fp = env_secret_fingerprint(&Boxfile {
        name: "mybox".to_string(),
        image: "fedora-toolbox:latest".to_string(),
        packages: vec![],
        docker: DockerModeField::None,
        mounts: vec![],
        sandbox: SandboxConfig::default(),
        box_config: BoxConfig::default(),
        provision: vec![],
        secrets: BTreeMap::new(),
        env: BTreeMap::new(),
    });

    let mut current_secrets = BTreeMap::new();
    current_secrets.insert(
        "DB_URL".to_string(),
        SecretEntry {
            persist: true,
            from: "keyring".to_string(),
        },
    );
    let current_bf = Boxfile {
        name: "mybox".to_string(),
        image: "fedora-toolbox:latest".to_string(),
        packages: vec![],
        docker: DockerModeField::None,
        mounts: vec![],
        sandbox: SandboxConfig::default(),
        box_config: BoxConfig::default(),
        provision: vec![],
        secrets: current_secrets,
        env: BTreeMap::new(),
    };

    let result = diff_secrets(&prior_fp, &prior_specs, &current_bf);
    let field = result.expect("fingerprint changed → must return Some(DiffField)");
    assert_eq!(
        field.class, "Recreate",
        "persist=true key added → Recreate, got: {}",
        field.class
    );
}

#[test]
fn classify_persist_flip_false_to_true_is_recreate() {
    use cbox::boxfile::model::{BoxConfig, Boxfile, DockerModeField, SandboxConfig, SecretEntry};

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
    let bf = Boxfile {
        name: "mybox".to_string(),
        image: "fedora-toolbox:latest".to_string(),
        packages: vec![],
        docker: DockerModeField::None,
        mounts: vec![],
        sandbox: SandboxConfig::default(),
        box_config: BoxConfig::default(),
        provision: vec![],
        secrets,
        env: BTreeMap::new(),
    };
    assert_eq!(classify_secret_delta(&prior, &bf), "Recreate");
}

#[test]
fn classify_persist_flip_true_to_false_is_recreate() {
    use cbox::boxfile::model::{BoxConfig, Boxfile, DockerModeField, SandboxConfig, SecretEntry};

    let prior = vec![SecretSpecSnapshot {
        key: "TOKEN".to_string(),
        persist: true,
        from: "keyring".to_string(),
    }];
    let mut secrets = BTreeMap::new();
    secrets.insert(
        "TOKEN".to_string(),
        SecretEntry {
            persist: false,
            from: "keyring".to_string(),
        },
    );
    let bf = Boxfile {
        name: "mybox".to_string(),
        image: "fedora-toolbox:latest".to_string(),
        packages: vec![],
        docker: DockerModeField::None,
        mounts: vec![],
        sandbox: SandboxConfig::default(),
        box_config: BoxConfig::default(),
        provision: vec![],
        secrets,
        env: BTreeMap::new(),
    };
    assert_eq!(classify_secret_delta(&prior, &bf), "Recreate");
}

// ─── AC-DIFF-1: secret add/remove re-converges (Incremental) ─────────────────

/// GIVEN a box with stored state fingerprint for {DATABASE_URL persist=false} and a
/// Boxfile adding {API_KEY persist=false}
/// WHEN core::apply runs
/// THEN the diff detects the fingerprint change, classifies Incremental, and
/// writes the new fingerprint to state.
#[test]
fn ac_diff_1_secret_add_incremental() {
    let dir = TempDir::new().unwrap();

    // Current Boxfile: adds API_KEY persist=false
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "mybox"
image = "fedora-toolbox:latest"

[secrets]
DATABASE_URL = { persist = false }
API_KEY = { persist = false }
"#,
    );

    // Prior state: only DATABASE_URL fingerprint
    let prior_secrets: BTreeMap<String, cbox::boxfile::model::SecretEntry> = {
        let mut m = BTreeMap::new();
        m.insert(
            "DATABASE_URL".to_string(),
            cbox::boxfile::model::SecretEntry {
                persist: false,
                from: "keyring".to_string(),
            },
        );
        m
    };
    let prior_bf = cbox::boxfile::model::Boxfile {
        name: "mybox".to_string(),
        image: "fedora-toolbox:latest".to_string(),
        packages: vec![],
        docker: cbox::boxfile::model::DockerModeField::None,
        mounts: vec![],
        sandbox: cbox::boxfile::model::SandboxConfig::default(),
        box_config: cbox::boxfile::model::BoxConfig::default(),
        provision: vec![],
        secrets: prior_secrets.clone(),
        env: BTreeMap::new(),
    };

    let mut prior_state = ProvisionState::new();
    prior_state.env_secret_fingerprint = env_secret_fingerprint(&prior_bf);
    prior_state.secret_specs = build_secret_specs(&prior_bf);
    prior_state.env_keys = build_env_keys(&prior_bf);

    let store = MemoryStore::with_state(prior_state);

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "mybox",
                "fedora-toolbox:latest",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let spec = make_apply_spec("mybox", &bf_path);
    let outcome = core::apply(&spec, &store, &runner).expect("apply should succeed");

    // Diff should have detected the secret change as Incremental
    let secret_field = outcome.diff.fields.iter().find(|f| f.field == "secrets");
    assert!(
        secret_field.is_some(),
        "diff must include a 'secrets' field when fingerprint changes"
    );
    let secret_field = secret_field.unwrap();
    assert_eq!(
        secret_field.class, "Incremental",
        "only persist=false change → Incremental, got: {}",
        secret_field.class
    );

    // The new fingerprint must be written to state (AC-DIFF-3 anti-blind-spot)
    let new_state = store.read("mybox", &runner).unwrap();
    assert!(
        !new_state.env_secret_fingerprint.is_empty(),
        "new fingerprint must be written to state after apply"
    );
    assert_ne!(
        new_state.env_secret_fingerprint,
        env_secret_fingerprint(&prior_bf),
        "new fingerprint must differ from prior (API_KEY was added)"
    );
}

// ─── AC-DIFF-2: persist flip forces recreate ─────────────────────────────────

/// GIVEN stored state {TOKEN persist=false} and a Boxfile changing it to
/// {TOKEN persist=true}
/// WHEN core::apply runs WITHOUT --recreate
/// THEN exit 65 with the "needs a recreate" message listing `secrets` as a forcing field.
#[test]
fn ac_diff_2_persist_flip_forces_recreate_without_flag() {
    let dir = TempDir::new().unwrap();

    // Current Boxfile: TOKEN is now persist=true
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "mybox"
image = "fedora-toolbox:latest"

[secrets]
TOKEN = { persist = true }
"#,
    );

    // Prior state: TOKEN was persist=false
    let prior_secrets: BTreeMap<String, cbox::boxfile::model::SecretEntry> = {
        let mut m = BTreeMap::new();
        m.insert(
            "TOKEN".to_string(),
            cbox::boxfile::model::SecretEntry {
                persist: false,
                from: "keyring".to_string(),
            },
        );
        m
    };
    let prior_bf = cbox::boxfile::model::Boxfile {
        name: "mybox".to_string(),
        image: "fedora-toolbox:latest".to_string(),
        packages: vec![],
        docker: cbox::boxfile::model::DockerModeField::None,
        mounts: vec![],
        sandbox: cbox::boxfile::model::SandboxConfig::default(),
        box_config: cbox::boxfile::model::BoxConfig::default(),
        provision: vec![],
        secrets: prior_secrets,
        env: BTreeMap::new(),
    };

    let mut prior_state = ProvisionState::new();
    prior_state.env_secret_fingerprint = env_secret_fingerprint(&prior_bf);
    prior_state.secret_specs = build_secret_specs(&prior_bf);
    prior_state.env_keys = build_env_keys(&prior_bf);

    let store = MemoryStore::with_state(prior_state);

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "mybox",
                "fedora-toolbox:latest",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    // apply WITHOUT --recreate
    let spec = make_apply_spec("mybox", &bf_path);
    let err = core::apply(&spec, &store, &runner).unwrap_err();

    assert_eq!(
        err.exit_code(),
        65,
        "persist flip without --recreate must exit 65"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("secrets") || msg.contains("recreate"),
        "error must mention 'secrets' or 'recreate', got: {msg}"
    );
}

// ─── AC-DIFF-3: fingerprint written AND read (not a box.home blind spot) ──────

/// GIVEN a fresh box created with no secrets, provisioned once (fingerprint written)
/// WHEN core::apply runs again with the SAME Boxfile
/// THEN the diff reports NO secret change (fingerprint matches) and the 'secrets' field
/// does NOT appear in the diff — proving the field is read back, not write-once-ignored.
#[test]
fn ac_diff_3_fingerprint_written_and_read_no_blind_spot() {
    let dir = TempDir::new().unwrap();

    let bf_path = write_boxfile(
        &dir,
        r#"
name = "mybox"
image = "fedora-toolbox:latest"

[secrets]
DB_URL = { persist = false }
"#,
    );

    // Parse the boxfile to get its fingerprint
    let (bf, _) = cbox::boxfile::parse_file(&bf_path).unwrap();
    let expected_fp = env_secret_fingerprint(&bf);

    // Simulate a prior apply that wrote the fingerprint correctly
    let mut prior_state = ProvisionState::new();
    prior_state.env_secret_fingerprint = expected_fp.clone();
    prior_state.secret_specs = build_secret_specs(&bf);
    prior_state.env_keys = build_env_keys(&bf);

    let store = MemoryStore::with_state(prior_state);

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_json(
                "mybox",
                "fedora-toolbox:latest",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let spec = make_apply_spec("mybox", &bf_path);
    let outcome = core::apply(&spec, &store, &runner).expect("apply should succeed");

    // CRITICAL: no 'secrets' field in the diff when fingerprint unchanged
    // This proves the fingerprint is READ BACK correctly (not a write-once-ignored field).
    let secret_field = outcome.diff.fields.iter().find(|f| f.field == "secrets");
    assert!(
        secret_field.is_none(),
        "secrets must NOT appear in diff when fingerprint matches — \
         this is the anti-blind-spot check (AC-DIFF-3). Found: {:?}",
        outcome.diff.fields
    );

    assert_eq!(
        outcome.diff.class, "Incremental",
        "no changes → Incremental, got: {}",
        outcome.diff.class
    );
}
