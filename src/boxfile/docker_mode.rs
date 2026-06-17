//! docker = none|host|nested → concrete flag bundles per §5.
//! Pure: no I/O, no runner.

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
}

/// Resolve docker mode flags given mode, backend, and host UID.
pub fn docker_mode_flags(mode: &DockerMode, backend: &Backend, uid: u32) -> DockerModeFlags {
    match mode {
        DockerMode::None => DockerModeFlags::default(),

        DockerMode::Host => {
            let (socket_volume, extra_packages, extra_flags) = match backend {
                Backend::Podman => {
                    let sock = format!(
                        "/run/user/{uid}/podman/podman.sock:/run/user/{uid}/podman/podman.sock"
                    );
                    (Some(sock), vec!["podman-remote".to_string()], vec![])
                }
                Backend::Docker => {
                    let sock = "/var/run/docker.sock:/var/run/docker.sock".to_string();
                    let env_flag = "--env DOCKER_HOST=unix:///var/run/docker.sock".to_string();
                    (Some(sock), vec!["docker-cli".to_string()], vec![env_flag])
                }
            };
            DockerModeFlags {
                extra_packages,
                socket_volume,
                extra_flags,
                force_init: false,
            }
        }

        DockerMode::Nested => DockerModeFlags {
            extra_packages: vec![
                "docker-ce".to_string(),
                "docker-ce-cli".to_string(),
                "containerd".to_string(),
            ],
            socket_volume: None,
            extra_flags: vec![],
            force_init: true, // --init so systemd can manage dockerd
        },
    }
}
