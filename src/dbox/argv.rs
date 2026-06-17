//! Pure argv builders — no I/O, no runner. Tested independently (AC-ARGV-1).
//! Every distrobox/backend command is built here. Flag mapping lives in one place.

use crate::boxfile::docker_mode::docker_mode_flags;
use crate::core::spec::{CreateSpec, DockerMode, EnterSpec, RmSpec};

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
    let mode_flags = docker_mode_flags(&spec.docker_mode, &spec.backend, spec.uid);
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

    // cbox labels (§5.5)
    let docker_label = match &spec.docker_mode {
        DockerMode::None => "none",
        DockerMode::Host => "host",
        DockerMode::Nested => "nested",
    };
    args.push("--additional-flags".into());
    args.push(format!("--label cbox.managed=true --label cbox.docker_mode={docker_label} --label cbox.boxfile_path={} --label cbox.version={}",
        spec.boxfile_path.as_deref().unwrap_or(""),
        env!("CARGO_PKG_VERSION"),
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
