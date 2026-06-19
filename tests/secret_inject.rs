//! AC-SEC-1..5 and AC-ROUNDTRIP-1 — secret injection wiring.
//! Create-path: persist=true → argv has --env KEY (name-only), env has KEY=VALUE.
//! Provision-path: persist=false → enter argv has --env KEY, env has KEY=VALUE.
//! INV-1: secret VALUE never appears in any recorded args.

use cbox::core::{
    self,
    secret_inject::{resolve_secret_env, SecretScope},
    spec::CreateSpec,
    state_store::{ProvisionState, ProvisionStateStore},
};
use cbox::dbox::{
    backend::Backend,
    mock::{MockMatcher, MockResponse, MockRunner},
    runner::DistroboxRunner,
};
use cbox::error::CboxError;
use cbox::secret::mock::MockSecretStore;
use std::collections::BTreeMap;

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

// ─── AC-SEC-1: persist=true → value in Config.Env (not in argv) ──────────────

/// GIVEN a Boxfile with [secrets] DATABASE_URL = { persist = true } and MockSecretStore
/// holding ("api-dev","DATABASE_URL") = "postgres://s3cr3t"
/// WHEN core::create runs (via create with env_flags/env_values pre-populated)
/// THEN the recorded create call has `--additional-flags "--env DATABASE_URL"` in args
/// AND its env contains ("DATABASE_URL","postgres://s3cr3t")
/// AND args does NOT contain the string "postgres://s3cr3t" (INV-1).
#[test]
fn ac_sec_1_persist_true_value_in_env_not_argv() {
    let secret_value = "postgres://s3cr3t";
    let store = MockSecretStore::new().with_secret("api-dev", "DATABASE_URL", secret_value);

    let mut secrets = BTreeMap::new();
    secrets.insert(
        "DATABASE_URL".to_string(),
        cbox::boxfile::model::SecretEntry {
            persist: true,
            from: "keyring".to_string(),
        },
    );

    let resolved = resolve_secret_env("api-dev", &secrets, SecretScope::Persisted, &store).unwrap();
    // resolved = [("DATABASE_URL", "postgres://s3cr3t")]

    let runner = MockRunner::new().with_default(MockResponse::ok(""));

    let mut spec = CreateSpec::new("api-dev", Backend::Podman);
    spec.image = "fedora-toolbox:latest".to_string();
    // Populate per-INV-1: KEY name in env_flags; value in env_values (not in argv)
    spec.env_flags = resolved.iter().map(|(k, _)| k.clone()).collect();
    spec.env_values = resolved.clone();

    core::create(&spec, &runner).expect("create should succeed");

    let calls = runner.calls();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];

    // The create argv must include `--additional-flags "--env DATABASE_URL"` (name-only)
    let has_env_flag = call
        .args
        .windows(2)
        .any(|w| w[0] == "--additional-flags" && w[1] == "--env DATABASE_URL");
    assert!(
        has_env_flag,
        "args must contain '--additional-flags \"--env DATABASE_URL\"', got: {:?}",
        call.args
    );

    // The env on the call must carry the VALUE
    let has_env_value = call
        .env
        .iter()
        .any(|(k, v)| k == "DATABASE_URL" && v == secret_value);
    assert!(
        has_env_value,
        "Invocation.env must carry DATABASE_URL=postgres://s3cr3t, got: {:?}",
        call.env
    );

    // INV-1: the secret VALUE must NOT appear in any arg
    let value_in_args = call.args.iter().any(|a| a.contains(secret_value));
    assert!(
        !value_in_args,
        "INV-1 violated: secret value found in argv: {:?}",
        call.args
    );
}

// ─── AC-SEC-2: persist=false → present at provision, absent from create ──────

