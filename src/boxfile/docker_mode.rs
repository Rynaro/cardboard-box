//! docker = none|host|nested → concrete flag bundles per §5.
//! Pure: no I/O, no runner.

use crate::boxfile::distro_family::DistroFamily;
use crate::core::spec::DockerMode;
use crate::dbox::backend::Backend;

/// The concrete flags that a docker mode adds to `distrobox create`.
#[derive(Debug, Default)]
pub struct DockerModeFlags {
    /// Extra packages to add to --additional-packages.
    pub extra_packages: Vec<String>,
    /// A --volume argument to add (host:guest), if any.
    pub socket_volume: Option<String>,
    /// Extra --additional-flags strings (env vars, etc.).
    pub extra_flags: Vec<String>,
    /// docker=nested forces --init.
    pub force_init: bool,
    /// Non-fatal advisory messages: populated when package injection is skipped
    /// for a distro family whose base repos don't ship the required client/daemon.
    /// The caller may surface these to the user so they know to install manually.
    ///
    /// TODO(follow-up): wire these through the create/up output to the CLI output
    /// layer. Currently they are populated here but not yet printed — the plumbing
    /// would require threading warnings through CreateOutcome and several call sites,
    /// which is out of scope for this bug fix.
    #[allow(dead_code)]
    pub warnings: Vec<String>,
}

/// Resolve docker mode flags given mode, backend, host UID, and target distro family.
///
/// Invariant: never emit a package name that the target image's package manager
/// cannot resolve from its base repositories.  This prevents the hard-abort in
/// `core::create` when distrobox exits non-zero because an RPM name was fed to apt
/// or apk.
///
/// Per-family package selection:
/// - **Rpm**: keep today's names exactly — `docker-cli`/`podman-remote` for host;
///   `docker-ce`, `docker-ce-cli`, `containerd` for nested.
/// - **Alpine** host: `docker-cli` (present in Alpine's community repo).
///   Nested: nothing — dockerd is not reliably present in Alpine base repos.
/// - **Arch** host: `docker` (ships the CLI and daemon in a single package).
///   Nested: nothing — the nested daemon scenario is not well-supported on Arch base.
/// - **Suse** host: `docker` (available in openSUSE base repos).
///   Nested: nothing — non-base daemon packages are not reliable.
/// - **Debian/Ubuntu**: nothing for either host or nested — neither `docker-cli`
///   nor `docker-ce` is present in the official base apt repos; users must add the
///   Docker APT repo themselves and install manually.
/// - **Unknown**: nothing (safe default — we cannot guess the package manager).
///
/// Socket mounts and env flags for host mode are preserved for every family —
/// only the package list varies.
pub fn docker_mode_flags(
    mode: &DockerMode,
    backend: &Backend,
    uid: u32,
    family: &DistroFamily,
) -> DockerModeFlags {
    match mode {
        DockerMode::None => DockerModeFlags::default(),

        DockerMode::Host => {
            let (socket_volume, extra_flags) = match backend {
                Backend::Podman => {
                    let sock = format!(
                        "/run/user/{uid}/podman/podman.sock:/run/user/{uid}/podman/podman.sock"
                    );
                    (Some(sock), vec![])
                }
                Backend::Docker => {
                    let sock = "/var/run/docker.sock:/var/run/docker.sock".to_string();
                    let env_flag = "--env DOCKER_HOST=unix:///var/run/docker.sock".to_string();
                    (Some(sock), vec![env_flag])
                }
            };

            let (extra_packages, warnings) = host_packages_for_family(backend, family);

            DockerModeFlags {
                extra_packages,
                socket_volume,
                extra_flags,
                force_init: false,
                warnings,
            }
        }

        DockerMode::Nested => {
            let (extra_packages, warnings) = nested_packages_for_family(family);
            DockerModeFlags {
                extra_packages,
                socket_volume: None,
                extra_flags: vec![],
                force_init: true, // --init so systemd can manage dockerd
                warnings,
            }
        }
    }
}

