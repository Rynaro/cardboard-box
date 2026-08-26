//! Verified self-update from the prebuilt archives attached to the latest GitHub release.

use std::fs::{self, File, OpenOptions};
use std::io::Cursor;
use std::path::{Component, Path};
use std::process::Command;

use clap::Args;
use flate2::read::GzDecoder;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::cli::output::OutputCtx;
use crate::error::CboxError;

const RELEASE_API: &str = "https://api.github.com/repos/Rynaro/cardboard-box/releases/latest";

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Only report whether a newer release is available.
    #[arg(long)]
    pub check: bool,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct UpdateReport {
    ok: bool,
    current_version: String,
    latest_version: String,
    update_available: bool,
    updated: bool,
    dry_run: bool,
    target: String,
}

trait Fetcher {
    fn get(&self, url: &str) -> Result<Vec<u8>, CboxError>;
}

struct CurlFetcher;

impl Fetcher for CurlFetcher {
    fn get(&self, url: &str) -> Result<Vec<u8>, CboxError> {
        let output = Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--silent",
                "--show-error",
                "--max-time",
                "30",
            ])
            .args(["--header", "Accept: application/vnd.github+json"])
            .args(["--header", "User-Agent: cbox-self-update"])
            .arg(url)
            .output()
            .map_err(|e| {
                CboxError::software(format!("cannot run curl (required for cbox update): {e}"))
            })?;
        if !output.status.success() {
            return Err(CboxError::tempfail(format!(
                "failed to download update: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    }
}

trait Installer {
    fn install(&self, archive: &[u8], version: &str, target: &str) -> Result<(), CboxError>;
}

struct CurrentExeInstaller;

impl Installer for CurrentExeInstaller {
    fn install(&self, archive: &[u8], version: &str, target: &str) -> Result<(), CboxError> {
        let destination = std::env::current_exe().map_err(|e| CboxError::ioerr(e.to_string()))?;
        install_verified_archive(archive, version, target, &destination)
    }
}

pub fn run(args: &UpdateArgs, dry_run: bool, ctx: &OutputCtx) -> Result<(), CboxError> {
    let report = execute(
        args,
        dry_run,
        env!("CARGO_PKG_VERSION"),
        &release_target()?,
        &CurlFetcher,
        &CurrentExeInstaller,
    )?;
    render(ctx, &report);
    Ok(())
}

fn execute(
    args: &UpdateArgs,
    dry_run: bool,
    current: &str,
    target: &str,
    fetcher: &dyn Fetcher,
    installer: &dyn Installer,
) -> Result<UpdateReport, CboxError> {
    let release: Release = serde_json::from_slice(&fetcher.get(RELEASE_API)?)
        .map_err(|e| CboxError::tempfail(format!("invalid GitHub release metadata: {e}")))?;
    let latest = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let available = version_is_newer(latest, current)?;
    let mut report = UpdateReport {
        ok: true,
        current_version: current.into(),
        latest_version: latest.into(),
        update_available: available,
        updated: false,
        dry_run,
        target: target.into(),
    };
    if args.check || dry_run || !available {
        return Ok(report);
    }

    let archive_name = format!("cbox-{latest}-{target}.tar.gz");
    let archive = fetcher.get(asset_url(&release, &archive_name)?)?;
    let sums = fetcher.get(asset_url(&release, "SHA256SUMS")?)?;
    verify_archive(&archive_name, &archive, &sums)?;
    installer.install(&archive, latest, target)?;
    report.updated = true;
    Ok(report)
}

fn render(ctx: &OutputCtx, report: &UpdateReport) {
    if ctx.json {
        ctx.print_json(report);
    } else if report.updated {
        ctx.success(&format!(
            "updated cbox {} -> {}",
            report.current_version, report.latest_version
        ));
    } else if report.update_available && report.dry_run {
        ctx.hint(&format!(
            "would update cbox {} -> {} ({})",
            report.current_version, report.latest_version, report.target
        ));
    } else if report.update_available {
        ctx.hint(&format!(
            "cbox {} is available (current: {})",
            report.latest_version, report.current_version
        ));
    } else {
        ctx.success(&format!("cbox {} is up to date", report.current_version));
    }
}

fn asset_url<'a>(release: &'a Release, name: &str) -> Result<&'a str, CboxError> {
    release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .map(|asset| asset.browser_download_url.as_str())
        .ok_or_else(|| CboxError::tempfail(format!("release asset {name} was not published")))
}

fn release_target() -> Result<String, CboxError> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => {
            return Err(CboxError::software(format!(
                "self-update is not available for architecture {other}"
            )))
        }
    };
    if std::env::consts::OS != "linux" {
        return Err(CboxError::software(
            "self-update is currently available only on Linux",
        ));
    }
    let env = if cfg!(target_env = "musl") {
        "musl"
    } else {
        "gnu"
    };
    Ok(format!("{arch}-unknown-linux-{env}"))
}

