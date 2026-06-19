//! AC-SEC-6 — validation rules V1–V6 for [secrets] and [env].

use cbox::boxfile;

// ─── V1: invalid env-var name rejected ───────────────────────────────────────

#[test]
fn v1_leading_digit_in_secrets_key_is_exit_65() {
    let toml = r#"
name  = "mybox"
image = "fedora-toolbox:latest"

[secrets]
"1BAD" = { persist = true }
"#;
    let err = boxfile::parse_and_validate(toml).unwrap_err();
    assert_eq!(err.exit_code(), 65, "V1: leading digit must be exit 65");
    assert!(
        err.to_string().contains("1BAD"),
        "error must name the bad key, got: {err}"
    );
}

#[test]
fn v1_dash_in_env_key_is_exit_65() {
    let toml = r#"
name  = "mybox"
image = "fedora-toolbox:latest"

[env]
"has-dash" = "value"
"#;
    let err = boxfile::parse_and_validate(toml).unwrap_err();
    assert_eq!(err.exit_code(), 65, "V1: dash in env key must be exit 65");
    assert!(
        err.to_string().contains("has-dash"),
        "error must name the bad key"
    );
}

#[test]
fn v1_valid_keys_pass() {
    let toml = r#"
name  = "mybox"
image = "fedora-toolbox:latest"

[secrets]
DATABASE_URL = {}
STRIPE_KEY   = { persist = false }

[env]
EDITOR = "nvim"
LANG   = "en_US.UTF-8"
_PRIVATE = "x"
"#;
    // Should parse and validate without error.
    boxfile::parse_and_validate(toml).expect("valid keys should pass");
}

// ─── V2: duplicate key across [env] and [secrets] ────────────────────────────

#[test]
fn v2_duplicate_key_across_tables_is_exit_65() {
    let toml = r#"
name  = "mybox"
image = "fedora-toolbox:latest"

[secrets]
DATABASE_URL = { persist = true }

[env]
DATABASE_URL = "postgres://localhost"
"#;
    let err = boxfile::parse_and_validate(toml).unwrap_err();
    assert_eq!(
        err.exit_code(),
        65,
        "V2: cross-table duplicate must be exit 65"
    );
    assert!(
        err.to_string().contains("DATABASE_URL"),
        "error must name the duplicate key"
    );
}

// ─── V3: from = "prompt" rejected ────────────────────────────────────────────

#[test]
fn v3_from_prompt_is_exit_65() {
    let toml = r#"
name  = "mybox"
image = "fedora-toolbox:latest"

[secrets]
API_KEY = { from = "prompt" }
"#;
    let err = boxfile::parse_and_validate(toml).unwrap_err();
    assert_eq!(err.exit_code(), 65, "V3: from=prompt must be exit 65");
    let msg = err.to_string();
    assert!(
        msg.contains("prompt") || msg.contains("API_KEY"),
        "error must mention the bad from value or key, got: {msg}"
    );
}

// ─── V4: count bound ─────────────────────────────────────────────────────────

#[test]
fn v4_too_many_entries_is_exit_65() {
    // Generate 65 entries (one over the 64 limit).
    let mut toml = String::from("name = \"mybox\"\nimage = \"fedora-toolbox:latest\"\n\n[env]\n");
    for i in 0..65 {
        toml.push_str(&format!("KEY_{i} = \"val\"\n"));
    }
    let err = boxfile::parse_and_validate(&toml).unwrap_err();
    assert_eq!(err.exit_code(), 65, "V4: >64 entries must be exit 65");
    let msg = err.to_string();
    assert!(
        msg.contains("64") || msg.contains("limit"),
        "error must mention the limit, got: {msg}"
    );
}

// ─── V5: key length bound ────────────────────────────────────────────────────

#[test]
fn v5_key_too_long_is_exit_65() {
    // 129-char key (one over 128 limit). Must be a valid env-var-name prefix (all A's).
    let long_key = format!("A{}", "A".repeat(128)); // 129 chars total
    let toml = format!(
        "name = \"mybox\"\nimage = \"fedora-toolbox:latest\"\n\n[env]\n{long_key} = \"val\"\n"
    );
    let err = boxfile::parse_and_validate(&toml).unwrap_err();
    assert_eq!(err.exit_code(), 65, "V5: key >128 chars must be exit 65");
    let msg = err.to_string();
    assert!(
        msg.contains("128") || msg.contains("long") || msg.contains(&long_key[..20]),
        "error must mention the length limit, got: {msg}"
    );
}

// ─── V6: env value length bound ──────────────────────────────────────────────

#[test]
fn v6_env_value_too_long_is_exit_65() {
    // 4097-byte value (one over 4096 limit).
    let long_val = "x".repeat(4097);
    let toml = format!(
        "name = \"mybox\"\nimage = \"fedora-toolbox:latest\"\n\n[env]\nTOO_LONG = \"{long_val}\"\n"
    );
    let err = boxfile::parse_and_validate(&toml).unwrap_err();
    assert_eq!(err.exit_code(), 65, "V6: value >4096 bytes must be exit 65");
    let msg = err.to_string();
    assert!(
        msg.contains("4096") || msg.contains("long"),
        "error must mention the length limit, got: {msg}"
    );
}