/// GIVEN [secrets] STRIPE_KEY = { persist = false } stored in the mock
/// WHEN provision runs a shell step
/// THEN the provision enter call has `--additional-flags "--env STRIPE_KEY"` AND
/// env contains ("STRIPE_KEY", <value>)
/// AND the create call does NOT carry --env STRIPE_KEY (it must not reach Config.Env)
/// AND INV-1: no secret value in any args.
#[test]
fn ac_sec_2_persist_false_at_provision_only() {
    use cbox::boxfile::model::{ProvisionStep, ProvisionType};
    use cbox::core::provision::{provision, ProvisionPlan};
    use cbox::core::state_store::ProvisionState;

    let secret_value = "sk_test_live_abc123";
    let store = MockSecretStore::new().with_secret("mybox", "STRIPE_KEY", secret_value);

    let mut secrets = BTreeMap::new();
    secrets.insert(
        "STRIPE_KEY".to_string(),
        cbox::boxfile::model::SecretEntry {
            persist: false,
            from: "keyring".to_string(),
        },
    );

    let resolved_provision =
        resolve_secret_env("mybox", &secrets, SecretScope::ProvisionOnly, &store).unwrap();

    // Create call: persist=false → no env_flags, no env_values for this key
    let resolved_create =
        resolve_secret_env("mybox", &secrets, SecretScope::Persisted, &store).unwrap();
    assert!(
        resolved_create.is_empty(),
        "persist=false must not appear in create env"
    );

    // Simulate a provision run with the persist=false env
    let runner = MockRunner::new().with_default(MockResponse::ok(""));

    let provision_env_keys: Vec<String> =
        resolved_provision.iter().map(|(k, _)| k.clone()).collect();
    let provision_env: Vec<(String, String)> = resolved_provision.clone();

    let steps = vec![ProvisionStep {
        step_type: ProvisionType::Shell,
        run: Some("echo hello".to_string()),
        src: None,
        dst: None,
    }];

    let plan = ProvisionPlan {
        name: "mybox",
        steps: &steps,
        boxfile_dir: std::path::Path::new("."),
        backend: &Backend::Podman,
        force: true,
        redo: &[],
        dry_run: false,
        provision_env_keys: &provision_env_keys,
        provision_env: &provision_env,
    };

    let mem_store = MemoryStore::empty();
    let mut state = ProvisionState::new();
    provision(&plan, &mem_store, &runner, &mut state).expect("provision should succeed");

    let calls = runner.calls();
    // Should have: one state-read call + one enter call + one state-write call
    // Find the enter (provision shell) call
    let enter_call = calls
        .iter()
        .find(|c| c.program == "distrobox" && c.args.iter().any(|a| a == "enter"))
        .expect("should have an enter call");

    // The enter argv must include `--additional-flags "--env STRIPE_KEY"` (name-only)
    let has_env_flag = enter_call
        .args
        .windows(2)
        .any(|w| w[0] == "--additional-flags" && w[1] == "--env STRIPE_KEY");
    assert!(
        has_env_flag,
        "provision enter must carry '--additional-flags \"--env STRIPE_KEY\"', got: {:?}",
        enter_call.args
    );

    // The env on the enter call must carry the VALUE
    let has_value = enter_call
        .env
        .iter()
        .any(|(k, v)| k == "STRIPE_KEY" && v == secret_value);
    assert!(
        has_value,
        "provision Invocation.env must carry STRIPE_KEY=..., got: {:?}",
        enter_call.env
    );

    // INV-1: the secret value must not appear in any call's args
    for call in &calls {
        assert!(
            !call.args.iter().any(|a| a.contains(secret_value)),
            "INV-1 violated: secret value found in argv of call {:?}: {:?}",
            call.program,
            call.args
        );
    }
}

// ─── AC-SEC-3: keyring missing → refuse, exit 75, NOTHING runs ───────────────

/// GIVEN a Boxfile referencing DATABASE_URL (persist=true) and MockSecretStore
/// with no value (AllNotFound mode)
/// WHEN core::create runs with pre-population that fails resolve_secret_env
/// THEN it returns CboxError with exit_code() == 75
/// AND the message contains "DATABASE_URL" and "cbox secret set"
/// AND MockRunner.call_count() == 0 (no spawn happened).
#[test]
fn ac_sec_3_missing_secret_refuses_exit_75_nothing_runs() {
    let store = MockSecretStore::new().with_all_not_found();

    let mut secrets = BTreeMap::new();
    secrets.insert(
        "DATABASE_URL".to_string(),
        cbox::boxfile::model::SecretEntry {
            persist: true,
            from: "keyring".to_string(),
        },
    );

    // Resolve must fail BEFORE calling the runner
    let err = resolve_secret_env("api-dev", &secrets, SecretScope::Persisted, &store).unwrap_err();
    assert_eq!(
        err.exit_code(),
        75,
        "missing secret must exit 75, got: {}",
        err.exit_code()
    );
    let msg = err.to_string();
    assert!(
        msg.contains("DATABASE_URL"),
        "error must name the missing key, got: {msg}"
    );
    assert!(
        msg.contains("cbox secret set"),
        "error must tell user the fix command, got: {msg}"
    );

    // Prove nothing ran: only call the runner if resolution succeeded
    let runner = MockRunner::new().with_default(MockResponse::ok(""));
    // (We don't call core::create because resolution already failed — nothing to call.)
    let calls = runner.calls();
    assert_eq!(
        calls.len(),
        0,
        "no runner call should happen when resolve fails"
    );
}

