//! Distro-family detection from image strings.
//!
//! Pure: no I/O, no side effects. Used by docker_mode_flags to select
//! per-family package names that the target image's package manager can resolve.

/// The package-manager family of a container image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistroFamily {
    /// RPM-based (Fedora, RHEL, CentOS, Rocky, Alma, Toolbox, …).
    Rpm,
    /// APT-based (Ubuntu, Debian, Mint, …).
    Debian,
    /// APK-based (Alpine).
    Alpine,
    /// Pacman-based (Arch, Manjaro, …).
    Arch,
    /// Zypper-based (openSUSE, SLES, …).
    Suse,
    /// Unknown / cannot be determined from the image string alone.
    Unknown,
}

/// Infer the distro family from an image reference string using substring
/// heuristics on the lowercased image name.  The function is intentionally
/// conservative: it only returns a specific family when one of the well-known
/// substrings matches; everything else is `Unknown`.
pub fn detect_family(image: &str) -> DistroFamily {
    let lower = image.to_lowercase();

    // RPM families — check before generic terms that could overlap.
    if lower.contains("fedora")
        || lower.contains("rhel")
        || lower.contains("centos")
        || lower.contains("rocky")
        || lower.contains("alma")
        || lower.contains("toolbox")
        || lower.contains("ubi8")
        || lower.contains("ubi9")
    {
        return DistroFamily::Rpm;
    }

    // Debian/Ubuntu families.
    if lower.contains("ubuntu")
        || lower.contains("debian")
        || lower.contains("mint")
        || lower.contains("linuxmint")
        || lower.contains("kali")
        || lower.contains("pop-os")
        || lower.contains("popos")
    {
        return DistroFamily::Debian;
    }

    // Alpine — must come before broader patterns.
    if lower.contains("alpine") {
        return DistroFamily::Alpine;
    }

    // Arch families.
    if lower.contains("archlinux")
        || lower.contains("arch-linux")
        || lower.contains("/arch:")
        || lower.contains("/arch@")
        || lower.ends_with("/arch")
        || lower.contains("manjaro")
        || lower.contains("endeavouros")
        || lower.contains("garuda")
    {
        return DistroFamily::Arch;
    }

    // openSUSE / SLES.
    if lower.contains("opensuse")
        || lower.contains("suse")
        || lower.contains("sles")
        || lower.contains("leap")
        || lower.contains("tumbleweed")
    {
        return DistroFamily::Suse;
    }

    DistroFamily::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Rpm ─────────────────────────────────────────────────────────────────

    #[test]
    fn fedora_toolbox_registry() {
        assert_eq!(
            detect_family("registry.fedoraproject.org/fedora-toolbox:latest"),
            DistroFamily::Rpm
        );
    }

    #[test]
    fn fedora_plain() {
        assert_eq!(detect_family("fedora:40"), DistroFamily::Rpm);
    }

    #[test]
    fn rhel_ubi() {
        assert_eq!(
            detect_family("registry.access.redhat.com/ubi9/ubi:latest"),
            DistroFamily::Rpm
        );
    }

    #[test]
    fn centos_stream() {
        assert_eq!(
            detect_family("quay.io/centos/centos:stream9"),
            DistroFamily::Rpm
        );
    }

    #[test]
    fn rocky_linux() {
        assert_eq!(
            detect_family("docker.io/rockylinux/rockylinux:9"),
            DistroFamily::Rpm
        );
    }

    #[test]
    fn almalinux() {
        assert_eq!(
            detect_family("docker.io/library/almalinux:9"),
            DistroFamily::Rpm
        );
    }

    // ─── Debian ──────────────────────────────────────────────────────────────

    #[test]
    fn ubuntu_docker_io() {
        // Spec example: docker.io/library/ubuntu:26.04
        assert_eq!(
            detect_family("docker.io/library/ubuntu:26.04"),
            DistroFamily::Debian
        );
    }

    #[test]
    fn debian_slim() {
        assert_eq!(
            detect_family("docker.io/library/debian:bookworm-slim"),
            DistroFamily::Debian
        );
    }

    #[test]
    fn ubuntu_plain() {
        assert_eq!(detect_family("ubuntu:22.04"), DistroFamily::Debian);
    }

    // ─── Alpine ──────────────────────────────────────────────────────────────

    #[test]
    fn alpine_plain() {
        assert_eq!(detect_family("alpine:3.19"), DistroFamily::Alpine);
    }

    #[test]
    fn alpine_docker_io() {
        assert_eq!(
            detect_family("docker.io/library/alpine:latest"),
            DistroFamily::Alpine
        );
    }

    // ─── Arch ────────────────────────────────────────────────────────────────

    #[test]
    fn arch_linux() {
        assert_eq!(
            detect_family("docker.io/archlinux/archlinux:latest"),
            DistroFamily::Arch
        );
    }

    #[test]
    fn manjaro() {
        assert_eq!(
            detect_family("docker.io/manjarolinux/manjaro:latest"),
            DistroFamily::Arch
        );
    }

    // ─── Suse ────────────────────────────────────────────────────────────────

    #[test]
    fn opensuse_tumbleweed() {
        assert_eq!(
            detect_family("registry.opensuse.org/opensuse/tumbleweed:latest"),
            DistroFamily::Suse
        );
    }

    #[test]
    fn opensuse_leap() {
        assert_eq!(
            detect_family("registry.opensuse.org/opensuse/leap:15.5"),
            DistroFamily::Suse
        );
    }

    // ─── Unknown ─────────────────────────────────────────────────────────────

    #[test]
    fn unknown_generic() {
        assert_eq!(
            detect_family("my-custom-image:latest"),
            DistroFamily::Unknown
        );
    }

    #[test]
    fn unknown_void_linux() {
        // Void Linux is not in the heuristic set — Unknown is the safe default.
        assert_eq!(
            detect_family("docker.io/voidlinux/voidlinux:latest"),
            DistroFamily::Unknown
        );
    }

    #[test]
    fn unknown_empty() {
        assert_eq!(detect_family(""), DistroFamily::Unknown);
    }
}