fn version_is_newer(candidate: &str, current: &str) -> Result<bool, CboxError> {
    let candidate = Version::parse(candidate).map_err(|e| {
        CboxError::tempfail(format!("release has invalid version {candidate}: {e}"))
    })?;
    let current = Version::parse(current)
        .map_err(|e| CboxError::software(format!("cbox has invalid embedded version: {e}")))?;
    Ok(candidate > current)
}

fn verify_archive(name: &str, archive: &[u8], sums: &[u8]) -> Result<(), CboxError> {
    let manifest = std::str::from_utf8(sums)
        .map_err(|_| CboxError::tempfail("SHA256SUMS is not valid UTF-8"))?;
    let expected = manifest
        .lines()
        .find_map(|line| {
            let (hash, file) = line.split_once(char::is_whitespace)?;
            (file.trim_start_matches([' ', '*']) == name).then_some(hash)
        })
        .ok_or_else(|| CboxError::tempfail(format!("SHA256SUMS has no entry for {name}")))?;
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CboxError::tempfail(format!(
            "invalid SHA-256 entry for {name}"
        )));
    }
    let actual = format!("{:x}", Sha256::digest(archive));
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(CboxError::tempfail(format!(
            "checksum verification failed for {name}"
        )));
    }
    Ok(())
}

fn extract_binary(archive: &[u8], root: &str, output: &Path) -> Result<(), CboxError> {
    let expected = Path::new(root).join("cbox");
    let mut found = false;
    let mut tar = tar::Archive::new(GzDecoder::new(Cursor::new(archive)));
    for item in tar
        .entries()
        .map_err(|e| CboxError::tempfail(format!("invalid update archive: {e}")))?
    {
        let mut entry =
            item.map_err(|e| CboxError::tempfail(format!("invalid update archive: {e}")))?;
        let path = entry
            .path()
            .map_err(|e| CboxError::tempfail(format!("invalid archive path: {e}")))?;
        if path.is_absolute()
            || path.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
            || !path.starts_with(root)
        {
            return Err(CboxError::tempfail(format!(
                "unsafe archive member {}",
                path.display()
            )));
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            return Err(CboxError::tempfail(format!(
                "unsafe archive member type for {}",
                path.display()
            )));
        }
        if path == expected {
            if found || !kind.is_file() {
                return Err(CboxError::tempfail(
                    "update archive has an invalid cbox member",
                ));
            }
            entry
                .unpack(output)
                .map_err(|e| CboxError::ioerr(e.to_string()))?;
            found = true;
        }
    }
    if !found {
        return Err(CboxError::tempfail("update archive did not contain cbox"));
    }
    Ok(())
}

