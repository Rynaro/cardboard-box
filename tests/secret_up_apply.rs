//! AC-SEC-UP-* and AC-SEC-APPLY-* — D3 safety guarantee on the up/apply paths.
//!
//! The mock-unit test gap: existing AC-SEC-* tests drove `resolve_secret_env` and
//! `core::create` directly, never touching `cli::up::run_with_store` or
//! `cli::apply::run_with_store`. These tests close that gap.
//!
//! Strategy: drive `cli::up::run_with_store` / `cli::apply::run_with_store` with a
//! `MockSecretStore` and a real on-disk Boxfile, and a `MockRunner` that records all
//! calls.  We also test `core::up` / `core::apply` directly with pre-resolved env
//! fields to verify the threading all the way through.

use cbox::cli::{
    apply::{run_with_store as apply_run_with_store, ApplyArgs},
    output::OutputCtx,
    up::{run_with_store as up_run_with_store, UpArgs},
};
use cbox::core::{
    self,
    spec::{CreateSpec, DockerMode, UpSpec},
    state_store::{ProvisionState, ProvisionStateStore},
};
use cbox::dbox::{
    backend::Backend,
    mock::{MockMatcher, MockResponse, MockRunner},
    runner::DistroboxRunner,
};
use cbox::error::CboxError;
use cbox::secret::mock::MockSecretStore;
use tempfile::TempDir;

// ─── shared helpers ───────────────────────────────────────────────────────────

struct MemoryStore {
    state: std::sync::Mutex<ProvisionState>,
}

impl MemoryStore {
    fn empty() -> Self {
        MemoryStore {
            state: std::sync::Mutex::new(ProvisionState::new()),
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

fn mock_inspect_absent() -> MockResponse {
    // An empty array from podman inspect means "not found".
    MockResponse::ok("[]")
}

fn mock_inspect_present(name: &str, image: &str) -> String {
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
                "cbox.version": "0.1.0",
                "cbox.image": image,
                "cbox.packages": ""
            }
        },
        "Mounts": [],
        "Created": "2026-06-16T00:00:00Z",
        "Name": name
    }])
    .to_string()
}

fn write_boxfile(dir: &TempDir, content: &str) -> String {
    let path = dir.path().join("Boxfile.toml");
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().to_string()
}

fn quiet_ctx() -> OutputCtx {
    OutputCtx::new(false, true, 0, true)
}

fn default_up_args(file: &str) -> UpArgs {
    UpArgs {
        name: None,
        file: Some(file.to_string()),
        image: "registry.fedoraproject.org/fedora-toolbox:latest".to_string(),
        packages: vec![],
        mounts: vec![],
        docker: "none".to_string(),
        home: None,
        hostname: None,
        init: false,
        pull: false,
        isolated: false,
        force: false,
        redo: vec![],
        no_provision: false,
        recreate: false,
    }
}

fn default_apply_args(file: &str) -> ApplyArgs {
    ApplyArgs {
        name: None,
        file: Some(file.to_string()),
        force: false,
        redo: vec![],
        no_provision: false,
        recreate: false,
    }
}

// ─── AC-SEC-UP-1: D3 refusal — persist=true missing → exit 75, NOTHING spawned ──

/// GIVEN a Boxfile with [secrets] DATABASE_URL = { persist = true }
/// AND a MockSecretStore with no value (AllNotFound)
/// WHEN cli::up::run_with_store is called
/// THEN it returns exit 75
/// AND MockRunner.calls() is EMPTY (nothing was created or provisioned).
#[test]
fn ac_sec_up_1_missing_persist_true_secret_refuses_exit_75_nothing_spawned() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "api-dev"
image = "fedora-toolbox:latest"

[secrets]
DATABASE_URL = { persist = true }

[[provision]]
type = "shell"
run = "echo setup"
"#,
    );

    let store = MockSecretStore::new().with_all_not_found();
    let runner = MockRunner::new().with_default(MockResponse::ok(""));

    let args = default_up_args(&bf_path);
    let ctx = quiet_ctx();

    let err = up_run_with_store(
        &args,
        false,
        Some("podman"),
        false,
        &ctx,
        &runner,
        Some(&store),
    )
    .expect_err("should have refused with exit 75");

    // D3: must be exit 75
    assert_eq!(
        err.exit_code(),
        75,
        "missing persist=true secret must produce exit 75, got: {}",
        err.exit_code()
    );

    // D3: nothing must have been spawned
    let calls = runner.calls();
    assert_eq!(
        calls.len(),
        0,
        "D3 violated: {len} call(s) recorded when zero expected: {calls:?}",
        len = calls.len(),
        calls = calls
    );
}

