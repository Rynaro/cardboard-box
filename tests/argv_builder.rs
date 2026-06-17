//! AC-ARGV-1: pure-function golden tests for build_create_argv.
//! No runner, no mock, no I/O — just pure function assertions.

use cbox::core::spec::{CreateSpec, DockerMode, MountSpec};
use cbox::dbox::argv::build_create_argv;
use cbox::dbox::backend::Backend;

fn base_spec(name: &str) -> CreateSpec {
    let mut s = CreateSpec::new(name, Backend::Podman);
    s.image = "fedora-toolbox:latest".to_string();
    s
}

/// Helper: assert args contain a subsequence (all needles present in order, not necessarily adjacent).
fn assert_args_contain(args: &[String], needles: &[&str]) {
    for needle in needles {
        assert!(
            args.iter().any(|a| a == needle),
            "Expected argv to contain \"{needle}\" but got:\n  {args:?}"
        );
    }
}

fn assert_args_not_contain(args: &[String], needle: &str) {
    assert!(
        !args.iter().any(|a| a == needle),
        "Expected argv NOT to contain \"{needle}\" but it does:\n  {args:?}"
    );
}

// ─── docker=none ─────────────────────────────────────────────────────────────

#[test]
fn test_argv_none_minimal() {
    let spec = base_spec("web-dev");
    let args = build_create_argv(&spec);
    assert_args_contain(
        &args,
        &[
            "create",
            "--yes",
            "--name",
            "web-dev",
            "--image",
            "fedora-toolbox:latest",
        ],
    );
    // No socket volume for none
    let v_pos: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, a)| a.as_str() == "--volume")
        .collect();
    assert!(
        v_pos.is_empty(),
        "docker=none should not have --volume for socket"
    );
}

#[test]
fn test_argv_none_with_packages() {
    let mut spec = base_spec("toolbox");
    spec.packages = vec!["git".to_string(), "ripgrep".to_string()];
    let args = build_create_argv(&spec);
    assert_args_contain(&args, &["--additional-packages"]);
    let idx = args
        .iter()
        .position(|a| a == "--additional-packages")
        .unwrap();
    let pkg_str = &args[idx + 1];
    assert!(pkg_str.contains("git"), "packages arg should contain 'git'");
    assert!(
        pkg_str.contains("ripgrep"),
        "packages arg should contain 'ripgrep'"
    );
}

#[test]
fn test_argv_none_with_mounts() {
    let mut spec = base_spec("dev");
    spec.mounts = vec![
        MountSpec {
            host: "/home/user/code".to_string(),
            guest: "/code".to_string(),
            mode: "rw".to_string(),
        },
        MountSpec {
            host: "/data".to_string(),
            guest: "/data".to_string(),
            mode: "ro".to_string(),
        },
    ];
    let args = build_create_argv(&spec);
    assert_args_contain(&args, &["--volume"]);
    let vols: Vec<&String> = args
        .windows(2)
        .filter(|w| w[0] == "--volume")
        .map(|w| &w[1])
        .collect();
    assert_eq!(vols.len(), 2, "should have 2 --volume flags");
    assert!(vols[0].contains("/home/user/code"));
    assert!(vols[1].contains("ro"));
}

#[test]
fn test_argv_none_unshare_all() {
    let mut spec = base_spec("sandboxed");
    spec.unshare = Some("all".to_string());
    let args = build_create_argv(&spec);
    assert_args_contain(&args, &["--unshare-all"]);
}

#[test]
fn test_argv_none_unshare_list() {
    let mut spec = base_spec("sandboxed");
    spec.unshare = Some("netns ipc".to_string());
    let args = build_create_argv(&spec);
    assert_args_contain(&args, &["--unshare-netns", "--unshare-ipc"]);
}

#[test]
fn test_argv_none_init() {
    let mut spec = base_spec("systemd-box");
    spec.init = true;
    let args = build_create_argv(&spec);
    assert_args_contain(&args, &["--init"]);
}

