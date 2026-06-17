//! Boxfile ↔ live diff classification (§6.2).
//! Pure function: diff_boxfile_vs_live(bf, live) -> DiffResult.

use crate::boxfile::model::Boxfile;
use crate::core::spec::{DiffField, DiffResult, InspectResult, MountResult, PackageDiff};

// ─── Public API ──────────────────────────────────────────────────────────────

/// Diff a Boxfile against the live container state.
/// Returns the diff classification and a list of changed fields.
pub fn diff_boxfile_vs_live(bf: &Boxfile, live: &InspectResult) -> DiffResult {
    let mut fields: Vec<DiffField> = Vec::new();

    // --- image ---
    let live_image = extract_image_label(live);
    if !images_equal(&bf.image, &live_image) {
        fields.push(DiffField {
            field: "image".to_string(),
            old: live_image,
            new: bf.image.clone(),
            class: "Recreate".to_string(),
        });
    }

    // --- docker mode ---
    let bf_docker = bf.docker.as_str().to_string();
    let live_docker = live.docker_mode.clone();
    if bf_docker != live_docker && live_docker != "unknown" {
        fields.push(DiffField {
            field: "docker".to_string(),
            old: live_docker,
            new: bf_docker,
            class: "Recreate".to_string(),
        });
    }

    // --- mounts ---
    let mount_diff = diff_mounts(&bf.mounts, &live.mounts);
    if !mount_diff.is_empty() {
        fields.push(DiffField {
            field: "mounts".to_string(),
            old: mount_diff.old,
            new: mount_diff.new,
            class: "Recreate".to_string(),
        });
    }

    // --- packages ---
    let pkg_diff = diff_packages(&bf.packages, &live.packages);
    if !pkg_diff.added.is_empty() {
        fields.push(DiffField {
            field: "packages".to_string(),
            old: live.packages.join(" "),
            new: format!("+{}", pkg_diff.added.join(" +")),
            class: "Incremental".to_string(),
        });
    }
    if !pkg_diff.removed.is_empty() {
        fields.push(DiffField {
            field: "packages".to_string(),
            old: live.packages.join(" "),
            new: format!("-{}", pkg_diff.removed.join(" -")),
            class: "Recreate".to_string(),
        });
    }

    // --- sandbox.unshare (R5: best-effort; assume unchanged if unrecoverable) ---
    // We treat sandbox fields as unrecoverable from live labels in v2.0.
    // Conservative: assume unchanged (never trigger a surprise recreate).
    // A later revision can read distrobox.unshare_* labels.

    let class = if fields.iter().any(|f| f.class == "Recreate") {
        "Recreate".to_string()
    } else {
        "Incremental".to_string()
    };

    DiffResult { class, fields }
}

/// Extract the package diff: which packages are added/removed vs live.
pub fn package_diff(bf: &Boxfile, live: &InspectResult) -> PackageDiff {
    diff_packages(&bf.packages, &live.packages)
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extract the image from the live inspect result.
/// Prefers the `cbox.image` label (§4.4); falls back to inspect Image field.
fn extract_image_label(live: &InspectResult) -> String {
    // The cbox.image label is not directly in InspectResult (it's a new label we add).
    // For now we use live.image which comes from the inspect JSON's Image field.
    // When the cbox.image label is present it will be identical to this value.
    live.image.clone()
}

/// Compare images with registry-prefix normalization.
/// "fedora-toolbox:latest" == "registry.fedoraproject.org/fedora-toolbox:latest" → false
/// We do simple string equality for now; a registry-aware comparison is a future refinement.
fn images_equal(bf_image: &str, live_image: &str) -> bool {
    if bf_image == live_image {
        return true;
    }
    // Both empty → equal
    if bf_image.is_empty() && live_image.is_empty() {
        return true;
    }
    false
}

struct MountDiff {
    old: String,
    new: String,
}

impl MountDiff {
    fn is_empty(&self) -> bool {
        self.old.is_empty() && self.new.is_empty()
    }
}

fn diff_mounts(
    bf_mounts: &[crate::boxfile::model::MountEntry],
    live_mounts: &[MountResult],
) -> MountDiff {
    // Build canonical "host:guest:mode" tuples for comparison.
    let bf_set: std::collections::BTreeSet<String> = bf_mounts
        .iter()
        .map(|m| format!("{}:{}:{}", m.host, m.guest, m.mode.as_str()))
        .collect();

    let live_set: std::collections::BTreeSet<String> = live_mounts
        .iter()
        .map(|m| format!("{}:{}:{}", m.host, m.guest, m.mode))
        .collect();

    if bf_set == live_set {
        return MountDiff {
            old: String::new(),
            new: String::new(),
        };
    }

    MountDiff {
        old: live_set.into_iter().collect::<Vec<_>>().join(", "),
        new: bf_set.into_iter().collect::<Vec<_>>().join(", "),
    }
}

fn diff_packages(bf_packages: &[String], live_packages: &[String]) -> PackageDiff {
    let bf_set: std::collections::BTreeSet<&str> = bf_packages.iter().map(|s| s.as_str()).collect();
    let live_set: std::collections::BTreeSet<&str> =
        live_packages.iter().map(|s| s.as_str()).collect();

    let added: Vec<String> = bf_set
        .difference(&live_set)
        .map(|s| s.to_string())
        .collect();
    let removed: Vec<String> = live_set
        .difference(&bf_set)
        .map(|s| s.to_string())
        .collect();

    PackageDiff { added, removed }
}