fn install_verified_archive(
    archive: &[u8],
    version: &str,
    target: &str,
    destination: &Path,
) -> Result<(), CboxError> {
    let parent = destination
        .parent()
        .ok_or_else(|| CboxError::ioerr("running executable has no parent directory"))?;
    let staged = parent.join(format!(".cbox-update-{}", std::process::id()));
    let result = (|| {
        drop(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staged)?,
        );
        extract_binary(archive, &format!("cbox-{version}-{target}"), &staged)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&staged)?
            .sync_all()?;
        fs::rename(&staged, destination)?;
        File::open(parent)?.sync_all()?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result.map_err(|e| {
        CboxError::ioerr(format!(
            "could not replace {}: {e} (try with sufficient permissions)",
            destination.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::io::Read;

    struct FakeFetcher {
        responses: RefCell<Vec<Result<Vec<u8>, CboxError>>>,
        calls: Cell<usize>,
    }
    impl Fetcher for FakeFetcher {
        fn get(&self, _: &str) -> Result<Vec<u8>, CboxError> {
            self.calls.set(self.calls.get() + 1);
            self.responses.borrow_mut().remove(0)
        }
    }
    struct FakeInstaller {
        calls: Cell<usize>,
    }
    impl Installer for FakeInstaller {
        fn install(&self, _: &[u8], _: &str, _: &str) -> Result<(), CboxError> {
            self.calls.set(self.calls.get() + 1);
            Ok(())
        }
    }

    fn release(version: &str, archive: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let name = format!("cbox-{version}-x86_64-unknown-linux-gnu.tar.gz");
        let json = serde_json::json!({"tag_name": format!("v{version}"), "assets": [
            {"name": name, "browser_download_url": "archive"},
            {"name": "SHA256SUMS", "browser_download_url": "sums"}
        ]});
        let sums = format!("{:x}  {name}\n", Sha256::digest(archive)).into_bytes();
        (serde_json::to_vec(&json).unwrap(), sums)
    }
    fn fake(responses: Vec<Result<Vec<u8>, CboxError>>) -> FakeFetcher {
        FakeFetcher {
            responses: RefCell::new(responses),
            calls: Cell::new(0),
        }
    }

    #[test]
    fn semver_comparison_observes_prerelease_and_build() {
        assert!(version_is_newer("1.0.0", "1.0.0-rc.1").unwrap());
        assert!(version_is_newer("1.0.0-rc.10", "1.0.0-rc.2").unwrap());
        assert!(!version_is_newer("1.0.0+new", "1.0.0+old").unwrap());
        assert!(!version_is_newer("0.9.0", "0.10.0").unwrap());
    }

    #[test]
    fn check_and_dry_run_never_download_or_install() {
        for (check, dry_run) in [(true, false), (false, true)] {
            let (metadata, _) = release("2.0.0", b"archive");
            let fetcher = fake(vec![Ok(metadata)]);
            let installer = FakeInstaller {
                calls: Cell::new(0),
            };
            let report = execute(
                &UpdateArgs { check },
                dry_run,
                "1.0.0",
                "x86_64-unknown-linux-gnu",
                &fetcher,
                &installer,
            )
            .unwrap();
            assert!(report.update_available && !report.updated);
            assert_eq!((fetcher.calls.get(), installer.calls.get()), (1, 0));
        }
    }

    #[test]
    fn current_and_local_newer_do_not_download() {
        for latest in ["1.0.0", "0.9.0"] {
            let (metadata, _) = release(latest, b"archive");
            let fetcher = fake(vec![Ok(metadata)]);
            let installer = FakeInstaller {
                calls: Cell::new(0),
            };
            let report = execute(
                &UpdateArgs { check: false },
                false,
                "1.0.0",
                "x86_64-unknown-linux-gnu",
                &fetcher,
                &installer,
            )
            .unwrap();
            assert!(!report.update_available);
            assert_eq!(fetcher.calls.get(), 1);
        }
    }

    #[test]
    fn newer_downloads_verifies_and_installs() {
        let archive = b"archive".to_vec();
        let (metadata, sums) = release("2.0.0", &archive);
        let fetcher = fake(vec![Ok(metadata), Ok(archive), Ok(sums)]);
        let installer = FakeInstaller {
            calls: Cell::new(0),
        };
        assert!(
            execute(
                &UpdateArgs { check: false },
                false,
                "1.0.0",
                "x86_64-unknown-linux-gnu",
                &fetcher,
                &installer
            )
            .unwrap()
            .updated
        );
        assert_eq!((fetcher.calls.get(), installer.calls.get()), (3, 1));
    }

    #[test]
    fn metadata_download_and_checksum_errors_propagate() {
        let fetcher = fake(vec![Err(CboxError::tempfail("offline"))]);
        let installer = FakeInstaller {
            calls: Cell::new(0),
        };
        assert!(execute(
            &UpdateArgs { check: true },
            false,
            "1.0.0",
            "x",
            &fetcher,
            &installer
        )
        .is_err());

        let fetcher = fake(vec![Ok(b"not json".to_vec())]);
        assert!(execute(
            &UpdateArgs { check: true },
            false,
            "1.0.0",
            "x",
            &fetcher,
            &installer
        )
        .is_err());

        let (metadata, _) = release("2.0.0", b"good");
        let fetcher = fake(vec![
            Ok(metadata),
            Err(CboxError::tempfail("asset offline")),
        ]);
        assert!(execute(
            &UpdateArgs { check: false },
            false,
            "1.0.0",
            "x86_64-unknown-linux-gnu",
            &fetcher,
            &installer
        )
        .is_err());

        let (metadata, _) = release("2.0.0", b"good");
        let name = "cbox-2.0.0-x86_64-unknown-linux-gnu.tar.gz";
        let fetcher = fake(vec![
            Ok(metadata),
            Ok(b"bad".to_vec()),
            Ok(format!("{}  {name}\n", "0".repeat(64)).into_bytes()),
        ]);
        assert!(execute(
            &UpdateArgs { check: false },
            false,
            "1.0.0",
            "x86_64-unknown-linux-gnu",
            &fetcher,
            &installer
        )
        .is_err());
        assert_eq!(installer.calls.get(), 0);
    }

    fn archive(entries: &[(&str, tar::EntryType, &[u8])]) -> Vec<u8> {
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut builder = tar::Builder::new(&mut gzip);
            for (path, kind, body) in entries {
                let mut header = tar::Header::new_gnu();
                header.set_entry_type(*kind);
                header.set_mode(0o755);
                header.set_size(body.len() as u64);
                header.set_cksum();
                builder.append_data(&mut header, path, *body).unwrap();
            }
            builder.finish().unwrap();
        }
        gzip.finish().unwrap()
    }

    fn archive_with_raw_path(path: &str) -> Vec<u8> {
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut builder = tar::Builder::new(&mut gzip);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_mode(0o755);
            header.set_size(1);
            header.as_mut_bytes()[..100].fill(0);
            header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
            header.set_cksum();
            builder.append(&header, &b"x"[..]).unwrap();
            builder.finish().unwrap();
        }
        gzip.finish().unwrap()
    }

    #[test]
    fn archive_rejects_missing_outside_and_link_members() {
        let dir = tempfile::tempdir().unwrap();
        assert!(extract_binary(
            &archive(&[("root/README", tar::EntryType::Regular, b"x")]),
            "root",
            &dir.path().join("out")
        )
        .is_err());
        assert!(extract_binary(
            &archive(&[("other/cbox", tar::EntryType::Regular, b"x")]),
            "root",
            &dir.path().join("out")
        )
        .is_err());
        assert!(extract_binary(
            &archive(&[("root/link", tar::EntryType::Symlink, b"")]),
            "root",
            &dir.path().join("out")
        )
        .is_err());
        for path in ["/root/cbox", "root/../cbox"] {
            assert!(extract_binary(
                &archive_with_raw_path(path),
                "root",
                &dir.path().join("out")
            )
            .is_err());
        }
    }

    #[test]
    fn atomic_install_preserves_mode_and_cleans_staging() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("cbox");
        fs::write(&destination, b"old").unwrap();
        let bytes = archive(&[("cbox-1-t/cbox", tar::EntryType::Regular, b"new")]);
        install_verified_archive(&bytes, "1", "t", &destination).unwrap();
        let mut actual = Vec::new();
        File::open(&destination)
            .unwrap()
            .read_to_end(&mut actual)
            .unwrap();
        assert_eq!(actual, b"new");
        assert!(!dir
            .path()
            .join(format!(".cbox-update-{}", std::process::id()))
            .exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(destination).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
    }
}
