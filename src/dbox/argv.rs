//! Pure argv builders — no I/O, no runner. Tested independently (AC-ARGV-1).
//! Every distrobox/backend command is built here. Flag mapping lives in one place.

use crate::boxfile::distro_family::detect_family;
use crate::boxfile::docker_mode::docker_mode_flags;
use crate::core::spec::{CreateSpec, DockerMode, EnterSpec, RmSpec, StopSpec};

/// Build the argv for `distrobox create` (not including the program name itself).
/// Pure function: given a `CreateSpec`, returns a deterministic `Vec<String>`.
pub fn build_create_argv(spec: &CreateSpec) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "create".into(),
        "--yes".into(),
        "--name".into(),
        spec.name.clone(),
        "--image".into(),
        spec.image.clone(),
    ];

    // Packages from the base spec + any docker-mode injected packages
    let mut all_packages = spec.packages.clone();
    let family = detect_family(&spec.image);
    let mode_flags = docker_mode_flags(&spec.docker_mode, &spec.backend, spec.uid, &family);
    all_packages.extend(mode_flags.extra_packages.iter().cloned());

    if !all_packages.is_empty() {
        args.push("--additional-packages".into());
        args.push(all_packages.join(" "));
    }

    // Mounts from spec
    for m in &spec.mounts {
        args.push("--volume".into());
        args.push(format!("{}:{}:{}", m.host, m.guest, m.mode));
    }

    // Docker-mode socket mount
    if let Some(sock_mount) = &mode_flags.socket_volume {
        args.push("--volume".into());
        args.push(sock_mount.clone());
    }

    // Docker-mode extra env flags (docker host)
    for extra in &mode_flags.extra_flags {
        args.push("--additional-flags".into());
        args.push(extra.clone());
    }

    // Optional fields
    if let Some(ref home) = spec.home {
        if !home.is_empty() {
            args.push("--home".into());
            args.push(home.clone());
        }
    }
    if let Some(ref hostname) = spec.hostname {
        if !hostname.is_empty() {
            args.push("--hostname".into());
            args.push(hostname.clone());
        }
    }

    // init: from spec OR docker=nested forces it
    if spec.init || mode_flags.force_init {
        args.push("--init".into());
    }

    if spec.pull {
        args.push("--pull".into());
    }
    if spec.root {
        args.push("--root".into());
    }

    // sandbox unshare flags (only meaningful for docker=none)
    if let Some(ref unshare) = spec.unshare {
        match unshare.as_str() {
            "all" => args.push("--unshare-all".into()),
            _ => {
                // space or comma-separated list
                for item in unshare.split_whitespace().chain(unshare.split(',')) {
                    let item = item.trim();
                    if !item.is_empty() && item != "all" {
                        args.push(format!("--unshare-{item}"));
                    }
                }
            }
        }
    }

    // ─── secret env flags (persist=true) — name-only, value rides Invocation.env ──
    // These must appear BEFORE the cbox label block (which is always the final token).
    for key in &spec.env_flags {
        args.push("--additional-flags".into());
        args.push(format!("--env {key}"));
    }

    // ─── plaintext [env] flags — value inline in argv (non-secret) ──────────────
    for (key, value) in &spec.plain_env {
        args.push("--additional-flags".into());
        args.push(format!("--env {key}={value}"));
    }

    // cbox labels (§5.5 + §4.4 cbox.image added in P2)
    let docker_label = match &spec.docker_mode {
        DockerMode::None => "none",
        DockerMode::Host => "host",
        DockerMode::Nested => "nested",
    };
    args.push("--additional-flags".into());
    args.push(format!("--label cbox.managed=true --label cbox.docker_mode={docker_label} --label cbox.boxfile_path={} --label cbox.version={} --label cbox.image={}",
        spec.boxfile_path.as_deref().unwrap_or(""),
        env!("CARGO_PKG_VERSION"),
        spec.image,
    ));

    args
}