// ─── AC-SEC-UP-2: D3 refusal — keyring unavailable → exit 75, nothing spawned ──

#[test]
fn ac_sec_up_2_keyring_unavailable_refuses_exit_75_nothing_spawned() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "api-dev"
image = "fedora-toolbox:latest"

[secrets]
DATABASE_URL = { persist = true }
"#,
    );

    let store = MockSecretStore::new().with_unavailable("D-Bus not available");
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    let args = default_up_args(&bf_path);
    let ctx = quiet_ctx();

    let err = up_run_with_store(
        &args,
        false,
        Some("podman"),
        false,
        &ctx,
        &runner,
        Some(&store),
    )
    .expect_err("should refuse when keyring is unavailable");

    assert_eq!(err.exit_code(), 75, "unavailable keyring must be exit 75");

    assert_eq!(
        runner.calls().len(),
        0,
        "D3: no spawn when keyring unavailable"
    );
}

// ─── AC-SEC-UP-3: secret present → create call carries --env KEY (name-only) ──

/// GIVEN a Boxfile with [secrets] DATABASE_URL = { persist = true }
/// AND MockSecretStore with ("api-dev", "DATABASE_URL") = "postgres://s3cr3t"
/// WHEN cli::up::run_with_store is called (box absent)
/// THEN the create call argv contains --additional-flags "--env DATABASE_URL" (name-only)
/// AND Invocation.env carries DATABASE_URL=postgres://s3cr3t
/// AND no call arg contains "postgres://s3cr3t" (INV-1).
#[test]
fn ac_sec_up_3_secret_present_create_call_carries_env_key_not_value() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "api-dev"
image = "fedora-toolbox:latest"

[secrets]
DATABASE_URL = { persist = true }

[[provision]]
type = "shell"
run = "echo setup"
"#,
    );

    let secret_value = "postgres://s3cr3t";
    let store = MockSecretStore::new().with_secret("api-dev", "DATABASE_URL", secret_value);

    // Box absent on first inspect; all other calls succeed.
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(mock_inspect_absent())
                .with_program("podman")
                .with_args_contain(vec!["inspect".to_string(), "api-dev".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let args = default_up_args(&bf_path);
    let ctx = quiet_ctx();

    up_run_with_store(
        &args,
        false,
        Some("podman"),
        false,
        &ctx,
        &runner,
        Some(&store),
    )
    .expect("should succeed when secret is present");

    let calls = runner.calls();

    // Find the create call
    let create_call = calls
        .iter()
        .find(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "create"))
        .expect("should have a distrobox create call");

    // argv must carry --additional-flags "--env DATABASE_URL" (name-only)
    let has_env_flag = create_call
        .args
        .windows(2)
        .any(|w| w[0] == "--additional-flags" && w[1] == "--env DATABASE_URL");
    assert!(
        has_env_flag,
        "create args must contain '--additional-flags \"--env DATABASE_URL\"', got: {:?}",
        create_call.args
    );

    // Invocation.env must carry the VALUE
    let has_env_value = create_call
        .env
        .iter()
        .any(|(k, v)| k == "DATABASE_URL" && v == secret_value);
    assert!(
        has_env_value,
        "create Invocation.env must carry DATABASE_URL={secret_value}, got: {:?}",
        create_call.env
    );

    // INV-1: value must NOT appear in any arg across all calls
    for call in &calls {
        assert!(
            !call.args.iter().any(|a| a.contains(secret_value)),
            "INV-1 violated: secret value in {:?} args: {:?}",
            call.program,
            call.args
        );
    }
}

// ─── AC-SEC-UP-4: persist=false secret threaded into provision plan ───────────