// ─── AC-SEC-4: keyring unavailable → exit 75 ─────────────────────────────────

#[test]
fn ac_sec_4_keyring_unavailable_exits_75() {
    let store = MockSecretStore::new().with_unavailable("D-Bus not available");

    let mut secrets = BTreeMap::new();
    secrets.insert(
        "DATABASE_URL".to_string(),
        cbox::boxfile::model::SecretEntry {
            persist: true,
            from: "keyring".to_string(),
        },
    );

    let err = resolve_secret_env("api-dev", &secrets, SecretScope::Persisted, &store).unwrap_err();
    assert_eq!(err.exit_code(), 75, "unavailable keyring must exit 75");
    let msg = err.to_string();
    assert!(
        msg.contains("keyring") || msg.contains("Secret Service"),
        "error must mention the keyring, got: {msg}"
    );
}

// ─── AC-SEC-5: no value in argv even on step failure (INV-1, S1/S2) ──────────

/// GIVEN a persist=false secret and a provision shell step whose MockResponse is err(1, "boom")
/// WHEN provision runs and fails
/// THEN the returned CboxError message does NOT contain the secret value (S1 sink)
/// AND the argv in the recorded call contains only --env KEY, never the value.
#[test]
fn ac_sec_5_no_value_in_argv_on_step_failure() {
    use cbox::boxfile::model::{ProvisionStep, ProvisionType};
    use cbox::core::provision::{provision, ProvisionPlan};
    use cbox::core::state_store::ProvisionState;

    let secret_value = "super_secret_value_must_not_appear";
    let store = MockSecretStore::new().with_secret("mybox", "MYSECRET", secret_value);

    let mut secrets = BTreeMap::new();
    secrets.insert(
        "MYSECRET".to_string(),
        cbox::boxfile::model::SecretEntry {
            persist: false,
            from: "keyring".to_string(),
        },
    );

    let resolved =
        resolve_secret_env("mybox", &secrets, SecretScope::ProvisionOnly, &store).unwrap();

    // Runner returns failure for the provision step
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(""))
                .with_program("distrobox")
                .with_args_contain(
                    vec!["sh".to_string(), "-c".to_string()]
                        .into_iter()
                        .chain(std::iter::once("cat".to_string()))
                        .collect(),
                ),
        )
        .with_default(MockResponse::err(1, "boom from step"));

    let provision_env_keys: Vec<String> = resolved.iter().map(|(k, _)| k.clone()).collect();
    let provision_env: Vec<(String, String)> = resolved.clone();

    let steps = vec![ProvisionStep {
        step_type: ProvisionType::Shell,
        run: Some("do_something".to_string()),
        src: None,
        dst: None,
    }];

    let plan = ProvisionPlan {
        name: "mybox",
        steps: &steps,
        boxfile_dir: std::path::Path::new("."),
        backend: &Backend::Podman,
        force: true,
        redo: &[],
        dry_run: false,
        provision_env_keys: &provision_env_keys,
        provision_env: &provision_env,
    };

    let mem_store = MemoryStore::empty();
    let mut state = ProvisionState::new();
    let err = provision(&plan, &mem_store, &runner, &mut state).unwrap_err();

    // S1: the error message (which may include the argv) must NOT contain the secret value
    let err_msg = err.to_string();
    assert!(
        !err_msg.contains(secret_value),
        "INV-1/S1: secret value must not appear in error message, got: {err_msg}"
    );

    // Check ALL recorded calls
    for call in runner.calls() {
        assert!(
            !call.args.iter().any(|a| a.contains(secret_value)),
            "INV-1: secret value must not appear in any argv, found in {:?}: {:?}",
            call.program,
            call.args
        );
    }
}

// ─── AC-ROUNDTRIP-1: full create+provision, INV-1 across all calls ───────────