/// Build the argv for `distrobox rm`.
pub fn build_rm_argv(spec: &RmSpec) -> Vec<String> {
    let mut args = vec!["rm".to_string()];
    if spec.force {
        args.push("--force".into());
    }
    if spec.rm_home {
        args.push("--rm-home".into());
    }
    if spec.all {
        args.push("--all".into());
    }
    for name in &spec.names {
        args.push(name.clone());
    }
    args
}

/// Build the argv for `distrobox stop`.
pub fn build_stop_argv(spec: &StopSpec) -> Vec<String> {
    let mut args = vec!["stop".to_string(), "--yes".to_string()];
    if spec.all {
        args.push("--all".into());
    }
    for name in &spec.names {
        args.push(name.clone());
    }
    args
}

/// Build the argv for `distrobox enter`.
pub fn build_enter_argv(spec: &EnterSpec) -> Vec<String> {
    let mut args = vec!["enter".to_string(), "--name".to_string(), spec.name.clone()];
    if spec.root {
        args.push("--root".into());
    }
    if spec.clean_path {
        args.push("--clean-path".into());
    }
    if !spec.cmd.is_empty() {
        args.push("--".into());
        args.extend(spec.cmd.iter().cloned());
    }
    args
}

/// Build the argv for `<backend> ps` (list machine path).
pub fn build_list_argv() -> Vec<String> {
    vec![
        "ps".to_string(),
        "-a".to_string(),
        "--filter".to_string(),
        "label=manager=distrobox".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ]
}

/// Build the argv for `<backend> inspect <name> --format json`.
pub fn build_inspect_argv(name: &str) -> Vec<String> {
    vec![
        "inspect".to_string(),
        name.to_string(),
        "--format".to_string(),
        "json".to_string(),
    ]
}

/// Build the argv for `distrobox list` (human path — used by list_human).
#[allow(dead_code)]
pub fn build_dbox_list_argv() -> Vec<String> {
    vec!["list".to_string()]
}

/// Build the argv for `<backend> logs -f <id>`.
/// This is an ENGINE call (podman/docker), not a distrobox subcommand.
/// Used for the live log streaming pane (Bundle 3 S5).
pub fn build_logs_argv(id: &str) -> Vec<String> {
    vec!["logs".to_string(), "-f".to_string(), id.to_string()]
}

/// Build the argv for `<backend> stats <id> --no-stream --format json`.
/// This is an ENGINE call (podman/docker), not a distrobox subcommand.
#[allow(dead_code)]
pub fn build_stats_argv(id: &str) -> Vec<String> {
    vec![
        "stats".to_string(),
        id.to_string(),
        "--no-stream".to_string(),
        "--format".to_string(),
        "json".to_string(),
    ]
}

// ─── P2 argv builders ────────────────────────────────────────────────────────

/// Build the argv for a provision shell step:
/// `distrobox enter --name <NAME> -- sh -c "<run>"`
/// The `run` string is passed as a single `sh -c` argument; the guest shell parses it.
pub fn build_provision_shell_argv(name: &str, run: &str) -> Vec<String> {
    vec![
        "enter".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--".to_string(),
        "sh".to_string(),
        "-c".to_string(),
        run.to_string(),
    ]
}

/// Build the argv for a provision shell step with env injection (name-only --env KEY flags).
/// `env_keys` are the KEY names for persist=false secrets; values ride Invocation.env.
///
/// distrobox enter --name <N> --additional-flags "--env K1" … -- sh -c "<run>"
pub fn build_provision_shell_argv_with_env(
    name: &str,
    run: &str,
    env_keys: &[String],
) -> Vec<String> {
    let mut args = vec!["enter".to_string(), "--name".to_string(), name.to_string()];
    for key in env_keys {
        args.push("--additional-flags".into());
        args.push(format!("--env {key}"));
    }
    args.push("--".to_string());
    args.push("sh".to_string());
    args.push("-c".to_string());
    args.push(run.to_string());
    args
}