/// GIVEN a Boxfile with [secrets] STRIPE_KEY = { persist = false }
/// AND MockSecretStore with ("prov-box", "STRIPE_KEY") = "sk_test_abc"
/// WHEN core::up is called with the pre-resolved provision_env (simulating CLI path)
/// THEN the provision enter call carries --additional-flags "--env STRIPE_KEY" (name-only)
/// AND Invocation.env carries STRIPE_KEY=sk_test_abc
/// AND INV-1: value absent from all args.
#[test]
fn ac_sec_up_4_provision_only_secret_injected_into_provision_step() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "prov-box"
image = "fedora-toolbox:latest"

[secrets]
STRIPE_KEY = { persist = false }

[[provision]]
type = "shell"
run = "echo provision"
"#,
    );

    let secret_value = "sk_test_abc";

    // Box absent on first inspect
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(mock_inspect_absent())
                .with_program("podman")
                .with_args_contain(vec!["inspect".to_string(), "prov-box".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::empty();

    // Simulate what CLI would do: resolve ProvisionOnly secrets before calling core::up
    let cs = CreateSpec {
        name: "prov-box".to_string(),
        image: "fedora-toolbox:latest".to_string(),
        packages: vec![],
        docker_mode: DockerMode::None,
        mounts: vec![],
        home: None,
        hostname: None,
        init: false,
        pull: false,
        root: false,
        boxfile_path: Some(bf_path),
        unshare: None,
        backend: Backend::Podman,
        uid: 1000,
        dry_run: false,
        env_flags: vec![],  // persist=false → NOT in create env_flags
        env_values: vec![], // persist=false → NOT in create env_values
        plain_env: vec![],
    };

    let spec = UpSpec {
        create_spec: cs,
        apply_force: false,
        apply_redo: vec![],
        no_provision: false,
        recreate: false,
        yes: false,
        dry_run: false,
        // CLI would populate these from resolve_secret_env(..., ProvisionOnly, ...)
        provision_env_keys: vec!["STRIPE_KEY".to_string()],
        provision_env: vec![("STRIPE_KEY".to_string(), secret_value.to_string())],
    };

    core::up(&spec, &store, &runner).expect("should succeed");

    let calls = runner.calls();

    // Find the provision enter call
    let enter_call = calls
        .iter()
        .find(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "enter"))
        .expect("should have a provision enter call");

    // argv must carry --additional-flags "--env STRIPE_KEY" (name-only)
    let has_env_flag = enter_call
        .args
        .windows(2)
        .any(|w| w[0] == "--additional-flags" && w[1] == "--env STRIPE_KEY");
    assert!(
        has_env_flag,
        "provision enter must carry '--additional-flags \"--env STRIPE_KEY\"', got: {:?}",
        enter_call.args
    );

    // Invocation.env must carry STRIPE_KEY=value
    let has_value = enter_call
        .env
        .iter()
        .any(|(k, v)| k == "STRIPE_KEY" && v == secret_value);
    assert!(
        has_value,
        "provision Invocation.env must carry STRIPE_KEY={secret_value}, got: {:?}",
        enter_call.env
    );

    // INV-1: value must NOT appear in any arg across all calls
    for call in &calls {
        assert!(
            !call.args.iter().any(|a| a.contains(secret_value)),
            "INV-1 violated: secret value in {:?} args: {:?}",
            call.program,
            call.args
        );
    }
}

// ─── AC-SEC-APPLY-1: D3 refusal on apply — persist=false missing → exit 75 ────

/// GIVEN a Boxfile with [secrets] STRIPE_KEY = { persist = false }
/// AND a MockSecretStore with no value
/// WHEN cli::apply::run_with_store is called
/// THEN returns exit 75 AND nothing is spawned.
#[test]
fn ac_sec_apply_1_missing_provision_only_secret_refuses_exit_75() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "stripe-box"
image = "fedora-toolbox:latest"

[secrets]
STRIPE_KEY = { persist = false }

[[provision]]
type = "shell"
run = "echo provision"
"#,
    );

    let store = MockSecretStore::new().with_all_not_found();
    let runner = MockRunner::new().with_default(MockResponse::ok(""));

    let args = default_apply_args(&bf_path);
    let ctx = quiet_ctx();

    let err = apply_run_with_store(
        &args,
        false,
        Some("podman"),
        false,
        &ctx,
        &runner,
        Some(&store),
    )
    .expect_err("should refuse with exit 75 when persist=false secret is missing");

    assert_eq!(
        err.exit_code(),
        75,
        "missing persist=false secret must produce exit 75, got: {}",
        err.exit_code()
    );

    assert_eq!(
        runner.calls().len(),
        0,
        "D3: nothing spawned when apply secret missing"
    );
}