// ─── docker=host, podman backend ─────────────────────────────────────────────

#[test]
fn test_argv_host_podman() {
    let mut spec = base_spec("docker-box");
    spec.docker_mode = DockerMode::Host;
    spec.backend = Backend::Podman;
    spec.uid = 1000;
    let args = build_create_argv(&spec);

    // Should have the podman socket volume
    let vols: Vec<&String> = args
        .windows(2)
        .filter(|w| w[0] == "--volume")
        .map(|w| &w[1])
        .collect();
    assert!(
        vols.iter().any(|v| v.contains("podman.sock")),
        "docker=host (podman) must have podman.sock volume, got: {args:?}"
    );

    // Should have podman-remote in additional-packages
    let pkg_idx = args
        .iter()
        .position(|a| a == "--additional-packages")
        .unwrap();
    assert!(
        args[pkg_idx + 1].contains("podman-remote"),
        "should include podman-remote package"
    );

    // Should have cbox.docker_mode=host label
    let label_flag = args.iter().any(|a| a.contains("cbox.docker_mode=host"));
    assert!(label_flag, "should stamp cbox.docker_mode=host label");

    // Should NOT have a docker socket
    assert!(
        !vols.iter().any(|v| v.contains("docker.sock")),
        "podman mode should not have docker.sock"
    );
}

#[test]
fn test_argv_host_docker() {
    let mut spec = base_spec("docker-box");
    spec.docker_mode = DockerMode::Host;
    spec.backend = Backend::Docker;
    spec.uid = 1000;
    let args = build_create_argv(&spec);

    // Should have the docker socket volume
    let vols: Vec<&String> = args
        .windows(2)
        .filter(|w| w[0] == "--volume")
        .map(|w| &w[1])
        .collect();
    assert!(
        vols.iter().any(|v| v.contains("docker.sock")),
        "docker=host (docker) must have docker.sock volume"
    );

    // Should have docker-cli in additional-packages
    let pkg_idx = args
        .iter()
        .position(|a| a == "--additional-packages")
        .unwrap();
    assert!(
        args[pkg_idx + 1].contains("docker-cli"),
        "should include docker-cli package"
    );

    // Should NOT have podman socket
    assert!(
        !vols.iter().any(|v| v.contains("podman.sock")),
        "docker mode should not have podman.sock"
    );
}

// ─── docker=nested ───────────────────────────────────────────────────────────

#[test]
fn test_argv_nested() {
    let mut spec = base_spec("dind-box");
    spec.docker_mode = DockerMode::Nested;
    let args = build_create_argv(&spec);

    // Must have --init (nested forces it)
    assert_args_contain(&args, &["--init"]);

    // Must have docker-ce in packages
    let pkg_idx = args
        .iter()
        .position(|a| a == "--additional-packages")
        .unwrap();
    let pkgs = &args[pkg_idx + 1];
    assert!(pkgs.contains("docker-ce"), "nested should add docker-ce");

    // Must NOT have any host socket volume
    let vols: Vec<&String> = args
        .windows(2)
        .filter(|w| w[0] == "--volume")
        .map(|w| &w[1])
        .collect();
    assert!(
        !vols.iter().any(|v| v.contains(".sock")),
        "nested should not have socket volume"
    );

    // Label
    assert!(args.iter().any(|a| a.contains("cbox.docker_mode=nested")));
}

// ─── labels ──────────────────────────────────────────────────────────────────

#[test]
fn test_argv_labels_stamped() {
    let spec = base_spec("labeled");
    let args = build_create_argv(&spec);

    // All cbox labels should be present
    let labels_flag = args.iter().find(|a| a.contains("cbox.managed=true"));
    assert!(
        labels_flag.is_some(),
        "cbox.managed=true label should be stamped"
    );
    assert!(args.iter().any(|a| a.contains("cbox.docker_mode=none")));
}

// ─── optional fields ─────────────────────────────────────────────────────────

