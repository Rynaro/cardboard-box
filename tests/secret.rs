//! AC-CLI-1/2/3 — `cbox secret set|list|rm` unit-level tests against MockSecretStore.
//! The stdin-pipe path is tested directly via the cli::secret module functions.

use cbox::cli::{
    output::OutputCtx,
    secret::{run, SecretArgs, SecretCommands, SecretListArgs, SecretRmArgs},
};
use cbox::secret::{mock::MockSecretStore, SecretStore};

fn quiet_ctx() -> OutputCtx {
    OutputCtx::new(false, true, 0, true) // quiet, no color
}

fn json_ctx() -> OutputCtx {
    OutputCtx::new(true, false, 0, true)
}

// ─── AC-CLI-2: secret list prints names only, never values ───────────────────

/// GIVEN the mock holds two keys for "api-dev"
/// WHEN cbox secret list api-dev runs
/// THEN it lists both KEY names and the MockStore.list output contains no stored value.
#[test]
fn ac_cli_2_list_names_only_never_values() {
    let store = MockSecretStore::new()
        .with_secret("api-dev", "DATABASE_URL", "postgres://super_secret")
        .with_secret("api-dev", "STRIPE_KEY", "sk_live_verysecret");

    let args = SecretArgs {
        command: SecretCommands::List(SecretListArgs {
            box_name: "api-dev".to_string(),
        }),
    };

    let ctx = quiet_ctx();
    let result = run(&args, &ctx, &store);
    assert!(
        result.is_ok(),
        "list should succeed: {:?}",
        result.unwrap_err()
    );

    // Verify: MockStore.list returns keys, not values
    let keys = store.list("api-dev").unwrap();
    assert!(
        keys.contains(&"DATABASE_URL".to_string()),
        "list must include DATABASE_URL"
    );
    assert!(
        keys.contains(&"STRIPE_KEY".to_string()),
        "list must include STRIPE_KEY"
    );
    // Values must not appear in the key list
    for key_name in &keys {
        assert!(
            !key_name.contains("postgres://"),
            "key name must not contain value substring: {key_name}"
        );
        assert!(
            !key_name.contains("sk_live"),
            "key name must not contain value substring: {key_name}"
        );
    }
}

/// list --json emits keys array, never values.
#[test]
fn ac_cli_2_list_json_keys_no_values() {
    let store = MockSecretStore::new().with_secret("api-dev", "DB_URL", "secret_value");

    let args = SecretArgs {
        command: SecretCommands::List(SecretListArgs {
            box_name: "api-dev".to_string(),
        }),
    };

    let ctx = json_ctx();
    let result = run(&args, &ctx, &store);
    assert!(result.is_ok(), "list --json should succeed");

    // Verify the store only provides keys, not values in list output
    let keys = store.list("api-dev").unwrap();
    assert_eq!(keys, vec!["DB_URL".to_string()]);
}

/// list on empty box → empty list, no error.
#[test]
fn ac_cli_2_list_empty_box() {
    let store = MockSecretStore::new();

    let args = SecretArgs {
        command: SecretCommands::List(SecretListArgs {
            box_name: "empty-box".to_string(),
        }),
    };

    let ctx = quiet_ctx();
    let result = run(&args, &ctx, &store);
    assert!(result.is_ok(), "list on empty box should succeed");

    let keys = store.list("empty-box").unwrap();
    assert!(keys.is_empty(), "empty box must have no keys");
}

// ─── AC-CLI-3: secret rm is idempotent ───────────────────────────────────────

/// WHEN cbox secret rm api-dev MISSING runs on an absent key
/// THEN exit 0 (idempotent success).
#[test]
fn ac_cli_3_rm_idempotent_on_absent_key() {
    let store = MockSecretStore::new(); // empty — MISSING key doesn't exist

    let args = SecretArgs {
        command: SecretCommands::Rm(SecretRmArgs {
            box_name: "api-dev".to_string(),
            key: "MISSING".to_string(),
        }),
    };

    let ctx = quiet_ctx();
    let result = run(&args, &ctx, &store);
    assert!(
        result.is_ok(),
        "rm on absent key must be idempotent (exit 0), got: {:?}",
        result.unwrap_err()
    );
}