/// Build the argv for `<backend> cp <host_src> <name>:<guest_dst>`.
/// program is the backend binary (podman/docker); these args are passed to `runner.run`.
pub fn build_copy_argv(name: &str, src: &str, dst: &str) -> Vec<String> {
    vec!["cp".to_string(), src.to_string(), format!("{name}:{dst}")]
}

/// Build the argv to read the guest provision state file.
/// `distrobox enter --name <NAME> -- sh -c 'cat "$STATE" 2>/dev/null || echo ""'`
pub fn build_state_read_argv(name: &str) -> Vec<String> {
    // Use a fixed state path constant; the guest resolves $HOME.
    // We use an explicit path rather than env var expansion for predictability in tests.
    let cmd = "cat \"${XDG_STATE_HOME:-$HOME/.local/state}/cbox/provision.json\" 2>/dev/null || echo \"\"";
    vec![
        "enter".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--".to_string(),
        "sh".to_string(),
        "-c".to_string(),
        cmd.to_string(),
    ]
}

/// Build the argv to write the guest provision state file.
/// The JSON is single-quote escaped and embedded in a `printf '%s'` shell command.
/// `distrobox enter --name <NAME> -- sh -c 'mkdir -p "$DIR"; printf %s '<json>' > "$STATE"'`
///
/// Single-quote escaping: replace `'` with `'\''` so the JSON is safe inside `sh -c '...'`.
pub fn build_state_write_argv(name: &str, json: &str) -> Vec<String> {
    let escaped = escape_single_quotes(json);
    let state_path = "${XDG_STATE_HOME:-$HOME/.local/state}/cbox/provision.json";
    let state_dir = "${XDG_STATE_HOME:-$HOME/.local/state}/cbox";
    let cmd = format!("mkdir -p \"{state_dir}\" && printf '%s' '{escaped}' > \"{state_path}\"");
    vec![
        "enter".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--".to_string(),
        "sh".to_string(),
        "-c".to_string(),
        cmd,
    ]
}

/// Single-quote escape: replace every `'` with `'\''`.
/// This makes arbitrary text safe to embed inside `sh -c '...'`.
pub fn escape_single_quotes(s: &str) -> String {
    s.replace('\'', "'\\''")
}

// ─── v6.0 export argv builders ───────────────────────────────────────────────

/// `distrobox enter --name <NAME> -- distrobox-export --app <APP> [--delete]`
pub fn build_export_app_argv(name: &str, app: &str, delete: bool) -> Vec<String> {
    let mut args = vec![
        "enter".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--".to_string(),
        "distrobox-export".to_string(),
        "--app".to_string(),
        app.to_string(),
    ];
    if delete {
        args.push("--delete".to_string());
    }
    args
}

/// `distrobox enter --name <NAME> -- distrobox-export --bin <PATH> [--export-path <DIR>] [--delete]`
/// `--export-path` is omitted when `to` is None; `--delete` may omit `--export-path` too.
pub fn build_export_bin_argv(name: &str, bin: &str, to: Option<&str>, delete: bool) -> Vec<String> {
    let mut args = vec![
        "enter".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--".to_string(),
        "distrobox-export".to_string(),
        "--bin".to_string(),
        bin.to_string(),
    ];
    if let Some(dir) = to {
        args.push("--export-path".to_string());
        args.push(dir.to_string());
    }
    if delete {
        args.push("--delete".to_string());
    }
    args
}

/// `distrobox enter --name <NAME> -- distrobox-export --service <NAME> [--delete]`
pub fn build_export_service_argv(name: &str, service: &str, delete: bool) -> Vec<String> {
    let mut args = vec![
        "enter".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--".to_string(),
        "distrobox-export".to_string(),
        "--service".to_string(),
        service.to_string(),
    ];
    if delete {
        args.push("--delete".to_string());
    }
    args
}

/// `distrobox enter --name <NAME> -- distrobox-export --list-apps`
pub fn build_export_list_apps_argv(name: &str) -> Vec<String> {
    vec![
        "enter".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--".to_string(),
        "distrobox-export".to_string(),
        "--list-apps".to_string(),
    ]
}

