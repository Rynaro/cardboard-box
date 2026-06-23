//! Tests for the "fully isolated box" expansion (FEATURE-2): private $HOME +
//! process/ipc namespaces, exposed via `[box] isolated` and `--isolated`.
//! These exercise the pure expansion (`core::apply_isolation`) and its effect on
//! the create argv; the CLI/Boxfile flag wiring is a thin call into these.

use cbox::core::spec::CreateSpec;
use cbox::core::{apply_isolation, isolated_home_under, synth_isolated_home};
use cbox::dbox::argv::build_create_argv;
use cbox::dbox::backend::Backend;

fn base_spec(name: &str) -> CreateSpec {
    let mut s = CreateSpec::new(name, Backend::Podman);
    s.image = "fedora-toolbox:latest".to_string();
    s
}

#[test]
fn isolated_home_under_is_pure() {
    assert_eq!(
        isolated_home_under("/home/u/.local/share", "web-dev"),
        "/home/u/.local/share/cbox/homes/web-dev"
    );
}

#[test]
fn synth_isolated_home_lands_under_data_dir() {
    // Env-independent: whatever the data dir resolves to, the suffix is stable.
    assert!(
        synth_isolated_home("web-dev").ends_with("/cbox/homes/web-dev"),
        "got: {}",
        synth_isolated_home("web-dev")
    );
}

// AC-4 / AC-5: isolated with no prior home → private home + process/ipc + init.
#[test]
fn apply_isolation_synthesizes_home_and_namespaces() {
    let mut spec = base_spec("web-dev");
    assert!(spec.home.is_none());
    apply_isolation(&mut spec, "web-dev");

    let home = spec.home.as_deref().expect("home should be synthesized");
    assert!(
        home.ends_with("/cbox/homes/web-dev"),
        "synthesized home should be the XDG private path, got: {home}"
    );
    let unshare = spec.unshare.as_deref().unwrap_or("");
    assert!(
        unshare.contains("ipc"),
        "should unshare ipc, got: {unshare}"
    );
    assert!(
        unshare.contains("process"),
        "should unshare process, got: {unshare}"
    );
    assert!(
        !unshare.contains("netns"),
        "default isolation must KEEP netns shared, got: {unshare}"
    );
    assert!(
        !spec.init,
        "isolation must NOT force --init (toolbox images ship no init system)"
    );
}

// AC-6: an explicit home wins over the synthesized one; hardening still applies.
#[test]
fn apply_isolation_respects_explicit_home() {
    let mut spec = base_spec("web-dev");
    spec.home = Some("/custom/home".to_string());
    apply_isolation(&mut spec, "web-dev");

    assert_eq!(
        spec.home.as_deref(),
        Some("/custom/home"),
        "explicit home must win"
    );
    let unshare = spec.unshare.as_deref().unwrap_or("");
    assert!(unshare.contains("ipc") && unshare.contains("process"));
}

#[test]
fn apply_isolation_is_idempotent() {
    let mut once = base_spec("web-dev");
    apply_isolation(&mut once, "web-dev");
    let mut twice = base_spec("web-dev");
    apply_isolation(&mut twice, "web-dev");
    apply_isolation(&mut twice, "web-dev");

    assert_eq!(once.home, twice.home);
    assert_eq!(once.unshare, twice.unshare);
    assert_eq!(once.init, twice.init);
}

#[test]
fn apply_isolation_merges_existing_unshare() {
    let mut spec = base_spec("web-dev");
    spec.unshare = Some("netns".to_string()); // user already asked for netns
    apply_isolation(&mut spec, "web-dev");

    let unshare = spec.unshare.as_deref().unwrap_or("");
    for ns in ["ipc", "netns", "process"] {
        assert!(
            unshare.contains(ns),
            "merged unshare should keep {ns}, got: {unshare}"
        );
    }
}

// The expansion lands in the create argv as concrete distrobox flags.
#[test]
fn isolated_spec_produces_expected_create_argv() {
    let mut spec = base_spec("web-dev");
    apply_isolation(&mut spec, "web-dev");
    let args = build_create_argv(&spec);

    let home_idx = args
        .iter()
        .position(|a| a == "--home")
        .expect("--home flag");
    assert!(args[home_idx + 1].ends_with("/cbox/homes/web-dev"));
    assert!(args.iter().any(|a| a == "--unshare-ipc"));
    assert!(args.iter().any(|a| a == "--unshare-process"));
    assert!(
        !args.iter().any(|a| a == "--init"),
        "isolation must not force --init (no init system in toolbox images)"
    );
    assert!(
        !args.iter().any(|a| a == "--unshare-netns"),
        "default isolation must not unshare netns"
    );
}
