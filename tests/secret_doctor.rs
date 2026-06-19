//! AC-DOCTOR-1 — keyring line is non-fatal in doctor output.
//! GIVEN MockSecretStore in Unavailable mode but distrobox+backend healthy
//! WHEN core::doctor runs
//! THEN result.ok is unchanged by keyring state, and keyring.available == false
//! with an actionable detail string; --json includes the keyring object.

use cbox::core::{self, spec::DoctorSpec};
use cbox::dbox::mock::{MockMatcher, MockResponse, MockRunner};
use cbox::secret::{SecretError, SecretStore};

// ─── test helpers ────────────────────────────────────────────────────────────

struct UnavailableStore;
impl SecretStore for UnavailableStore {
    fn set(&self, _: &str, _: &str, _: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable("test: service unavailable".into()))
    }
    fn get(&self, _: &str, _: &str) -> Result<Option<String>, SecretError> {
        Err(SecretError::Unavailable("test: service unavailable".into()))
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable("test: service unavailable".into()))
    }
    fn list(&self, _: &str) -> Result<Vec<String>, SecretError> {
        Err(SecretError::Unavailable("test: service unavailable".into()))
    }
}

struct AvailableStore;
impl SecretStore for AvailableStore {
    fn set(&self, _: &str, _: &str, _: &str) -> Result<(), SecretError> {
        Ok(())
    }
    fn get(&self, _: &str, _: &str) -> Result<Option<String>, SecretError> {
        Ok(None) // no entry — that's fine, just proves service is reachable
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), SecretError> {
        Ok(())
    }
    fn list(&self, _: &str) -> Result<Vec<String>, SecretError> {
        Ok(vec![])
    }
}

fn healthy_runner() -> MockRunner {
    MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::ok("distrobox: 1.8.2.4"))
                .with_program("distrobox")
                .with_args_contain(vec!["version".to_string()]),
        )
        .with_matcher(
            MockMatcher::new(MockResponse::ok("podman version 5.8.2"))
                .with_program("podman")
                .with_args_contain(vec!["--version".to_string()]),
        )
        .with_matcher(
            MockMatcher::new(MockResponse::ok("ok"))
                .with_program("podman")
                .with_args_contain(vec!["info".to_string()]),
        )
        .with_default(MockResponse::err(127, "not found"))
}

// ─── AC-DOCTOR-1: keyring unavailable → doctor.ok still true ─────────────────

#[test]
fn ac_doctor_1_keyring_unavailable_non_fatal() {
    let runner = healthy_runner();
    let spec = DoctorSpec {
        backend_override: None,
    };
    let store = UnavailableStore;

    let result = core::doctor(&spec, &runner, &store).expect("doctor should succeed");

    // distrobox+backend healthy → ok must be true regardless of keyring
    assert!(
        result.ok,
        "doctor.ok must be true when distrobox+backend are healthy, even with unavailable keyring"
    );

    // Keyring must report unavailable
    assert!(
        !result.keyring.available,
        "keyring.available must be false when store returns Unavailable"
    );

    // The detail string must be actionable
    assert!(
        !result.keyring.detail.is_empty(),
        "keyring.detail must contain an actionable message"
    );
}

/// When the keyring IS available (Ok(None) from probe → service reachable),
/// keyring.available must be true.
#[test]
fn ac_doctor_1_keyring_available_when_probe_succeeds() {
    let runner = healthy_runner();
    let spec = DoctorSpec {
        backend_override: None,
    };
    let store = AvailableStore;

    let result = core::doctor(&spec, &runner, &store).expect("doctor should succeed");

    assert!(result.ok, "doctor.ok must be true");
    assert!(
        result.keyring.available,
        "keyring.available must be true when probe returns Ok(None)"
    );
}

/// doctor.ok must be GATED ON distrobox+backend, NOT keyring.
/// When distrobox is absent AND keyring is unavailable → error (not a double-fault
/// where keyring unavailability is reported as a separate non-fatal).
#[test]
fn ac_doctor_1_ok_gated_on_distrobox_not_keyring() {
    let runner = MockRunner::new()
        .with_matcher(
            MockMatcher::new(MockResponse::err(127, "command not found: distrobox"))
                .with_program("distrobox")
                .with_args_contain(vec!["version".to_string()]),
        )
        .with_default(MockResponse::err(1, "not found"));

    let spec = DoctorSpec {
        backend_override: None,
    };
    let store = UnavailableStore;

    // Doctor should FAIL (exit 70) because distrobox is absent — not because of keyring
    let err = core::doctor(&spec, &runner, &store)
        .expect_err("doctor must fail when distrobox is absent");
    assert_eq!(
        err.exit_code(),
        70,
        "distrobox absent → exit 70 (software), got: {}",
        err.exit_code()
    );
}

/// doctor JSON output must include the keyring object with available + detail.
#[test]
fn ac_doctor_1_json_includes_keyring_object() {
    let runner = healthy_runner();
    let spec = DoctorSpec {
        backend_override: None,
    };
    let store = UnavailableStore;

    let result = core::doctor(&spec, &runner, &store).expect("doctor should succeed");

    // Serialize the keyring to verify the JSON shape
    let keyring_json = serde_json::to_value(&result.keyring).unwrap();
    assert!(
        keyring_json.get("available").is_some(),
        "keyring JSON must have 'available' field"
    );
    assert!(
        keyring_json.get("detail").is_some(),
        "keyring JSON must have 'detail' field"
    );
    assert_eq!(
        keyring_json["available"].as_bool(),
        Some(false),
        "available must be false for UnavailableStore"
    );
}
