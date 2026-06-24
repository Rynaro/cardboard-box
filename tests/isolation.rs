//! Tests for the "fully isolated box" expansion (FEATURE-2): private $HOME +
//! process/ipc namespaces, exposed via `[box] isolated` and `--isolated`.
//! These exercise the pure expansion (`core::apply_isolation`) and its effect on
//! the create argv; the CLI/Boxfile flag wiring is a thin call into these.

use cbox::core::provision::resolve_host_dst;
use cbox::core::spec::CreateSpec;
use cbox::core::{apply_isolation, isolated_home_under, private_box_home, synth_isolated_home};
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

// ─── resolve_host_dst (Change 3) ─────────────────────────────────────────────

const HOME: &str = "/home/rynaro/.local/share/cbox/homes/mybox";

#[test]
fn resolve_host_dst_tilde_exact() {
    assert_eq!(
        resolve_host_dst("~", HOME),
        Some(HOME.to_string()),
        "bare ~ maps to home"
    );
}

#[test]
fn resolve_host_dst_dollar_home_exact() {
    assert_eq!(resolve_host_dst("$HOME", HOME), Some(HOME.to_string()));
}

#[test]
fn resolve_host_dst_dollar_brace_home_exact() {
    assert_eq!(resolve_host_dst("${HOME}", HOME), Some(HOME.to_string()));
}

#[test]
fn resolve_host_dst_tilde_prefix() {
    assert_eq!(
        resolve_host_dst("~/.ssh/id_rsa", HOME),
        Some(format!("{HOME}/.ssh/id_rsa"))
    );
}

#[test]
fn resolve_host_dst_dollar_home_prefix() {
    assert_eq!(
        resolve_host_dst("$HOME/.config/git/config", HOME),
        Some(format!("{HOME}/.config/git/config"))
    );
}

#[test]
fn resolve_host_dst_dollar_brace_home_prefix() {
    assert_eq!(
        resolve_host_dst("${HOME}/.bashrc", HOME),
        Some(format!("{HOME}/.bashrc"))
    );
}

#[test]
fn resolve_host_dst_relative() {
    assert_eq!(resolve_host_dst("x/y", HOME), Some(format!("{HOME}/x/y")));
}

#[test]
fn resolve_host_dst_absolute_inside_home() {
    let inside = format!("{HOME}/.ssh/id_rsa");
    assert_eq!(
        resolve_host_dst(&inside, HOME),
        Some(inside.clone()),
        "absolute path inside home maps to itself"
    );
}

#[test]
fn resolve_host_dst_absolute_equal_home() {
    assert_eq!(
        resolve_host_dst(HOME, HOME),
        Some(HOME.to_string()),
        "absolute path == home maps to home"
    );
}

#[test]
fn resolve_host_dst_absolute_outside_home() {
    assert_eq!(
        resolve_host_dst("/etc/passwd", HOME),
        None,
        "absolute path outside home must return None"
    );
}

#[test]
fn resolve_host_dst_trailing_slash_on_home_is_stripped() {
    let home_with_slash = format!("{HOME}/");
    assert_eq!(
        resolve_host_dst("~/.vimrc", &home_with_slash),
        Some(format!("{HOME}/.vimrc")),
        "trailing slash on home must be stripped before joining"
    );
}

// ─── private_box_home (Change 3) ─────────────────────────────────────────────

#[test]
fn private_box_home_returns_none_when_matches_real_home() {
    let real = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    // When the candidate home IS the real HOME, it's not private.
    let result = private_box_home(Some(&real));
    assert!(
        result.is_none(),
        "home equal to $HOME must not be treated as private"
    );
}

#[test]
fn private_box_home_returns_some_for_private_path() {
    let private = "/home/rynaro/.local/share/cbox/homes/mybox";
    // As long as this differs from $HOME it should be returned.
    let real = std::env::var("HOME").unwrap_or_default();
    if private == real {
        // Degenerate env — skip.
        return;
    }
    assert_eq!(private_box_home(Some(private)), Some(private.to_string()));
}

#[test]
fn private_box_home_returns_none_for_empty() {
    assert!(private_box_home(Some("")).is_none());
    assert!(private_box_home(None).is_none());
}
