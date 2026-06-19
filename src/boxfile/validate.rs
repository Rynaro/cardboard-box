//! Validation rules per §6.2. Exit 65 on failure.

use super::model::{Boxfile, DockerModeField, ProvisionType, UnshareSpec};
use crate::error::CboxError;

const NAME_REGEX: &str = "^[a-zA-Z0-9][a-zA-Z0-9_.-]*$";
const VALID_UNSHARE: &[&str] = &["netns", "ipc", "process", "devsys", "groups"];

/// POSIX env-var name regex: first char alpha or underscore, rest alnum or underscore.
const ENV_NAME_REGEX: &str = "^[A-Za-z_][A-Za-z0-9_]*$";

/// Check that a [secrets] or [env] key is a valid env-var name.
pub fn is_valid_env_name(k: &str) -> bool {
    if k.is_empty() {
        return false;
    }
    let mut chars = k.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Validate a Boxfile. Returns a list of warnings (not fatal) and errors as CboxError::DataErr.
pub struct ValidationResult {
    pub warnings: Vec<String>,
}

pub fn validate(bf: &Boxfile) -> Result<ValidationResult, CboxError> {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    // name: non-empty + regex
    if bf.name.is_empty() {
        errors.push("Boxfile 'name' is required and cannot be empty.".to_string());
    } else if !is_valid_name(&bf.name) {
        errors.push(format!(
            "Boxfile 'name' \"{}\" is invalid. Must match {}",
            bf.name, NAME_REGEX
        ));
    }

    // docker: already enforced by serde enum, no extra check needed

    // mounts: host/guest must be absolute paths; guest paths unique
    let mut guest_paths = std::collections::HashSet::new();
    for (i, m) in bf.mounts.iter().enumerate() {
        if m.host.is_empty() {
            errors.push(format!("mounts[{i}].host is required."));
        } else if !m.host.starts_with('/') {
            errors.push(format!(
                "mounts[{i}].host \"{}\" must be an absolute path.",
                m.host
            ));
        }
        if m.guest.is_empty() {
            errors.push(format!("mounts[{i}].guest is required."));
        } else if !m.guest.starts_with('/') {
            errors.push(format!(
                "mounts[{i}].guest \"{}\" must be an absolute path.",
                m.guest
            ));
        } else if !guest_paths.insert(m.guest.as_str()) {
            errors.push(format!("mounts[{i}].guest \"{}\" is not unique.", m.guest));
        }
    }

    // sandbox.unshare: validate enum values
    match &bf.sandbox.unshare {
        UnshareSpec::Empty => {}
        UnshareSpec::All(s) => {
            if s != "all" && !s.is_empty() {
                errors.push(format!(
                    "sandbox.unshare: expected \"all\" or a list, got \"{s}\"."
                ));
            }
        }
        UnshareSpec::List(items) => {
            for item in items {
                if !VALID_UNSHARE.contains(&item.as_str()) {
                    errors.push(format!(
                        "sandbox.unshare: unknown value \"{item}\". Valid: {}",
                        VALID_UNSHARE.join(", ")
                    ));
                }
            }
        }
    }

    // sandbox.unshare non-empty AND docker != none → warning
    if !bf.sandbox.unshare.is_empty() && bf.docker != DockerModeField::None {
        warnings.push(format!(
            "sandbox.unshare is set with docker=\"{}\". Hardening flags may contradict host/nested coupling.",
            bf.docker.as_str()
        ));
    }

    // provision steps
    for (i, step) in bf.provision.iter().enumerate() {
        match step.step_type {
            ProvisionType::Shell => {
                if step.run.is_none() {
                    errors.push(format!("provision[{i}]: type=\"shell\" requires 'run'."));
                }
            }
            ProvisionType::Copy => {
                if step.src.is_none() {
                    errors.push(format!("provision[{i}]: type=\"copy\" requires 'src'."));
                }
                if step.dst.is_none() {
                    errors.push(format!("provision[{i}]: type=\"copy\" requires 'dst'."));
                }
            }
        }
    }

    // ─── [secrets] + [env] validation rules (V1–V6) ──────────────────────────

    let total_count = bf.secrets.len() + bf.env.len();

    // V4: count bound
    if total_count > 64 {
        errors.push(format!(
            "Too many env/secret entries ({total_count}). The limit is 64."
        ));
    }

    for (key, entry) in &bf.secrets {
        // V5: key length
        if key.len() > 128 {
            errors.push(format!(
                "env/secret key \"{key}\" is too long ({} chars). Max 128.",
                key.len()
            ));
            continue;
        }
        // V1: valid env-var name
        if !is_valid_env_name(key) {
            errors.push(format!(
                "secrets.\"{key}\": invalid env-var name. Must match {ENV_NAME_REGEX}"
            ));
        }
        // V2: no collision with [env]
        if bf.env.contains_key(key) {
            errors.push(format!(
                "\"{key}\" is declared in both [env] and [secrets]; a key may be one or the other, not both."
            ));
        }
        // V3: from must be "keyring"
        if entry.from != "keyring" {
            errors.push(format!(
                "secrets.\"{key}\".from = \"{}\" is not supported in this version. \
                 Only \"keyring\" is valid (prompt-as-source is deferred).",
                entry.from
            ));
        }
    }

    for (key, value) in &bf.env {
        // V5: key length
        if key.len() > 128 {
            errors.push(format!(
                "env/secret key \"{key}\" is too long ({} chars). Max 128.",
                key.len()
            ));
            continue;
        }
        // V1: valid env-var name
        if !is_valid_env_name(key) {
            errors.push(format!(
                "env.\"{key}\": invalid env-var name. Must match {ENV_NAME_REGEX}"
            ));
        }
        // V6: value length
        if value.len() > 4096 {
            errors.push(format!(
                "env.\"{key}\" value is too long ({} bytes). Max 4096.",
                value.len()
            ));
        }
    }

    if !errors.is_empty() {
        return Err(CboxError::dataerr(errors.join("\n")));
    }

    Ok(ValidationResult { warnings })
}

/// Client-side NAME validation per §3.0.
pub fn is_valid_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}