#[test]
fn test_argv_home_and_hostname() {
    let mut spec = base_spec("mybox");
    spec.home = Some("/custom/home".to_string());
    spec.hostname = Some("mybox.local".to_string());
    let args = build_create_argv(&spec);
    assert_args_contain(
        &args,
        &["--home", "/custom/home", "--hostname", "mybox.local"],
    );
}

#[test]
fn test_argv_pull_and_root() {
    let mut spec = base_spec("mybox");
    spec.pull = true;
    spec.root = true;
    let args = build_create_argv(&spec);
    assert_args_contain(&args, &["--pull", "--root"]);
}

// ─── rm / enter argv ─────────────────────────────────────────────────────────

#[test]
fn test_build_rm_argv_basic() {
    use cbox::core::spec::RmSpec;
    use cbox::dbox::argv::build_rm_argv;

    let spec = RmSpec {
        names: vec!["web-dev".to_string()],
        force: false,
        rm_home: false,
        all: false,
        yes: true,
        backend: Backend::Podman,
    };
    let args = build_rm_argv(&spec);
    assert_args_contain(&args, &["rm", "web-dev"]);
    assert_args_not_contain(&args, "--force");
}

#[test]
fn test_build_rm_argv_force() {
    use cbox::core::spec::RmSpec;
    use cbox::dbox::argv::build_rm_argv;

    let spec = RmSpec {
        names: vec!["web-dev".to_string()],
        force: true,
        rm_home: false,
        all: false,
        yes: true,
        backend: Backend::Podman,
    };
    let args = build_rm_argv(&spec);
    assert_args_contain(&args, &["rm", "--force", "web-dev"]);
}

#[test]
fn test_build_enter_argv_basic() {
    use cbox::core::spec::EnterSpec;
    use cbox::dbox::argv::build_enter_argv;

    let spec = EnterSpec {
        name: "web-dev".to_string(),
        root: false,
        clean_path: false,
        cmd: vec![],
        backend: Backend::Podman,
    };
    let args = build_enter_argv(&spec);
    assert_args_contain(&args, &["enter", "--name", "web-dev"]);
    assert_args_not_contain(&args, "--");
}

#[test]
fn test_build_enter_argv_with_cmd() {
    use cbox::core::spec::EnterSpec;
    use cbox::dbox::argv::build_enter_argv;

    let spec = EnterSpec {
        name: "web-dev".to_string(),
        root: false,
        clean_path: false,
        cmd: vec!["ls".to_string(), "-la".to_string()],
        backend: Backend::Podman,
    };
    let args = build_enter_argv(&spec);
    assert_args_contain(&args, &["enter", "--name", "web-dev", "--", "ls", "-la"]);
}

// ─── P2 argv builders (AC-ARGV-P2-1, AC-ARGV-P2-2, AC-ARGV-P2-3) ────────────

// AC-ARGV-P2-1: build_provision_shell_argv golden test
#[test]
fn ac_argv_p2_1_provision_shell_argv() {
    use cbox::dbox::argv::build_provision_shell_argv;

    let args = build_provision_shell_argv("web-dev", "echo hi");
    assert_eq!(
        args,
        vec!["enter", "--name", "web-dev", "--", "sh", "-c", "echo hi"],
        "provision shell argv should match golden"
    );
}

// AC-ARGV-P2-2: build_copy_argv golden test
#[test]
fn ac_argv_p2_2_copy_argv() {
    use cbox::dbox::argv::build_copy_argv;

    let args = build_copy_argv("web-dev", "./a", "/b");
    assert_eq!(
        args,
        vec!["cp", "./a", "web-dev:/b"],
        "copy argv should match golden"
    );
}

// AC-ARGV-P2-3: build_create_argv now includes cbox.image label (additive to P1)
#[test]
fn ac_argv_p2_3_create_includes_image_label() {
    let mut spec = base_spec("web-dev");
    spec.image = "fedora-toolbox:40".to_string();
    let args = build_create_argv(&spec);

    let image_label = args
        .iter()
        .any(|a| a.contains("cbox.image=fedora-toolbox:40"));
    assert!(
        image_label,
        "create argv should include cbox.image label, got: {args:?}"
    );
}