/// Return (packages, warnings) for docker=host per distro family.
fn host_packages_for_family(
    backend: &Backend,
    family: &DistroFamily,
) -> (Vec<String>, Vec<String>) {
    match family {
        DistroFamily::Rpm => {
            // RPM names are correct for dnf-based images — keep as-is.
            let pkgs = match backend {
                Backend::Podman => vec!["podman-remote".to_string()],
                Backend::Docker => vec!["docker-cli".to_string()],
            };
            (pkgs, vec![])
        }
        DistroFamily::Alpine => {
            // `docker-cli` is the correct Alpine community package name.
            // For Podman there is no reliable Alpine base package; fall through to warn.
            match backend {
                Backend::Docker => (vec!["docker-cli".to_string()], vec![]),
                Backend::Podman => (
                    vec![],
                    vec![
                        "docker=host (podman backend): no reliable podman-remote package in \
                         Alpine base repos. Install the Podman client manually inside the box."
                            .to_string(),
                    ],
                ),
            }
        }
        DistroFamily::Arch => {
            // `docker` ships the CLI on Arch; `podman` ships the Podman CLI.
            let pkgs = match backend {
                Backend::Docker => vec!["docker".to_string()],
                Backend::Podman => vec!["podman".to_string()],
            };
            (pkgs, vec![])
        }
        DistroFamily::Suse => {
            // `docker` is available in openSUSE base repos; podman as well.
            let pkgs = match backend {
                Backend::Docker => vec!["docker".to_string()],
                Backend::Podman => vec!["podman".to_string()],
            };
            (pkgs, vec![])
        }
        DistroFamily::Debian => {
            // Neither docker-cli nor podman-remote is in Debian/Ubuntu base apt repos.
            // Inject nothing; warn the user.
            let backend_name = match backend {
                Backend::Docker => "docker-cli",
                Backend::Podman => "podman-remote",
            };
            (
                vec![],
                vec![format!(
                    "docker=host: {backend_name} is not available in Debian/Ubuntu base apt \
                     repos. Add the upstream Docker (or Podman) apt repository inside the box \
                     and install the client manually."
                )],
            )
        }
        DistroFamily::Unknown => {
            // Cannot determine the package manager; do not inject anything.
            let backend_name = match backend {
                Backend::Docker => "docker-cli",
                Backend::Podman => "podman-remote",
            };
            (
                vec![],
                vec![format!(
                    "docker=host: could not detect distro family from image name; skipping \
                     automatic injection of {backend_name}. Install the container client \
                     manually inside the box."
                )],
            )
        }
    }
}