// ─── AC-SEC-APPLY-2: apply --recreate with persist=true missing → exit 75 ─────

/// GIVEN a Boxfile with [secrets] DATABASE_URL = { persist = true }
/// AND a MockSecretStore with no value
/// WHEN cli::apply::run_with_store with recreate=true is called
/// THEN returns exit 75 AND nothing is spawned.
#[test]
fn ac_sec_apply_2_missing_persist_true_on_recreate_refuses_exit_75() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "db-box"
image = "fedora-toolbox:latest"

[secrets]
DATABASE_URL = { persist = true }
"#,
    );

    let store = MockSecretStore::new().with_all_not_found();
    let runner = MockRunner::new().with_default(MockResponse::ok(""));

    let args = ApplyArgs {
        recreate: true,
        ..default_apply_args(&bf_path)
    };
    let ctx = quiet_ctx();

    let err = apply_run_with_store(
        &args,
        false,
        Some("podman"),
        false,
        &ctx,
        &runner,
        Some(&store),
    )
    .expect_err("should refuse on recreate when persist=true secret is missing");

    assert_eq!(
        err.exit_code(),
        75,
        "missing persist=true secret on recreate must produce exit 75"
    );

    assert_eq!(
        runner.calls().len(),
        0,
        "D3: nothing spawned when recreate secret missing"
    );
}

// ─── AC-SEC-APPLY-3: apply provision_env threaded into provision plan ─────────

/// GIVEN a Boxfile with persist=false secret and core::apply with provision_env populated
/// WHEN core::apply runs provision steps on an existing box
/// THEN the provision enter call carries --env KEY (name-only) and VALUE in env.
#[test]
fn ac_sec_apply_3_provision_env_threaded_into_provision_plan() {
    let dir = TempDir::new().unwrap();
    let bf_path = write_boxfile(
        &dir,
        r#"
name = "prov-apply-box"
image = "fedora-toolbox:latest"

[secrets]
API_TOKEN = { persist = false }

[[provision]]
type = "shell"
run = "echo provision"
"#,
    );

    let secret_value = "tok_live_abc123";

    // Box is PRESENT so apply goes incremental (no create)
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(mock_inspect_present(
                "prov-apply-box",
                "fedora-toolbox:latest",
            )))
            .with_program("podman")
            .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let store = MemoryStore::empty();

    let spec = cbox::core::spec::ApplySpec {
        provision_env_keys: vec!["API_TOKEN".to_string()],
        provision_env: vec![("API_TOKEN".to_string(), secret_value.to_string())],
        ..cbox::core::spec::ApplySpec::new("prov-apply-box", &bf_path, Backend::Podman)
    };

    core::apply(&spec, &store, &runner).expect("apply should succeed");

    let calls = runner.calls();

    // Find the provision enter call (shell step)
    let enter_call = calls
        .iter()
        .find(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "enter"))
        .expect("should have a provision enter call");

    // argv must carry --additional-flags "--env API_TOKEN"
    let has_env_flag = enter_call
        .args
        .windows(2)
        .any(|w| w[0] == "--additional-flags" && w[1] == "--env API_TOKEN");
    assert!(
        has_env_flag,
        "provision enter must carry '--additional-flags \"--env API_TOKEN\"', got: {:?}",
        enter_call.args
    );

    // Invocation.env must carry the value
    let has_value = enter_call
        .env
        .iter()
        .any(|(k, v)| k == "API_TOKEN" && v == secret_value);
    assert!(
        has_value,
        "provision Invocation.env must carry API_TOKEN={secret_value}, got: {:?}",
        enter_call.env
    );

    // INV-1: value must NOT appear in any arg
    for call in &calls {
        assert!(
            !call.args.iter().any(|a| a.contains(secret_value)),
            "INV-1 violated: secret value in {:?} args: {:?}",
            call.program,
            call.args
        );
    }
}
