use crate::error::CboxError;
use std::process::Command;

/// The detected container backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    Podman,
    Docker,
}

impl Backend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Backend::Podman => "podman",
            Backend::Docker => "docker",
        }
    }

    /// Resolve the host socket path for `docker = host` mode at create time.
    // Used by docker_mode.rs flag bundle; the lib target may not see the call path.
    #[allow(dead_code)]
    pub fn socket_path(&self) -> String {
        match self {
            Backend::Podman => {
                let uid = libc_getuid();
                format!("/run/user/{uid}/podman/podman.sock")
            }
            Backend::Docker => "/var/run/docker.sock".to_string(),
        }
    }

    /// Detection order per §4.5:
    /// 1. --backend flag (passed in as `override_arg`)
    /// 2. $CBOX_BACKEND env
    /// 3. $DBX_CONTAINER_MANAGER env
    /// 4. podman on PATH and `podman info` exit 0 → podman (preferred)
    /// 5. docker on PATH and `docker info` exit 0 → docker
    /// 6. else exit 75
    pub fn detect(override_arg: Option<&str>) -> Result<Backend, CboxError> {
        // 1. explicit --backend flag
        if let Some(b) = override_arg {
            return parse_backend(b);
        }
        // 2. $CBOX_BACKEND
        if let Ok(v) = std::env::var("CBOX_BACKEND") {
            if !v.is_empty() {
                return parse_backend(&v);
            }
        }
        // 3. $DBX_CONTAINER_MANAGER
        if let Ok(v) = std::env::var("DBX_CONTAINER_MANAGER") {
            if !v.is_empty() {
                return parse_backend(&v);
            }
        }
        // 4. probe podman
        if probe_backend("podman") {
            return Ok(Backend::Podman);
        }
        // 5. probe docker
        if probe_backend("docker") {
            return Ok(Backend::Docker);
        }
        // 6. no usable backend
        Err(CboxError::tempfail(
            "No usable container backend found (tried podman, docker). \
             Is podman or docker installed and the service running?  cbox doctor",
        ))
    }

    /// Detect from environment only — used in tests where we can't probe real backends.
    /// Falls back to Podman as a test default.
    #[allow(dead_code)]
    pub fn detect_or_default(override_arg: Option<&str>) -> Result<Backend, CboxError> {
        // Try env vars first
        if let Some(b) = override_arg {
            return parse_backend(b);
        }
        if let Ok(v) = std::env::var("CBOX_BACKEND") {
            if !v.is_empty() {
                return parse_backend(&v);
            }
        }
        if let Ok(v) = std::env::var("DBX_CONTAINER_MANAGER") {
            if !v.is_empty() {
                return parse_backend(&v);
            }
        }
        // Default to podman in test context
        Ok(Backend::Podman)
    }
}

fn parse_backend(s: &str) -> Result<Backend, CboxError> {
    match s.to_lowercase().as_str() {
        "podman" => Ok(Backend::Podman),
        "docker" => Ok(Backend::Docker),
        other => Err(CboxError::usage(format!(
            "Unknown backend \"{other}\". Expected podman or docker."
        ))),
    }
}

fn probe_backend(name: &str) -> bool {
    Command::new(name)
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Safe wrapper around getuid.
fn libc_getuid() -> u32 {
    #[cfg(unix)]
    // SAFETY: getuid() is always safe to call.
    unsafe {
        extern "C" {
            fn getuid() -> u32;
        }
        getuid()
    }
    #[cfg(not(unix))]
    {
        1000
    }
}
