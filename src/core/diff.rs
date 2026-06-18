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
    // Build the exclusion context so diff_mounts can strip distrobox-injected
    // mounts before comparing against the Boxfile [[mounts]] list.
    //
    // box_home: the live container $HOME (from Config.Env HOME=…).  For a custom
    //   --home box this equals the host path that distrobox bind-mounts in; for the
    //   shared-home case it equals the host user's home directory.
    //
    // host_home: the host user's home directory.  Distrobox always bind-mounts it
    //   into the container (at the same path) as a default; it must be excluded
    //   from the [[mounts]] diff regardless of any box.home setting.  We read it
    //   from the process environment (the cbox binary runs as the same user).
    let box_home = live.home.as_deref().unwrap_or("").to_string();
    let host_home = std::env::var("HOME").unwrap_or_default();
    let mount_ctx = MountFilterCtx {
        box_home,
        host_home,
    };
    let mount_diff = diff_mounts(&bf.mounts, &live.mounts, &mount_ctx);
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

    // --- box.home ---
    // Changing --home requires destroy+recreate; distrobox cannot migrate $HOME in place.
    // We recover the live home from Config.Env HOME=… (set by distrobox unconditionally).
    //
    // Empty Boxfile home ("") means "use distrobox default / shared host home" —
    // do NOT compare it against the live HOME string to avoid a spurious recreate
    // on boxes that are correctly on the shared home.
    //
    // If the live home is unrecoverable (None) AND the Boxfile requests a custom
    // home, we emit a warn-class DiffField so the user knows convergence cannot be
    // verified — but we do NOT silently report "no diff" (that is the exact failure
    // mode being fixed). We choose "warn + proceed" over a hard Recreate here
    // because we cannot be sure the box actually differs; a hard Recreate on a
    // false-positive would destroy the user's box data unexpectedly.
    let bf_home = bf.box_config.home.trim();
    if !bf_home.is_empty() {
        match &live.home {
            Some(live_home) => {
                if bf_home != live_home.as_str() {
                    fields.push(DiffField {
                        field: "home".to_string(),
                        old: live_home.clone(),
                        new: bf_home.to_string(),
                        class: "Recreate".to_string(),
                    });
                }
                // else: identical → no diff (correct convergence)
            }
            None => {
                // Live home unrecoverable — warn but do not silently pass.
                // We use a dedicated "Warn" class so callers can surface it
                // without treating it as a hard Recreate trigger.
                fields.push(DiffField {
                    field: "home".to_string(),
                    old: "(unverifiable — inspect returned no HOME env)".to_string(),
                    new: bf_home.to_string(),
                    class: "Warn".to_string(),
                });
            }
        }
    }
    // When bf_home is empty we skip the check entirely — the user has not set a
    // custom home, so whatever distrobox chose is correct.

    // --- box.hostname ---
    // Same Recreate classification: --hostname is fixed at create time.
    // Empty Boxfile hostname ("") means "use distrobox default (box name + -box suffix)"
    // — same pattern as home: skip the check to avoid spurious recreates.
    let bf_hostname = bf.box_config.hostname.trim();
    if !bf_hostname.is_empty() {
        match &live.hostname {
            Some(live_hostname) => {
                if bf_hostname != live_hostname.as_str() {
                    fields.push(DiffField {
                        field: "hostname".to_string(),
                        old: live_hostname.clone(),
                        new: bf_hostname.to_string(),
                        class: "Recreate".to_string(),
                    });
                }
                // else: identical → no diff
            }
            None => {
                // Hostname unrecoverable — warn.
                fields.push(DiffField {
                    field: "hostname".to_string(),
                    old: "(unverifiable — inspect returned no Hostname field)".to_string(),
                    new: bf_hostname.to_string(),
                    class: "Warn".to_string(),
                });
            }
        }
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

/// Extract the image reference to compare against the Boxfile tag.
///
/// Prefers the `cbox.image` label written at create time — that label stores
/// the original Boxfile tag, so comparing tag-to-tag avoids a spurious
/// recreate when the resolved digest (`30ba4450…`) differs from the tag string.
///
/// Falls back to `live.image` (the raw digest/image from the backend inspect)
/// when the label is absent, which preserves the original behaviour for boxes
/// created before the `cbox.image` label was introduced.
fn extract_image_label(live: &InspectResult) -> String {
    live.cbox_image
        .clone()
        .unwrap_or_else(|| live.image.clone())
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

/// Context used to strip distrobox-injected mounts from the live mount set
/// before comparing it against the Boxfile `[[mounts]]` list.
///
/// Fields are parameterized (not hard-coded) so the predicate is portable
/// across different users, UIDs, and box names.
struct MountFilterCtx {
    /// The live container $HOME (from `Config.Env HOME=…`).
    ///
    /// For a custom `box.home` box this is the host path that distrobox
    /// bind-mounts in; for the shared-home case it equals `host_home`.
    /// Either way distrobox owns this mount — it must be excluded from the
    /// `[[mounts]]` diff.
    box_home: String,

    /// The host user's home directory (e.g. `/home/rynaro`).
    ///
    /// Distrobox always bind-mounts the host home into the container at the
    /// same path as a default.  This must be excluded even when `box.home`
    /// points elsewhere (the host home is mounted *in addition*).
    host_home: String,
}

/// Return `true` when `guest` is a destination that distrobox (or another
/// cbox subsystem) manages automatically — i.e. it must NOT be compared
/// against the `[[mounts]]` list.
///
/// Covered categories:
/// - distrobox kernel/dev mounts: `/dev`, `/dev/pts`, `/dev/ptmx`, `/sys`,
///   `/sys/fs/selinux`, `/var/log/journal`, `/tmp`
/// - distrobox network config overlays: `/etc/hosts`, `/etc/resolv.conf`
/// - distrobox host-passthrough prefixes: `/run/host/`, `/run/user/<uid>/`
/// - distrobox helper binaries: `/usr/bin/entrypoint`,
///   `/usr/bin/distrobox-export`, `/usr/bin/distrobox-host-exec`
/// - box.home mount (the `--home` path, governed by `box.home`): `ctx.box_home`
/// - host user home (distrobox default bind-mount): `ctx.host_home`
/// - docker-mode socket (`/var/run/docker.sock`, governed by `docker = "host"`)
///
/// Matching strategy:
/// - Exact match for well-known single-path entries (no prefix overlap risk).
/// - Prefix match only for `/run/host/` and `/run/user/` — scoped tightly to
///   avoid accidental exclusion of legitimate user mounts that share a prefix
///   with a distrobox default.
fn is_distrobox_injected(guest: &str, ctx: &MountFilterCtx) -> bool {
    // ── exact-match distrobox kernel / dev / tmp mounts ──────────────────────
    const EXACT_INJECTED: &[&str] = &[
        "/dev",
        "/dev/pts",
        "/dev/ptmx",
        "/sys",
        "/sys/fs/selinux",
        "/var/log/journal",
        "/tmp",
        "/etc/hosts",
        "/etc/resolv.conf",
        "/usr/bin/entrypoint",
        "/usr/bin/distrobox-export",
        "/usr/bin/distrobox-host-exec",
        // docker-mode socket — governed by `docker = "host"`, not [[mounts]]
        "/var/run/docker.sock",
    ];

    if EXACT_INJECTED.contains(&guest) {
        return true;
    }

    // ── prefix-scoped matches for run/ paths ─────────────────────────────────
    // /run/host/        — distrobox host-passthrough (exact-prefix, trailing /)
    // /run/user/<uid>/  — user runtime dir (exact-prefix, trailing /)
    // We require the trailing slash on the prefix to avoid matching a
    // hypothetical user mount at e.g. "/run/hostfoo" or "/run/users".
    if guest.starts_with("/run/host/") || guest == "/run/host" {
        return true;
    }
    if guest.starts_with("/run/user/") || guest == "/run/user" {
        return true;
    }

    // ── box.home and host home (both parameterized, never hard-coded) ─────────
    // box_home: the --home mount distrobox creates; empty string = unset, skip.
    if !ctx.box_home.is_empty() && guest == ctx.box_home {
        return true;
    }
    // host_home: distrobox always mounts the real host home into the container.
    if !ctx.host_home.is_empty() && guest == ctx.host_home {
        return true;
    }

    false
}

/// Normalize a mount mode string so that the default read-write mode is
/// represented consistently.
///
/// The container runtime (podman/docker) omits an explicit mode string for
/// ordinary read-write bind-mounts — `Mode` comes back as `""` from
/// `podman/docker inspect`, even though `RW=true`.  A Boxfile declaring
/// `mode = "rw"` is semantically identical.  Without normalization the
/// comparison `"" != "rw"` fires a spurious recreate.
///
/// Mapping:
/// - `""` → `"rw"`  (no explicit mode = default r/w)
/// - `"rw"` → `"rw"` (explicit r/w = same default)
/// - `"ro"` → `"ro"` (read-only is the only meaningful non-default)
/// - anything else is returned trimmed and lowercased as-is.
fn normalize_mount_mode(mode: &str) -> String {
    match mode.trim().to_lowercase().as_str() {
        "" | "rw" => "rw".to_string(),
        other => other.to_string(),
    }
}

fn diff_mounts(
    bf_mounts: &[crate::boxfile::model::MountEntry],
    live_mounts: &[MountResult],
    ctx: &MountFilterCtx,
) -> MountDiff {
    // Build canonical "host:guest:mode" tuples for comparison.
    // normalize_mount_mode ensures "" and "rw" are treated identically —
    // the runtime omits Mode for default rw mounts, but the Boxfile declares "rw".
    let bf_set: std::collections::BTreeSet<String> = bf_mounts
        .iter()
        .map(|m| {
            format!(
                "{}:{}:{}",
                m.host,
                m.guest,
                normalize_mount_mode(m.mode.as_str())
            )
        })
        .collect();

    // Filter the live set: drop mounts that distrobox (or another cbox
    // subsystem) injects automatically.  Only the remainder is governed by
    // [[mounts]] and should be compared against the Boxfile list.
    let live_set: std::collections::BTreeSet<String> = live_mounts
        .iter()
        .filter(|m| !is_distrobox_injected(&m.guest, ctx))
        .map(|m| format!("{}:{}:{}", m.host, m.guest, normalize_mount_mode(&m.mode)))
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boxfile::model::Boxfile;

    // Helper: build a minimal InspectResult for diffing.
    fn make_live(image: &str, cbox_image: Option<&str>) -> InspectResult {
        InspectResult {
            name: "testbox".to_string(),
            status: "running".to_string(),
            image: image.to_string(),
            created: "2024-01-01T00:00:00Z".to_string(),
            docker_mode: "none".to_string(),
            mounts: vec![],
            packages: vec![],
            backend: "podman".to_string(),
            id: "abc123".to_string(),
            boxfile_path: None,
            cbox_image: cbox_image.map(|s| s.to_string()),
            home: None,
            hostname: None,
        }
    }

    // Helper: build a minimal Boxfile for diffing.
    fn make_boxfile(image: &str) -> Boxfile {
        Boxfile {
            name: "testbox".to_string(),
            image: image.to_string(),
            docker: crate::boxfile::model::DockerModeField::None,
            packages: vec![],
            mounts: vec![],
            provision: vec![],
            box_config: crate::boxfile::model::BoxConfig::default(),
            sandbox: crate::boxfile::model::SandboxConfig::default(),
        }
    }

    // ─── Fix B: cbox.image label present, tag matches → no image diff ────────

    #[test]
    fn image_label_matches_boxfile_no_diff() {
        // The live container's resolved image is a digest; cbox.image label holds
        // the original tag. The Boxfile still specifies the same tag → no recreate.
        let live = make_live(
            "30ba4450abc123",                       // digest from backend inspect
            Some("docker.io/library/ubuntu:26.04"), // cbox.image label = original tag
        );
        let bf = make_boxfile("docker.io/library/ubuntu:26.04");

        let result = diff_boxfile_vs_live(&bf, &live);

        assert_eq!(result.class, "Incremental", "no image diff expected");
        assert!(
            result.fields.iter().all(|f| f.field != "image"),
            "image field must not appear in diff"
        );
    }

    // ─── Fix B: image genuinely changed → recreate still fires ───────────────

    #[test]
    fn image_genuinely_changed_triggers_recreate() {
        // The user changes the image in the Boxfile to a different tag.
        let live = make_live(
            "30ba4450abc123",
            Some("docker.io/library/ubuntu:26.04"), // label = old tag
        );
        let bf = make_boxfile("docker.io/library/ubuntu:27.10"); // new tag in Boxfile

        let result = diff_boxfile_vs_live(&bf, &live);

        assert_eq!(
            result.class, "Recreate",
            "changed image must trigger recreate"
        );
        let img_field = result.fields.iter().find(|f| f.field == "image");
        assert!(img_field.is_some(), "image diff field must be present");
        let img_field = img_field.unwrap();
        assert_eq!(img_field.old, "docker.io/library/ubuntu:26.04");
        assert_eq!(img_field.new, "docker.io/library/ubuntu:27.10");
    }

    // ─── Fix B: label absent → falls back to digest comparison (old boxes) ───

    #[test]
    fn label_absent_falls_back_to_live_image() {
        // Older box without cbox.image label. If the Boxfile image matches the
        // live image string exactly, no diff is raised.
        let live = make_live("docker.io/library/ubuntu:26.04", None); // no label
        let bf = make_boxfile("docker.io/library/ubuntu:26.04");

        let result = diff_boxfile_vs_live(&bf, &live);

        assert_eq!(result.class, "Incremental");
        assert!(result.fields.iter().all(|f| f.field != "image"));
    }

    #[test]
    fn label_absent_digest_differs_triggers_recreate() {
        // Older box without label, digest in live differs from tag in Boxfile.
        let live = make_live("sha256:30ba4450abc123", None); // no label
        let bf = make_boxfile("docker.io/library/ubuntu:26.04");

        let result = diff_boxfile_vs_live(&bf, &live);

        assert_eq!(result.class, "Recreate");
        assert!(result.fields.iter().any(|f| f.field == "image"));
    }

    // ─── Fix A: resolve_backend routes to correct backend ────────────────────

    #[test]
    fn resolve_backend_uses_override_arg() {
        use crate::dbox::backend::Backend;
        use crate::dbox::mock::{MockResponse, MockRunner};

        // When --backend podman is passed explicitly, resolve_backend must honour it
        // without probing (the MockRunner returns empty lists for all ps calls).
        let runner = MockRunner::new().with_default(MockResponse::ok("[]"));

        let result = crate::core::resolve_backend("mybox", Some("podman"), &runner);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Backend::Podman);
    }

    // ─── Fix #2: home/hostname diff ──────────────────────────────────────────

    /// Build an InspectResult with explicit home and hostname.
    fn make_live_with_home(
        image: &str,
        home: Option<&str>,
        hostname: Option<&str>,
    ) -> InspectResult {
        InspectResult {
            name: "testbox".to_string(),
            status: "running".to_string(),
            image: image.to_string(),
            created: "2024-01-01T00:00:00Z".to_string(),
            docker_mode: "none".to_string(),
            mounts: vec![],
            packages: vec![],
            backend: "podman".to_string(),
            id: "abc123".to_string(),
            boxfile_path: None,
            cbox_image: None,
            home: home.map(|s| s.to_string()),
            hostname: hostname.map(|s| s.to_string()),
        }
    }

    /// Build a Boxfile with explicit home and hostname (other fields minimal).
    fn make_boxfile_with_home(image: &str, home: &str, hostname: &str) -> Boxfile {
        Boxfile {
            name: "testbox".to_string(),
            image: image.to_string(),
            docker: crate::boxfile::model::DockerModeField::None,
            packages: vec![],
            mounts: vec![],
            provision: vec![],
            box_config: crate::boxfile::model::BoxConfig {
                home: home.to_string(),
                hostname: hostname.to_string(),
                pull: false,
            },
            sandbox: crate::boxfile::model::SandboxConfig::default(),
        }
    }

    // identical home/hostname → no diff (must NOT fire a spurious recreate)
    #[test]
    fn home_identical_no_diff() {
        let live = make_live_with_home(
            "ubuntu:22.04",
            Some("/home/rynaro/.cbox-homes/testbox"),
            Some("testbox-box"),
        );
        let bf = make_boxfile_with_home(
            "ubuntu:22.04",
            "/home/rynaro/.cbox-homes/testbox",
            "testbox-box",
        );
        let result = diff_boxfile_vs_live(&bf, &live);
        assert_eq!(
            result.class, "Incremental",
            "identical home/hostname must not diff"
        );
        assert!(
            result
                .fields
                .iter()
                .all(|f| f.field != "home" && f.field != "hostname"),
            "no home/hostname fields must appear when unchanged"
        );
    }

    // changed home → Recreate
    #[test]
    fn home_changed_triggers_recreate() {
        let live = make_live_with_home(
            "ubuntu:22.04",
            Some("/home/rynaro"), // live: shared home
            Some("testbox-box"),
        );
        let bf = make_boxfile_with_home(
            "ubuntu:22.04",
            "/home/rynaro/.cbox-homes/testbox", // bf: custom isolated home
            "testbox-box",
        );
        let result = diff_boxfile_vs_live(&bf, &live);
        assert_eq!(
            result.class, "Recreate",
            "changed home must trigger recreate"
        );
        let home_field = result.fields.iter().find(|f| f.field == "home");
        assert!(home_field.is_some(), "home diff field must be present");
        let home_field = home_field.unwrap();
        assert_eq!(home_field.class, "Recreate");
        assert_eq!(home_field.old, "/home/rynaro");
        assert_eq!(home_field.new, "/home/rynaro/.cbox-homes/testbox");
    }

    // changed hostname → Recreate
    #[test]
    fn hostname_changed_triggers_recreate() {
        let live = make_live_with_home(
            "ubuntu:22.04",
            Some("/home/rynaro/.cbox-homes/testbox"),
            Some("old-hostname"),
        );
        let bf = make_boxfile_with_home(
            "ubuntu:22.04",
            "/home/rynaro/.cbox-homes/testbox",
            "new-hostname",
        );
        let result = diff_boxfile_vs_live(&bf, &live);
        assert_eq!(
            result.class, "Recreate",
            "changed hostname must trigger recreate"
        );
        let host_field = result.fields.iter().find(|f| f.field == "hostname");
        assert!(host_field.is_some(), "hostname diff field must be present");
        let host_field = host_field.unwrap();
        assert_eq!(host_field.class, "Recreate");
        assert_eq!(host_field.old, "old-hostname");
        assert_eq!(host_field.new, "new-hostname");
    }

    // empty Boxfile home ("") → skip check entirely (no spurious diff against
    // a live box that is correctly on the shared host home)
    #[test]
    fn empty_boxfile_home_no_diff() {
        let live = make_live_with_home(
            "ubuntu:22.04",
            Some("/home/rynaro"), // distrobox default: shared host home
            Some("testbox-box"),
        );
        // box_config.home = "" means user did not set a custom home
        let bf = make_boxfile_with_home("ubuntu:22.04", "", "");
        let result = diff_boxfile_vs_live(&bf, &live);
        assert_eq!(
            result.class, "Incremental",
            "empty Boxfile home must not diff"
        );
        assert!(
            result
                .fields
                .iter()
                .all(|f| f.field != "home" && f.field != "hostname"),
            "no home/hostname field when Boxfile leaves them empty"
        );
    }

    // live home unrecoverable (None) + Boxfile has a custom home → Warn (not silent skip)
    #[test]
    fn live_home_unrecoverable_warns_not_silent() {
        let live = make_live_with_home(
            "ubuntu:22.04",
            None, // unrecoverable — no Env array in inspect
            None,
        );
        let bf = make_boxfile_with_home(
            "ubuntu:22.04",
            "/home/rynaro/.cbox-homes/testbox",
            "testbox-box",
        );
        let result = diff_boxfile_vs_live(&bf, &live);

        // Must NOT be silently clean — there must be a home warning field
        let home_field = result.fields.iter().find(|f| f.field == "home");
        assert!(
            home_field.is_some(),
            "a home warn-field must be emitted when live home is unrecoverable"
        );
        let home_field = home_field.unwrap();
        assert_eq!(
            home_field.class, "Warn",
            "unrecoverable home must be Warn, not Recreate"
        );

        // Warn alone must NOT promote DiffResult.class to Recreate
        assert_eq!(
            result.class, "Incremental",
            "Warn fields alone must not promote class to Recreate"
        );
    }

    // ─── Fix #4: mounts diff excludes distrobox-injected mounts ─────────────

    /// Helper: build a MountFilterCtx with explicit box_home and host_home.
    fn make_mount_ctx(box_home: &str, host_home: &str) -> MountFilterCtx {
        MountFilterCtx {
            box_home: box_home.to_string(),
            host_home: host_home.to_string(),
        }
    }

    /// Helper: build a MountResult (live mount entry).
    fn live_mount(host: &str, guest: &str, mode: &str) -> MountResult {
        MountResult {
            host: host.to_string(),
            guest: guest.to_string(),
            mode: mode.to_string(),
        }
    }

    /// Helper: build a MountEntry (Boxfile [[mounts]] entry).
    fn bf_mount(host: &str, guest: &str, mode: &str) -> crate::boxfile::model::MountEntry {
        use crate::boxfile::model::MountMode;
        crate::boxfile::model::MountEntry {
            host: host.to_string(),
            guest: guest.to_string(),
            mode: if mode == "ro" {
                MountMode::Ro
            } else {
                MountMode::Rw
            },
        }
    }

    /// Build the full set of distrobox-injected live mounts that a typical box
    /// would have, plus the one declared [[mounts]] entry and the box.home mount.
    fn typical_injected_mounts(box_home: &str, host_home: &str, uid: u32) -> Vec<MountResult> {
        vec![
            // distrobox kernel / dev / tmp mounts
            live_mount("/dev", "/dev", "rw"),
            live_mount("/dev/pts", "/dev/pts", "rw"),
            live_mount("/dev/ptmx", "/dev/ptmx", "rw"),
            live_mount("/sys", "/sys", "ro"),
            live_mount("/sys/fs/selinux", "/sys/fs/selinux", "ro"),
            live_mount("/var/log/journal", "/var/log/journal", "ro"),
            live_mount("/tmp", "/tmp", "rw"),
            // network config overlays
            live_mount("/etc/hosts", "/etc/hosts", "ro"),
            live_mount("/etc/resolv.conf", "/etc/resolv.conf", "rw"),
            // host-passthrough
            live_mount("/run/host", "/run/host/", "rw"),
            live_mount(
                &format!("/run/user/{uid}"),
                &format!("/run/user/{uid}"),
                "rw",
            ),
            // distrobox helper binaries
            live_mount("/usr/bin/distrobox-host-exec", "/usr/bin/entrypoint", "ro"),
            live_mount(
                "/usr/bin/distrobox-export",
                "/usr/bin/distrobox-export",
                "ro",
            ),
            live_mount(
                "/usr/bin/distrobox-host-exec",
                "/usr/bin/distrobox-host-exec",
                "ro",
            ),
            // docker socket — governed by docker="host", not [[mounts]]
            live_mount("/var/run/docker.sock", "/var/run/docker.sock", "rw"),
            // host home — distrobox default bind-mount
            live_mount(host_home, host_home, "rw"),
            // box.home mount (--home path) — governed by box.home
            live_mount(box_home, box_home, "rw"),
        ]
    }

    // Bug-fix test: live set full of injected defaults + the one declared mount
    // present → NO mounts diff (the core bug being fixed).
    #[test]
    fn mounts_injected_defaults_plus_declared_no_diff() {
        let box_home = "/home/rynaro/.cbox-homes/electionbuddy";
        let host_home = "/home/rynaro";
        let uid: u32 = 1000;

        // The one user-declared mount
        let declared_host = "/home/rynaro/workspace/electionbuddy";
        let declared_guest = "/home/rynaro/workspace/electionbuddy";

        let mut live_mounts = typical_injected_mounts(box_home, host_home, uid);
        // Add the declared mount to the live set (it is present in the container)
        live_mounts.push(live_mount(declared_host, declared_guest, "rw"));

        let bf_mounts = vec![bf_mount(declared_host, declared_guest, "rw")];
        let ctx = make_mount_ctx(box_home, host_home);

        let diff = diff_mounts(&bf_mounts, &live_mounts, &ctx);
        assert!(
            diff.is_empty(),
            "no mounts diff expected when only declared mount is present alongside injected defaults; \
             got old={:?} new={:?}",
            diff.old, diff.new
        );
    }

    // A [[mounts]] entry declared in Boxfile but absent from the live set → Recreate.
    #[test]
    fn mounts_declared_absent_from_live_triggers_recreate() {
        let box_home = "/home/rynaro/.cbox-homes/electionbuddy";
        let host_home = "/home/rynaro";
        let uid: u32 = 1000;

        // Live set: only injected mounts — the declared mount is NOT present.
        let live_mounts = typical_injected_mounts(box_home, host_home, uid);

        let bf_mounts = vec![bf_mount(
            "/home/rynaro/workspace/electionbuddy",
            "/home/rynaro/workspace/electionbuddy",
            "rw",
        )];
        let ctx = make_mount_ctx(box_home, host_home);

        let diff = diff_mounts(&bf_mounts, &live_mounts, &ctx);
        assert!(
            !diff.is_empty(),
            "mounts diff must fire when a declared [[mounts]] entry is absent from live"
        );
    }

    // A non-injected mount present in live but NOT declared in [[mounts]]
    // (simulates a removed declared mount) → Recreate.
    #[test]
    fn mounts_live_has_extra_non_injected_mount_triggers_recreate() {
        let box_home = "/home/rynaro/.cbox-homes/electionbuddy";
        let host_home = "/home/rynaro";
        let uid: u32 = 1000;

        let mut live_mounts = typical_injected_mounts(box_home, host_home, uid);
        // A mount that was once declared but is no longer in the Boxfile.
        live_mounts.push(live_mount(
            "/home/rynaro/workspace/old-project",
            "/home/rynaro/workspace/old-project",
            "rw",
        ));

        // Boxfile has NO [[mounts]] now (user removed the entry).
        let bf_mounts: Vec<crate::boxfile::model::MountEntry> = vec![];
        let ctx = make_mount_ctx(box_home, host_home);

        let diff = diff_mounts(&bf_mounts, &live_mounts, &ctx);
        assert!(
            !diff.is_empty(),
            "mounts diff must fire when a non-injected live mount is not in [[mounts]]"
        );
    }

    // A declared mount with a changed mode (rw → ro) → diff fires.
    #[test]
    fn mounts_mode_change_triggers_diff() {
        let box_home = "/home/rynaro/.cbox-homes/electionbuddy";
        let host_home = "/home/rynaro";
        let uid: u32 = 1000;

        let declared_host = "/home/rynaro/workspace/electionbuddy";
        let declared_guest = "/home/rynaro/workspace/electionbuddy";

        let mut live_mounts = typical_injected_mounts(box_home, host_home, uid);
        // Live: rw
        live_mounts.push(live_mount(declared_host, declared_guest, "rw"));

        // Boxfile: ro (changed by the user)
        let bf_mounts = vec![bf_mount(declared_host, declared_guest, "ro")];
        let ctx = make_mount_ctx(box_home, host_home);

        let diff = diff_mounts(&bf_mounts, &live_mounts, &ctx);
        assert!(
            !diff.is_empty(),
            "mounts diff must fire when a declared mount changes mode (rw→ro)"
        );
    }

    // box.home mount and docker.sock present in live but not in [[mounts]]
    // → no diff (correctly excluded).
    #[test]
    fn mounts_box_home_and_docker_sock_excluded() {
        let box_home = "/home/rynaro/.cbox-homes/electionbuddy";
        let host_home = "/home/rynaro";

        // Live: only the box.home, host home, and docker socket — no user mounts.
        let live_mounts = vec![
            live_mount(box_home, box_home, "/var/run/docker.sock"),
            live_mount(host_home, host_home, "rw"),
            live_mount(box_home, box_home, "rw"),
            live_mount("/var/run/docker.sock", "/var/run/docker.sock", "rw"),
        ];

        // Boxfile: no [[mounts]] entries.
        let bf_mounts: Vec<crate::boxfile::model::MountEntry> = vec![];
        let ctx = make_mount_ctx(box_home, host_home);

        let diff = diff_mounts(&bf_mounts, &live_mounts, &ctx);
        assert!(
            diff.is_empty(),
            "box.home and docker.sock must be excluded; expected no diff but got \
             old={:?} new={:?}",
            diff.old,
            diff.new
        );
    }

    // Verify is_distrobox_injected does not drop a legitimate user mount that
    // merely shares a common path prefix with a distrobox default.
    // e.g. a user mounts /run/myapp — must NOT be excluded just because the
    // distrobox rule covers /run/host/ and /run/user/.
    #[test]
    fn mounts_user_mount_with_run_prefix_not_excluded() {
        let ctx = make_mount_ctx("/home/user/.cbox-homes/box", "/home/user");

        // These should NOT be treated as injected:
        assert!(
            !is_distrobox_injected("/run/myapp", &ctx),
            "/run/myapp must not be excluded (no trailing-slash match)"
        );
        assert!(
            !is_distrobox_injected("/run/hostfoo", &ctx),
            "/run/hostfoo must not be excluded (not an exact /run/host match)"
        );
        assert!(
            !is_distrobox_injected("/run/users", &ctx),
            "/run/users must not be excluded (not an exact /run/user match)"
        );
        assert!(
            !is_distrobox_injected("/dev/shm", &ctx),
            "/dev/shm is not in the exact-match list and must not be excluded"
        );

        // These SHOULD be treated as injected (sanity check):
        assert!(
            is_distrobox_injected("/run/host", &ctx),
            "/run/host must be excluded"
        );
        assert!(
            is_distrobox_injected("/run/host/foo", &ctx),
            "/run/host/foo must be excluded (prefix)"
        );
        assert!(
            is_distrobox_injected("/run/user/1000", &ctx),
            "/run/user/1000 must be excluded (prefix)"
        );
        assert!(is_distrobox_injected("/dev", &ctx), "/dev must be excluded");
    }

    // ─── Mount-mode normalization (fix: empty mode == "rw") ─────────────────

    // live mode "" vs Boxfile "rw" → no diff (the bug being fixed).
    #[test]
    fn mount_mode_empty_live_vs_rw_boxfile_no_diff() {
        let ctx = make_mount_ctx("/home/user/.cbox-homes/box", "/home/user");
        // Live runtime returns Mode="" for a default rw bind-mount.
        let live_mounts = vec![live_mount(
            "/home/user/workspace/project",
            "/home/user/workspace/project",
            "", // empty Mode — the bug scenario
        )];
        let bf_mounts = vec![bf_mount(
            "/home/user/workspace/project",
            "/home/user/workspace/project",
            "rw", // Boxfile declares explicit "rw"
        )];
        let diff = diff_mounts(&bf_mounts, &live_mounts, &ctx);
        assert!(
            diff.is_empty(),
            "empty live mode must be treated as rw — no diff expected; \
             got old={:?} new={:?}",
            diff.old,
            diff.new
        );
    }

    // live "rw" vs Boxfile "rw" → no diff (baseline sanity).
    #[test]
    fn mount_mode_rw_vs_rw_no_diff() {
        let ctx = make_mount_ctx("/home/user/.cbox-homes/box", "/home/user");
        let live_mounts = vec![live_mount(
            "/home/user/workspace/project",
            "/home/user/workspace/project",
            "rw",
        )];
        let bf_mounts = vec![bf_mount(
            "/home/user/workspace/project",
            "/home/user/workspace/project",
            "rw",
        )];
        let diff = diff_mounts(&bf_mounts, &live_mounts, &ctx);
        assert!(
            diff.is_empty(),
            "rw vs rw must produce no diff; got old={:?} new={:?}",
            diff.old,
            diff.new
        );
    }

    // live "" vs Boxfile "ro" → Recreate (genuine ro intent).
    #[test]
    fn mount_mode_empty_live_vs_ro_boxfile_triggers_diff() {
        let ctx = make_mount_ctx("/home/user/.cbox-homes/box", "/home/user");
        let live_mounts = vec![live_mount(
            "/home/user/workspace/project",
            "/home/user/workspace/project",
            "", // empty = rw by default
        )];
        let bf_mounts = vec![bf_mount(
            "/home/user/workspace/project",
            "/home/user/workspace/project",
            "ro", // user now wants read-only
        )];
        let diff = diff_mounts(&bf_mounts, &live_mounts, &ctx);
        assert!(
            !diff.is_empty(),
            "empty live mode (=rw) vs ro Boxfile must trigger recreate"
        );
    }

    // live "rw" vs Boxfile "ro" → Recreate (genuine ro intent).
    #[test]
    fn mount_mode_rw_live_vs_ro_boxfile_triggers_diff() {
        let ctx = make_mount_ctx("/home/user/.cbox-homes/box", "/home/user");
        let live_mounts = vec![live_mount(
            "/home/user/workspace/project",
            "/home/user/workspace/project",
            "rw",
        )];
        let bf_mounts = vec![bf_mount(
            "/home/user/workspace/project",
            "/home/user/workspace/project",
            "ro",
        )];
        let diff = diff_mounts(&bf_mounts, &live_mounts, &ctx);
        assert!(
            !diff.is_empty(),
            "rw live vs ro Boxfile must trigger recreate (genuine mode change)"
        );
    }

    // live "ro" vs Boxfile "rw" → Recreate (genuine rw intent from ro live).
    #[test]
    fn mount_mode_ro_live_vs_rw_boxfile_triggers_diff() {
        let ctx = make_mount_ctx("/home/user/.cbox-homes/box", "/home/user");
        let live_mounts = vec![live_mount(
            "/home/user/workspace/project",
            "/home/user/workspace/project",
            "ro",
        )];
        let bf_mounts = vec![bf_mount(
            "/home/user/workspace/project",
            "/home/user/workspace/project",
            "rw",
        )];
        let diff = diff_mounts(&bf_mounts, &live_mounts, &ctx);
        assert!(
            !diff.is_empty(),
            "ro live vs rw Boxfile must trigger recreate (genuine mode change)"
        );
    }

    // Full diff_boxfile_vs_live integration: live has injected defaults + one
    // declared mount → class must be Incremental (no mounts field).
    // Uses InspectResult.home for box_home context (as the real code does).
    //
    // We use the process's actual $HOME as the host_home value so the
    // host-home exclusion logic works correctly regardless of the test
    // execution environment (container or bare host).
    #[test]
    fn diff_boxfile_vs_live_mounts_no_spurious_recreate() {
        use crate::boxfile::model::{
            BoxConfig, DockerModeField, MountEntry, MountMode, SandboxConfig,
        };

        // box_home is a custom --home path, distinct from the host home.
        // live.home is set to this value, which is how diff_boxfile_vs_live
        // learns the box_home for the exclusion filter.
        let box_home_path = "/cbox-test/home/electionbuddy";

        // Use the actual $HOME of the test process so that the host-home
        // exclusion works in whatever environment (dev container or CI).
        let host_home_path = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        let uid: u32 = 9999;

        // Build the injected-defaults live mount set.
        // typical_injected_mounts includes a host_home_path -> host_home_path mount
        // which must be excluded, plus the box.home mount.
        let mut live_mounts = typical_injected_mounts(box_home_path, &host_home_path, uid);
        // Add the one user-declared mount (present in live — correctly converged).
        live_mounts.push(live_mount(
            "/cbox-test/workspace/project",
            "/cbox-test/workspace/project",
            "rw",
        ));

        let live = InspectResult {
            name: "electionbuddy".to_string(),
            status: "running".to_string(),
            image: "ubuntu:22.04".to_string(),
            created: "2024-01-01T00:00:00Z".to_string(),
            docker_mode: "none".to_string(),
            mounts: live_mounts,
            packages: vec![],
            backend: "podman".to_string(),
            id: "abc123".to_string(),
            boxfile_path: None,
            cbox_image: Some("ubuntu:22.04".to_string()),
            // live.home drives box_home in diff_boxfile_vs_live
            home: Some(box_home_path.to_string()),
            hostname: None,
        };

        let bf = Boxfile {
            name: "electionbuddy".to_string(),
            image: "ubuntu:22.04".to_string(),
            docker: DockerModeField::None,
            packages: vec![],
            mounts: vec![MountEntry {
                host: "/cbox-test/workspace/project".to_string(),
                guest: "/cbox-test/workspace/project".to_string(),
                mode: MountMode::Rw,
            }],
            provision: vec![],
            box_config: BoxConfig::default(),
            sandbox: SandboxConfig::default(),
        };

        let result = diff_boxfile_vs_live(&bf, &live);

        assert!(
            result.fields.iter().all(|f| f.field != "mounts"),
            "mounts field must not appear in diff when only declared mount is present \
             alongside injected defaults; fields={:?}",
            result.fields
        );
    }
}