/// `distrobox enter --name <NAME> -- distrobox-export --list-binaries`
pub fn build_export_list_bins_argv(name: &str) -> Vec<String> {
    vec![
        "enter".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--".to_string(),
        "distrobox-export".to_string(),
        "--list-binaries".to_string(),
    ]
}

/// Build the argv to probe the distro package manager inside the box.
/// Returns the path of the first found package manager.
pub fn build_pkg_probe_argv(name: &str) -> Vec<String> {
    let cmd = "command -v dnf || command -v apt-get || command -v apk || echo unknown";
    vec![
        "enter".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--".to_string(),
        "sh".to_string(),
        "-c".to_string(),
        cmd.to_string(),
    ]
}

#[cfg(test)]
mod export_argv_tests {
    use super::*;

    #[test]
    fn export_app_argv_no_delete() {
        let args = build_export_app_argv("dev", "firefox", false);
        assert_eq!(
            args,
            &[
                "enter",
                "--name",
                "dev",
                "--",
                "distrobox-export",
                "--app",
                "firefox"
            ]
        );
    }

    #[test]
    fn export_app_argv_with_delete() {
        let args = build_export_app_argv("dev", "firefox", true);
        assert_eq!(
            args,
            &[
                "enter",
                "--name",
                "dev",
                "--",
                "distrobox-export",
                "--app",
                "firefox",
                "--delete"
            ]
        );
    }

    #[test]
    fn export_bin_argv_with_to() {
        let args = build_export_bin_argv("dev", "/usr/bin/htop", Some("/home/u/.local/bin"), false);
        assert_eq!(
            args,
            &[
                "enter",
                "--name",
                "dev",
                "--",
                "distrobox-export",
                "--bin",
                "/usr/bin/htop",
                "--export-path",
                "/home/u/.local/bin"
            ]
        );
    }

    #[test]
    fn export_bin_argv_without_to() {
        let args = build_export_bin_argv("dev", "/usr/bin/htop", None, false);
        assert_eq!(
            args,
            &[
                "enter",
                "--name",
                "dev",
                "--",
                "distrobox-export",
                "--bin",
                "/usr/bin/htop"
            ]
        );
        assert!(
            !args.iter().any(|a| a == "--export-path"),
            "must not contain --export-path when to is None"
        );
    }

    #[test]
    fn export_bin_argv_with_delete() {
        let args = build_export_bin_argv("dev", "/usr/bin/htop", Some("/home/u/.local/bin"), true);
        assert_eq!(
            args,
            &[
                "enter",
                "--name",
                "dev",
                "--",
                "distrobox-export",
                "--bin",
                "/usr/bin/htop",
                "--export-path",
                "/home/u/.local/bin",
                "--delete"
            ]
        );
    }

    #[test]
    fn export_service_argv_no_delete() {
        let args = build_export_service_argv("dev", "nginx", false);
        assert_eq!(
            args,
            &[
                "enter",
                "--name",
                "dev",
                "--",
                "distrobox-export",
                "--service",
                "nginx"
            ]
        );
    }

    #[test]
    fn export_service_argv_with_delete() {
        let args = build_export_service_argv("dev", "nginx", true);
        assert_eq!(
            args,
            &[
                "enter",
                "--name",
                "dev",
                "--",
                "distrobox-export",
                "--service",
                "nginx",
                "--delete"
            ]
        );
    }

    #[test]
    fn export_list_apps_argv() {
        let args = build_export_list_apps_argv("dev");
        assert_eq!(
            args,
            &[
                "enter",
                "--name",
                "dev",
                "--",
                "distrobox-export",
                "--list-apps"
            ]
        );
    }

    #[test]
    fn export_list_bins_argv() {
        let args = build_export_list_bins_argv("dev");
        assert_eq!(
            args,
            &[
                "enter",
                "--name",
                "dev",
                "--",
                "distrobox-export",
                "--list-binaries"
            ]
        );
    }
}