/// Return (packages, warnings) for docker=nested per distro family.
fn nested_packages_for_family(family: &DistroFamily) -> (Vec<String>, Vec<String>) {
    match family {
        DistroFamily::Rpm => {
            // RPM names are correct for dnf-based images — keep as-is.
            (
                vec![
                    "docker-ce".to_string(),
                    "docker-ce-cli".to_string(),
                    "containerd".to_string(),
                ],
                vec![],
            )
        }
        DistroFamily::Debian
        | DistroFamily::Alpine
        | DistroFamily::Arch
        | DistroFamily::Suse
        | DistroFamily::Unknown => {
            // docker-ce and containerd are not in base repos for these families.
            // Inject nothing; warn the user.
            let family_name = match family {
                DistroFamily::Debian => "Debian/Ubuntu",
                DistroFamily::Alpine => "Alpine",
                DistroFamily::Arch => "Arch",
                DistroFamily::Suse => "openSUSE/SUSE",
                DistroFamily::Unknown => "unknown",
                DistroFamily::Rpm => unreachable!(),
            };
            (
                vec![],
                vec![format!(
                    "docker=nested: docker-ce/containerd are not in {family_name} base repos. \
                     Add the upstream Docker repository inside the box and install \
                     docker-ce, docker-ce-cli, and containerd manually."
                )],
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Helper ──────────────────────────────────────────────────────────────

    fn flags(mode: DockerMode, backend: Backend, family: DistroFamily) -> DockerModeFlags {
        docker_mode_flags(&mode, &backend, 1000, &family)
    }

    // ─── docker=none (unchanged for all families) ─────────────────────────────

    #[test]
    fn none_always_empty() {
        for family in [
            DistroFamily::Rpm,
            DistroFamily::Debian,
            DistroFamily::Alpine,
            DistroFamily::Arch,
            DistroFamily::Suse,
            DistroFamily::Unknown,
        ] {
            let f = flags(DockerMode::None, Backend::Docker, family);
            assert!(f.extra_packages.is_empty());
            assert!(f.socket_volume.is_none());
            assert!(f.extra_flags.is_empty());
            assert!(!f.force_init);
            assert!(f.warnings.is_empty());
        }
    }

    // ─── docker=host, RPM — no regression ────────────────────────────────────

    #[test]
    fn host_rpm_docker_injects_docker_cli() {
        let f = flags(DockerMode::Host, Backend::Docker, DistroFamily::Rpm);
        assert_eq!(f.extra_packages, vec!["docker-cli"]);
        assert!(f.warnings.is_empty());
        assert!(f.socket_volume.as_deref().unwrap().contains("docker.sock"));
    }

    #[test]
    fn host_rpm_podman_injects_podman_remote() {
        let f = flags(DockerMode::Host, Backend::Podman, DistroFamily::Rpm);
        assert_eq!(f.extra_packages, vec!["podman-remote"]);
        assert!(f.warnings.is_empty());
        assert!(f.socket_volume.as_deref().unwrap().contains("podman.sock"));
    }

    // ─── docker=host, Debian — empty packages + warning ──────────────────────

    #[test]
    fn host_debian_docker_no_packages_has_warning() {
        let f = flags(DockerMode::Host, Backend::Docker, DistroFamily::Debian);
        assert!(
            f.extra_packages.is_empty(),
            "Debian host docker: must inject nothing, got {:?}",
            f.extra_packages
        );
        assert!(
            !f.warnings.is_empty(),
            "Debian host docker: must produce a warning"
        );
    }

    #[test]
    fn host_debian_podman_no_packages_has_warning() {
        let f = flags(DockerMode::Host, Backend::Podman, DistroFamily::Debian);
        assert!(f.extra_packages.is_empty());
        assert!(!f.warnings.is_empty());
        // Socket volume must still be present
        assert!(f.socket_volume.as_deref().unwrap().contains("podman.sock"));
    }

    // ─── docker=host, Alpine ─────────────────────────────────────────────────

    #[test]
    fn host_alpine_docker_injects_docker_cli() {
        let f = flags(DockerMode::Host, Backend::Docker, DistroFamily::Alpine);
        assert_eq!(f.extra_packages, vec!["docker-cli"]);
        assert!(f.warnings.is_empty());
    }

    #[test]
    fn host_alpine_podman_no_packages_has_warning() {
        let f = flags(DockerMode::Host, Backend::Podman, DistroFamily::Alpine);
        assert!(f.extra_packages.is_empty());
        assert!(!f.warnings.is_empty());
    }

    // ─── docker=host, Arch ───────────────────────────────────────────────────

    #[test]
    fn host_arch_docker_injects_docker() {
        let f = flags(DockerMode::Host, Backend::Docker, DistroFamily::Arch);
        assert_eq!(f.extra_packages, vec!["docker"]);
        assert!(f.warnings.is_empty());
    }

    #[test]
    fn host_arch_podman_injects_podman() {
        let f = flags(DockerMode::Host, Backend::Podman, DistroFamily::Arch);
        assert_eq!(f.extra_packages, vec!["podman"]);
        assert!(f.warnings.is_empty());
    }

    // ─── docker=host, Unknown — empty + warning ───────────────────────────────

    #[test]
    fn host_unknown_no_packages_has_warning() {
        let f = flags(DockerMode::Host, Backend::Docker, DistroFamily::Unknown);
        assert!(f.extra_packages.is_empty());
        assert!(!f.warnings.is_empty());
    }

    // ─── docker=host: socket_volume preserved for all families ───────────────

    #[test]
    fn host_socket_volume_preserved_all_families() {
        for family in [
            DistroFamily::Rpm,
            DistroFamily::Debian,
            DistroFamily::Alpine,
            DistroFamily::Arch,
            DistroFamily::Suse,
            DistroFamily::Unknown,
        ] {
            let f = flags(DockerMode::Host, Backend::Docker, family);
            assert!(
                f.socket_volume.is_some(),
                "docker=host: socket_volume must be present for every family"
            );
            assert!(f.socket_volume.as_deref().unwrap().contains("docker.sock"));
        }
    }

    // ─── docker=nested, RPM — no regression ──────────────────────────────────

    #[test]
    fn nested_rpm_injects_docker_ce() {
        let f = flags(DockerMode::Nested, Backend::Docker, DistroFamily::Rpm);
        assert!(f.extra_packages.contains(&"docker-ce".to_string()));
        assert!(f.extra_packages.contains(&"docker-ce-cli".to_string()));
        assert!(f.extra_packages.contains(&"containerd".to_string()));
        assert!(f.force_init);
        assert!(f.warnings.is_empty());
    }

    // ─── docker=nested, Debian — empty + warning ─────────────────────────────

    #[test]
    fn nested_debian_no_packages_has_warning() {
        let f = flags(DockerMode::Nested, Backend::Docker, DistroFamily::Debian);
        assert!(
            f.extra_packages.is_empty(),
            "Debian nested: must inject nothing"
        );
        assert!(!f.warnings.is_empty());
        // force_init is still set so --init is passed (systemd path)
        assert!(f.force_init);
    }

    // ─── docker=nested, Alpine — empty + warning ──────────────────────────────

    #[test]
    fn nested_alpine_no_packages_has_warning() {
        let f = flags(DockerMode::Nested, Backend::Docker, DistroFamily::Alpine);
        assert!(f.extra_packages.is_empty());
        assert!(!f.warnings.is_empty());
        assert!(f.force_init);
    }

    // ─── docker=nested, Unknown — empty + warning ─────────────────────────────

    #[test]
    fn nested_unknown_no_packages_has_warning() {
        let f = flags(DockerMode::Nested, Backend::Docker, DistroFamily::Unknown);
        assert!(f.extra_packages.is_empty());
        assert!(!f.warnings.is_empty());
    }

    // ─── docker=nested: no socket_volume for any family ──────────────────────

    #[test]
    fn nested_never_has_socket_volume() {
        for family in [
            DistroFamily::Rpm,
            DistroFamily::Debian,
            DistroFamily::Alpine,
            DistroFamily::Arch,
            DistroFamily::Suse,
            DistroFamily::Unknown,
        ] {
            let f = flags(DockerMode::Nested, Backend::Docker, family);
            assert!(
                f.socket_volume.is_none(),
                "docker=nested must never produce a socket volume"
            );
        }
    }
}
