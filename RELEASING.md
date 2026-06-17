# Releasing cbox

This document describes the release process for maintainers and contributors.
All releases are automated via `release-please` and triggered by landing
[Conventional Commits](https://www.conventionalcommits.org/) on `main`.

---

## Prerequisites (one-time repo settings)

Before the first release the maintainer must toggle two GitHub settings:

1. **Settings → Actions → General → Workflow permissions**
   - Enable **"Allow GitHub Actions to create and approve pull requests."**
     Without this, `release-please` cannot open the Release PR and the whole
     pipeline is silently blocked. This is the most common first-run failure.
   - Confirm **"Read and write permissions"** is selected for the default
     `GITHUB_TOKEN` (or rely on the per-job `permissions:` blocks in the
     workflow, which are already set).

2. **Branch protection on `main`** (if enabled): allow the `release-please`
   bot to push the version-bump + CHANGELOG commit that closes the Release PR.
   The maintainer still reviews and merges the Release PR manually — that is the
   intended human gate. Only the final merge-commit push needs to be unblocked.

No PAT, no GitHub App, and no external secrets are required.

---

## Conventional Commits — the only thing you need to remember

`cbox` uses **squash-merge** for feature PRs. The PR title becomes the commit
subject on `main`. `release-please` reads those subjects to compute the next
version and populate `CHANGELOG.md`.

**Format:** `<type>[optional scope]: <description>`

Examples:
- `feat: add cbox export command`
- `fix: handle empty Boxfile gracefully`
- `feat(tui): add doctor panel keyboard shortcut`
- `feat!: rename --docker flag to --mode`  ← breaking change

### Commit types → SemVer bump (pre-1.0 behaviour, `0.x`)

| Commit type | Example subject | Bump (pre-1.0) | In CHANGELOG |
|---|---|---|---|
| `feat:` | `feat: add cbox export command` | **minor** `0.1.0 → 0.2.0` | Features section |
| `fix:` | `fix: handle empty Boxfile` | **patch** `0.1.0 → 0.1.1` | Bug Fixes section |
| `feat!:` or `BREAKING CHANGE:` footer | `feat!: rename --docker flag` | **minor** pre-1.0 (would be major post-1.0) | Breaking Changes + Features |
| `perf:` | `perf: cache backend detection` | patch | Performance |
| `docs:`, `chore:`, `ci:`, `build:`, `refactor:`, `test:`, `style:` | `docs: fix typo in README` | **no release** | most hidden |
| `revert:` | `revert: feat: broken command` | patch | Reverts |

**Pre-1.0 rule (`bump-minor-pre-major: true`):** while `version < 1.0.0`, a
breaking change (`feat!:` / `BREAKING CHANGE:`) bumps the **minor** version
rather than the major. This is the conventional pre-1.0 contract. Once the
maintainer decides `cbox` is stable enough for 1.0, see "Going 1.0" below.

**Non-version-bumping types** (`docs:`, `chore:`, `ci:`, `build:`, `refactor:`,
`test:`, `style:`) do **not** trigger a release and do not appear in the
CHANGELOG. The `build` and `publish` jobs in the release workflow are gated
and skipped — no wasted CI minutes.

---

## How the Release PR works

1. A Conventional Commit lands on `main` (e.g. `feat: add export command`).
2. The `release-please` job in `.github/workflows/release.yml` runs and opens
   (or updates) a **Release PR** proposing the next version bump. The PR title
   looks like `chore(main): release 0.2.0`. It contains:
   - `Cargo.toml` version field updated to `0.2.0`
   - `Cargo.lock` updated
   - `CHANGELOG.md` entry added
3. More Conventional Commits land → `release-please` updates the same PR
   (batching changes into the next release), accumulating CHANGELOG entries.
4. The maintainer reviews and **merges** the Release PR (do **not** squash it —
   use a regular merge commit so the release-please metadata in the PR
   description is preserved).
5. On merge, the same workflow run:
   - Creates a Git tag (`vX.Y.Z`)
   - Creates a GitHub Release
   - Triggers the `build` (matrix × 4 targets) and `publish` jobs in the
     **same run**, uploading 4 tarballs + `SHA256SUMS` to the Release.

The full pipeline from merge to published Release typically completes in under
10 minutes.

---

## Artifacts

Each GitHub Release contains **5 assets**:

| Asset | Description |
|---|---|
| `cbox-<version>-x86_64-unknown-linux-gnu.tar.gz` | x86-64 Linux, dynamically linked to glibc ≥ 2.28 |
| `cbox-<version>-x86_64-unknown-linux-musl.tar.gz` | x86-64 Linux, fully static (musl) — widest compatibility |
| `cbox-<version>-aarch64-unknown-linux-gnu.tar.gz` | ARM64 Linux, dynamically linked to glibc ≥ 2.28 |
| `cbox-<version>-aarch64-unknown-linux-musl.tar.gz` | ARM64 Linux, fully static (musl) |
| `SHA256SUMS` | SHA-256 checksums for all 4 tarballs |

Each tarball contains exactly: `cbox` (the binary), `README.md`, `LICENSE`.

**glibc floor:** the `gnu` targets are built with `cargo-zigbuild` pinning
glibc to **2.28** (Debian 10 / RHEL 8 / Ubuntu 18.04). If you need to run on
a system with an older glibc, use the `musl` tarball instead.

**musl tarballs:** fully statically linked. `ldd cbox` reports
`not a dynamic executable`. Runs on any Linux regardless of glibc version.

### Verifying artifacts

After downloading a tarball and `SHA256SUMS` into the same directory:

```bash
sha256sum -c SHA256SUMS
```

Expected output:

```
cbox-0.2.0-x86_64-unknown-linux-gnu.tar.gz: OK
cbox-0.2.0-x86_64-unknown-linux-musl.tar.gz: OK
cbox-0.2.0-aarch64-unknown-linux-gnu.tar.gz: OK
cbox-0.2.0-aarch64-unknown-linux-musl.tar.gz: OK
```

Note: binaries are **not cryptographically signed** in this release (see
"Future hardening" below). `SHA256SUMS` provides integrity (detect corruption
or tampering in transit) but not authenticity (does not prove the binary came
from this repo's CI). SLSA provenance and `cosign` signing are deferred.

---

## Manual / emergency release path

If `release-please` is unavailable or you need to cut a hotfix outside the
normal flow:

```bash
# 1. Manually bump the version in Cargo.toml and Cargo.lock
#    (or use `cargo set-version X.Y.Z` if cargo-edit is installed)
vim Cargo.toml   # set version = "X.Y.Z"
cargo generate-lockfile

# 2. Commit with a non-bumping type so release-please doesn't double-bump:
git commit -am "chore: bump version to X.Y.Z (manual release)"

# 3. Tag:
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin vX.Y.Z

# 4. Create the GitHub Release and kick off the build by re-running
#    the release workflow manually (Actions → Release → Run workflow),
#    or use gh:
gh release create vX.Y.Z --title "vX.Y.Z" --notes "Emergency release."

# 5. The build/publish jobs in release.yml will NOT fire automatically
#    for a manually-created tag (they gate on release-please output).
#    For a manual release, build the 4 tarballs locally (or in CI) and
#    upload them:
gh release upload vX.Y.Z dist/*.tar.gz dist/SHA256SUMS --clobber
```

Alternatively, you can use the `release-as:` footer in a commit to force
`release-please` to propose a specific version:

```
feat: my feature

Release-As: 1.0.0
```

This is the recommended way to cut `1.0.0` deliberately (see "Going 1.0").

---

## Going 1.0

`cbox` is currently pre-1.0 (`0.x`). The `bump-minor-pre-major: true` setting
means that breaking changes bump the minor version (not the major) while
`version < 1.0.0`.

When the maintainer decides `cbox` is stable enough for 1.0:

1. Land a commit with the `Release-As:` footer:
   ```
   feat: prepare for stable release

   Release-As: 1.0.0
   ```
2. `release-please` will open a Release PR proposing `1.0.0`.
3. From `1.0.0` onwards, breaking changes (`feat!:` / `BREAKING CHANGE:`) bump
   the **major** version (`1.0.0 → 2.0.0`), per standard SemVer.

Do NOT manually set `version = "1.0.0"` in `Cargo.toml` — let `release-please`
own the version field.

---

## Why one workflow file

The release pipeline lives entirely in `.github/workflows/release.yml` rather
than being split into a `release-please.yml` + a separate `build.yml`.

This is intentional. A GitHub Release (or tag) created by the default
`GITHUB_TOKEN` does **not** emit events that trigger other workflows — GitHub
blocks this to prevent infinite loops. The naive two-file pattern silently never
fires the build workflow. The fix is a single file where the `build` and
`publish` jobs are gated on `release-please`'s output within the same run.

**Do NOT split this into two files** without also introducing a PAT or GitHub
App token (which adds secret-management overhead and a wider blast radius).
The single-workflow pattern is the canonical, secret-free solution recommended
by the `release-please` docs for exactly this case.

---

## Future hardening (deferred)

- **SLSA provenance / `cosign` signing** — cryptographically sign release
  binaries so users can verify they came from this repo's CI. Deferred to a
  future version.
- **crates.io publish** — `cbox` is a binary, not a library; GitHub Releases
  cover users. A publish job is deferred.
- **Packaging** — AUR, Homebrew tap, `.deb`/`.rpm`, Nix. Deferred.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the contribution workflow.