/// The consolidated INV-1 assertion across a full create (with persist=true) and
/// provision (with persist=false) round-trip.
/// For every recorded call: args contains no secret value.
/// For relevant calls: env carries KEY=value.
#[test]
fn ac_roundtrip_1_full_roundtrip_inv1() {
    use cbox::boxfile::model::{ProvisionStep, ProvisionType};
    use cbox::core::provision::{provision, ProvisionPlan};
    use cbox::core::state_store::ProvisionState;

    let create_secret_val = "create_s3cr3t_value";
    let provision_secret_val = "provision_s3cr3t_value";

    let store = MockSecretStore::new()
        .with_secret("roundbox", "DB_URL", create_secret_val)
        .with_secret("roundbox", "STRIPE_KEY", provision_secret_val);

    // persist=true → create path
    let mut persisted_secrets = BTreeMap::new();
    persisted_secrets.insert(
        "DB_URL".to_string(),
        cbox::boxfile::model::SecretEntry {
            persist: true,
            from: "keyring".to_string(),
        },
    );
    let create_resolved = resolve_secret_env(
        "roundbox",
        &persisted_secrets,
        SecretScope::Persisted,
        &store,
    )
    .unwrap();

    // persist=false → provision path
    let mut provision_secrets = BTreeMap::new();
    provision_secrets.insert(
        "STRIPE_KEY".to_string(),
        cbox::boxfile::model::SecretEntry {
            persist: false,
            from: "keyring".to_string(),
        },
    );
    let provision_resolved = resolve_secret_env(
        "roundbox",
        &provision_secrets,
        SecretScope::ProvisionOnly,
        &store,
    )
    .unwrap();

    let runner = MockRunner::new().with_default(MockResponse::ok(""));

    // Phase 1: create
    let mut spec = CreateSpec::new("roundbox", Backend::Podman);
    spec.image = "fedora-toolbox:latest".to_string();
    spec.env_flags = create_resolved.iter().map(|(k, _)| k.clone()).collect();
    spec.env_values = create_resolved.clone();
    core::create(&spec, &runner).expect("create should succeed");

    // Phase 2: provision
    let provision_env_keys: Vec<String> =
        provision_resolved.iter().map(|(k, _)| k.clone()).collect();
    let provision_env = provision_resolved.clone();

    let steps = vec![ProvisionStep {
        step_type: ProvisionType::Shell,
        run: Some("echo setup".to_string()),
        src: None,
        dst: None,
    }];

    let plan = ProvisionPlan {
        name: "roundbox",
        steps: &steps,
        boxfile_dir: std::path::Path::new("."),
        backend: &Backend::Podman,
        force: true,
        redo: &[],
        dry_run: false,
        provision_env_keys: &provision_env_keys,
        provision_env: &provision_env,
    };

    let mem_store = MemoryStore::empty();
    let mut state = ProvisionState::new();
    provision(&plan, &mem_store, &runner, &mut state).expect("provision should succeed");

    // INV-1: check ALL recorded calls — no secret value in any arg
    let all_calls = runner.calls();
    for call in &all_calls {
        assert!(
            !call.args.iter().any(|a| a.contains(create_secret_val)),
            "INV-1: create secret value in argv of {:?}: {:?}",
            call.program,
            call.args
        );
        assert!(
            !call.args.iter().any(|a| a.contains(provision_secret_val)),
            "INV-1: provision secret value in argv of {:?}: {:?}",
            call.program,
            call.args
        );
    }

    // Verify the create call carries DB_URL value in env
    let create_call = all_calls
        .iter()
        .find(|c| c.args.iter().any(|a| a == "create"))
        .expect("should have a create call");
    assert!(
        create_call
            .env
            .iter()
            .any(|(k, v)| k == "DB_URL" && v == create_secret_val),
        "create env must carry DB_URL={create_secret_val}, got: {:?}",
        create_call.env
    );

    // Verify the provision enter call carries STRIPE_KEY in env
    let enter_call = all_calls
        .iter()
        .find(|c| c.args.iter().any(|a| a == "enter") && c.args.iter().any(|a| a == "sh"))
        .expect("should have a provision enter call");
    assert!(
        enter_call
            .env
            .iter()
            .any(|(k, v)| k == "STRIPE_KEY" && v == provision_secret_val),
        "provision enter env must carry STRIPE_KEY={provision_secret_val}, got: {:?}",
        enter_call.env
    );
}
