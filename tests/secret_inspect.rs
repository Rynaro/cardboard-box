//! AC-INSPECT-1/2 — inspect --raw masking and projected inspect safety.

use cbox::core::spec::InspectSpec;
use cbox::core::{inspect, mask_secret_keys_in_raw_json};
use cbox::dbox::{
    backend::Backend,
    mock::{MockMatcher, MockResponse, MockRunner},
};

// ─── AC-INSPECT-1: inspect --raw masks known secret keys ─────────────────────

/// GIVEN raw backend JSON whose Config.Env contains DATABASE_URL=postgres://s3cr3t
/// and a secret key list with DATABASE_URL
/// WHEN mask_secret_keys_in_raw_json runs
/// THEN the result shows DATABASE_URL=*** and does NOT contain postgres://s3cr3t.
#[test]
fn ac_inspect_1_raw_masks_known_secret_keys() {
    let secret_value = "postgres://s3cr3t";
    let raw_json = serde_json::json!([{
        "Id": "abc123",
        "Config": {
            "Image": "fedora-toolbox:latest",
            "Env": [
                "HOME=/home/user",
                format!("DATABASE_URL={secret_value}"),
                "EDITOR=nvim",
                "LANG=en_US.UTF-8",
            ]
        }
    }])
    .to_string();

    let secret_keys = vec!["DATABASE_URL".to_string()];
    let masked = mask_secret_keys_in_raw_json(&raw_json, &secret_keys).unwrap();

    // The secret VALUE must be masked
    assert!(
        !masked.contains(secret_value),
        "masked output must NOT contain the secret value; got: {masked}"
    );

    // The KEY must remain visible (just value replaced)
    assert!(
        masked.contains("DATABASE_URL=***"),
        "masked output must show DATABASE_URL=***; got: {masked}"
    );

    // Non-secret env should be untouched
    assert!(
        masked.contains("EDITOR=nvim"),
        "non-secret env must be preserved; got: {masked}"
    );
    assert!(
        masked.contains("HOME=/home/user"),
        "HOME must be preserved; got: {masked}"
    );
}

/// Multiple secrets in Config.Env — all should be masked.
#[test]
fn ac_inspect_1_masks_multiple_secret_keys() {
    let raw_json = serde_json::json!([{
        "Config": {
            "Env": [
                "DATABASE_URL=postgres://secret123",
                "STRIPE_KEY=sk_live_xyz",
                "PUBLIC_VAR=visible_value",
            ]
        }
    }])
    .to_string();

    let secret_keys = vec!["DATABASE_URL".to_string(), "STRIPE_KEY".to_string()];
    let masked = mask_secret_keys_in_raw_json(&raw_json, &secret_keys).unwrap();

    assert!(
        !masked.contains("postgres://secret123"),
        "DATABASE_URL value must be masked"
    );
    assert!(
        !masked.contains("sk_live_xyz"),
        "STRIPE_KEY value must be masked"
    );
    assert!(
        masked.contains("DATABASE_URL=***"),
        "DATABASE_URL must show ***"
    );
    assert!(
        masked.contains("STRIPE_KEY=***"),
        "STRIPE_KEY must show ***"
    );
    assert!(
        masked.contains("PUBLIC_VAR=visible_value"),
        "non-secret must be preserved"
    );
}

/// Empty secret_keys → no masking, output passes through unchanged.
#[test]
fn ac_inspect_1_no_masking_when_no_secret_keys() {
    let secret_value = "real_password";
    let raw_json = serde_json::json!([{
        "Config": {
            "Env": [format!("MYSECRET={secret_value}")]
        }
    }])
    .to_string();

    let masked = mask_secret_keys_in_raw_json(&raw_json, &[]).unwrap();

    // With no secret keys, value passes through untouched
    // (per spec: masking is only for KNOWN keys, not a general scrubber)
    assert!(
        masked.contains(secret_value),
        "without secret_keys, raw value must pass through unchanged"
    );
}

/// Malformed JSON → returns the original raw string, no panic.
#[test]
fn ac_inspect_1_graceful_on_malformed_json() {
    let raw = "not valid json {{";
    let result = mask_secret_keys_in_raw_json(raw, &["KEY".to_string()]).unwrap();
    assert_eq!(result, raw, "malformed JSON must be returned as-is");
}

// ─── AC-INSPECT-2: projected inspect JSON never contains secret values ────────

/// GIVEN a live container whose Config.Env contains a secret value
/// WHEN inspect() runs (projected output — InspectResult)
/// THEN InspectResult has no field that contains the secret value.
/// This ensures the projected path is safe by structural construction
/// (InspectResult has no env field — only HOME is extracted from Config.Env).
#[test]
fn ac_inspect_2_projected_inspect_never_contains_secret_value() {
    let secret_value = "super_secret_db_password_must_not_leak";

    let raw_inspect = serde_json::json!([{
        "Id": "abc123",
        "State": { "Status": "running" },
        "Config": {
            "Image": "fedora-toolbox:latest",
            "Labels": {
                "manager": "distrobox",
                "cbox.managed": "true",
                "cbox.docker_mode": "none",
                "cbox.boxfile_path": "",
                "cbox.version": "0.6.0",
                "cbox.image": "fedora-toolbox:latest",
                "cbox.packages": ""
            },
            "Env": [
                "HOME=/home/user",
                format!("DATABASE_URL={secret_value}"),
            ]
        },
        "Mounts": [],
        "Created": "2026-06-19T00:00:00Z",
        "Name": "mybox"
    }])
    .to_string();

    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok(raw_inspect))
                .with_program("podman")
                .with_args_contain(vec!["inspect".to_string()]),
        )
        .with_default(MockResponse::ok(""));

    let spec = InspectSpec {
        name: "mybox".to_string(),
        raw: false,
        backend: Backend::Podman,
    };

    let result = inspect(&spec, &runner).expect("inspect should succeed");

    // Serialize InspectResult to JSON and verify the secret value is not present
    let result_json = serde_json::to_string(&result).unwrap();
    assert!(
        !result_json.contains(secret_value),
        "projected InspectResult JSON must not contain the secret value, got: {result_json}"
    );

    // HOME is extracted from Config.Env and is expected in InspectResult
    assert_eq!(
        result.home.as_deref(),
        Some("/home/user"),
        "HOME must be extracted correctly"
    );
}