// Additional P2 argv tests

#[test]
fn test_build_state_write_argv_escaping() {
    use cbox::dbox::argv::build_state_write_argv;

    let json = r#"{"steps":[{"result":"ok"}]}"#;
    let args = build_state_write_argv("web-dev", json);

    // Must have 7 elements: enter --name web-dev -- sh -c <cmd>
    assert_eq!(args.len(), 7);
    assert_eq!(args[0], "enter");
    assert_eq!(args[1], "--name");
    assert_eq!(args[2], "web-dev");
    assert_eq!(args[3], "--");
    assert_eq!(args[4], "sh");
    assert_eq!(args[5], "-c");

    let sh_cmd = &args[6];
    assert!(sh_cmd.contains("mkdir"), "should create dir");
    assert!(sh_cmd.contains("printf"), "should use printf");
    assert!(
        sh_cmd.contains("provision.json"),
        "should reference provision.json"
    );
}

// Regression: paths must be double-quoted so the shell expands ${XDG_STATE_HOME:-…}.
// Single-quoting prevents expansion and creates literal junk directories (bug regression).
#[test]
fn test_build_state_write_argv_paths_double_quoted() {
    use cbox::dbox::argv::build_state_write_argv;

    let json = r#"{"steps":[]}"#;
    let args = build_state_write_argv("my-box", json);
    let sh_cmd = &args[6];

    // The XDG path must be wrapped in double quotes, not single quotes, so the
    // shell expands ${XDG_STATE_HOME:-$HOME/.local/state} at runtime.
    assert!(
        sh_cmd.contains("\"${XDG_STATE_HOME"),
        "write: XDG path must be double-quoted for shell expansion, got: {sh_cmd}"
    );

    // Regression guard: single-quoted XDG_STATE_HOME would suppress expansion entirely.
    assert!(
        !sh_cmd.contains("'${XDG_STATE_HOME"),
        "write: XDG path must NOT be single-quoted (would suppress shell expansion), got: {sh_cmd}"
    );

    // The JSON payload must still be single-quoted (safe literal embedding).
    assert!(
        sh_cmd.contains("'") && sh_cmd.contains("printf '%s'"),
        "write: JSON payload must remain single-quoted via printf '%s', got: {sh_cmd}"
    );
}

// Regression: read and write must reference the same expanded path so state
// written by one is found by the other (idempotency contract).
#[test]
fn test_build_state_read_write_path_consistency() {
    use cbox::dbox::argv::build_state_read_argv;
    use cbox::dbox::argv::build_state_write_argv;

    let read_cmd = &build_state_read_argv("my-box")[6];
    let write_cmd = &build_state_write_argv("my-box", "{}")[6];

    // Both must reference the same XDG expression (double-quoted).
    assert!(
        read_cmd.contains("\"${XDG_STATE_HOME:-$HOME/.local/state}/cbox/provision.json\""),
        "read: must double-quote XDG path, got: {read_cmd}"
    );
    assert!(
        write_cmd.contains("\"${XDG_STATE_HOME:-$HOME/.local/state}/cbox/provision.json\""),
        "write: must double-quote XDG path to match read, got: {write_cmd}"
    );
}

#[test]
fn test_build_state_read_argv_golden() {
    use cbox::dbox::argv::build_state_read_argv;

    let args = build_state_read_argv("web-dev");
    assert_eq!(args[0], "enter");
    assert_eq!(args[1], "--name");
    assert_eq!(args[2], "web-dev");
    assert_eq!(args[3], "--");
    assert_eq!(args[4], "sh");
    assert_eq!(args[5], "-c");
    assert!(
        args[6].contains("provision.json"),
        "state read should reference provision.json"
    );
}