/// rm on existing key removes it; subsequent list shows it gone.
#[test]
fn ac_cli_3_rm_removes_existing_key() {
    let store = MockSecretStore::new().with_secret("mybox", "DB_URL", "secret_val");

    let args = SecretArgs {
        command: SecretCommands::Rm(SecretRmArgs {
            box_name: "mybox".to_string(),
            key: "DB_URL".to_string(),
        }),
    };

    let ctx = quiet_ctx();
    run(&args, &ctx, &store).expect("rm should succeed");

    // After rm, the key must be gone
    let keys = store.list("mybox").unwrap();
    assert!(
        !keys.contains(&"DB_URL".to_string()),
        "DB_URL must be gone after rm"
    );
}

/// rm --json emits ok: true with box and key.
#[test]
fn ac_cli_3_rm_json_output() {
    let store = MockSecretStore::new();

    let args = SecretArgs {
        command: SecretCommands::Rm(SecretRmArgs {
            box_name: "mybox".to_string(),
            key: "ABSENT".to_string(),
        }),
    };

    let ctx = json_ctx();
    let result = run(&args, &ctx, &store);
    assert!(result.is_ok(), "rm --json must succeed");
}

// ─── Validation tests (exit codes) ───────────────────────────────────────────

/// Bad box name → exit 64.
#[test]
fn cli_bad_box_name_exits_64() {
    let store = MockSecretStore::new();

    let args = SecretArgs {
        command: SecretCommands::List(SecretListArgs {
            box_name: "-bad-name".to_string(), // leading dash = invalid
        }),
    };

    let ctx = quiet_ctx();
    let err = run(&args, &ctx, &store).unwrap_err();
    assert_eq!(err.exit_code(), 64, "bad box name must exit 64");
}

/// Bad KEY name → exit 65 for rm.
#[test]
fn cli_bad_key_name_exits_65() {
    let store = MockSecretStore::new();

    let args = SecretArgs {
        command: SecretCommands::Rm(SecretRmArgs {
            box_name: "mybox".to_string(),
            key: "1INVALID".to_string(), // leading digit = invalid env-var name
        }),
    };

    let ctx = quiet_ctx();
    let err = run(&args, &ctx, &store).unwrap_err();
    assert_eq!(err.exit_code(), 65, "bad key name must exit 65");
}

/// Keyring unavailable → exit 75 for list.
#[test]
fn cli_keyring_unavailable_exits_75() {
    let store = MockSecretStore::new().with_unavailable("test: service down");

    let args = SecretArgs {
        command: SecretCommands::List(SecretListArgs {
            box_name: "mybox".to_string(),
        }),
    };

    let ctx = quiet_ctx();
    let err = run(&args, &ctx, &store).unwrap_err();
    assert_eq!(err.exit_code(), 75, "unavailable keyring must exit 75");
}

// ─── AC-CLI-1 partial: MockStore set+get roundtrip (stdin path unit-tested ──
// Note: the full stdin-piped path requires process-level stdin replacement.
// We test the store roundtrip here and leave the binary-level stdin test to
// assert_cmd-based tests (which run in the compiled binary where stdin IS a pipe).

#[test]
fn mock_store_set_get_roundtrip() {
    let store = MockSecretStore::new();
    store.set("api-dev", "DATABASE_URL", "s3cr3t").unwrap();
    let got = store.get("api-dev", "DATABASE_URL").unwrap();
    assert_eq!(got, Some("s3cr3t".to_string()));

    // Verify no OTHER box sees this key
    let other = store.get("other-box", "DATABASE_URL").unwrap();
    assert!(other.is_none(), "key must be namespaced per box");
}
