<p align="center">
  <img src="assets/cardbox-logo.jpeg" alt="The Cardboard Box — a pixel-art cardboard box with Ubuntu, Debian, and Fedora emblems bursting out" width="320">
</p>

<h1 align="center">The Cardboard Box</h1>
<p align="center"><strong><code>cbox</code></strong> — declarative management for <code>distrobox</code></p>

<p align="center">
  <a href="https://github.com/Rynaro/cardboard-box/actions/workflows/ci.yml"><img src="https://github.com/Rynaro/cardboard-box/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/Rynaro/cardboard-box/actions/workflows/release.yml"><img src="https://github.com/Rynaro/cardboard-box/actions/workflows/release.yml/badge.svg" alt="Release"></a>
  <a href="https://github.com/Rynaro/cardboard-box/releases/latest"><img src="https://img.shields.io/github/v/release/Rynaro/cardboard-box?sort=semver" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Made%20with-Rust-orange?logo=rust" alt="Made with Rust"></a>
  <img src="https://img.shields.io/badge/platform-Linux-blue?logo=linux" alt="Platform: Linux">
</p>

---

## Welcome to cbox

`distrobox` is wonderful—it lets you run any Linux distro as a lightweight container, keeping your host pristine. But after the thirtieth time you're memorizing flags, hand-mounting sockets, and re-typing the same `distrobox create` incantation, you realize: this should be simpler.

**`cbox` is that simpler.** One CLI for the whole box lifecycle. One declarative `Boxfile.toml` (Vagrant for distrobox). A terminal cockpit when you want it. Your Linux environments, unboxed.

---

## Why cbox?

📦 **You keep the focus on distrobox.** `cbox` wraps the real thing—it doesn't reimplement it. Your knowledge transfers directly; the CLI just handles the repetition.

🎯 **Docker access, one legible knob.** Three modes (`none`, `host`, `nested`)—not a free-form field. Each is a named bundle of exact flags. Pick one; the rest is decided.

♻️ **Idempotent provisioning.** Declare your box once in a `Boxfile`. Run `cbox apply` or `cbox up`. Second time? Only changed steps re-run. Drift is math, not magic.

✨ **The TUI is the acceptance test.** Not just a shell-over-CLI. It reuses the exact same core logic—zero duplication, zero drift risk. And if you prefer the command line, it's equally first-class.

🔧 **Just works.** Static binary, containerized build (your host stays pristine), built-in `cbox doctor` to catch issues early.

---

## A first taste

### Create and enter a box imperatively

```bash
cbox create web-dev -i fedora-toolbox:latest
cbox enter web-dev
```

### Or go declarative with a Boxfile

`Boxfile.toml`:
```toml
name = "web-dev"
image = "fedora-toolbox:latest"
packages = ["git", "ripgrep", "rust"]
docker = "host"

[[mounts]]
host = "/home/me/code"
guest = "/code"
```

Then:
```bash
cbox up web-dev --file Boxfile.toml
cbox list
cbox enter web-dev
```

That's it. The box is live, provisioned, and ready.

---

## What's in the box

### A just-works CLI

Seven core subcommands: `create`, `list`, `rm` (alias `destroy`), `enter` (alias `use`), `inspect` (alias `show`), `edit`, `doctor`. Plus `apply` and `up` for provisioning. Global flags for scripting: `--json`, `--dry-run`, `-v` for debugging, `-y` to skip confirmations.

```bash
cbox list --json | jq .
cbox apply web-dev --dry-run    # preview changes
cbox doctor                      # health check
```

### The docker-access spectrum: `none | host | nested`

**Exactly one legible knob** across the sandbox ↔ host-Docker tradeoff:

- **`none`** (default): box is decoupled from the host runtime. Clean separation. Optionally harden with `--unshare-*` flags.
- **`host`**: mount the host's container socket into the box (auto-detects podman or docker). Containers you create inside the box appear in `docker ps` on the host.
- **`nested`**: a private Docker daemon inside the box. Containers are isolated, not visible on the host. True Docker-in-Docker.

Each mode is a **named bundle**—no guessing. Set `docker = "host"` in the Boxfile; the rest is automatic.

<details>
<summary>More: the docker modes explained</summary>

**`docker = "none"` (default)**

```toml
docker = "none"
[sandbox]
unshare = ["netns", "ipc"]  # optional hardening
```

Inside the box, `docker ps` or `podman ps` fail with "cannot connect" — no socket. Useful for isolated development.

**`docker = "host"`**

```toml
docker = "host"
```

Auto-detection picks podman or docker based on your host's backend:
- **On podman host:** mount `/run/user/$UID/podman/podman.sock` and install `podman-remote`.
- **On docker host:** mount `/var/run/docker.sock` and install `docker-cli`.

Result: `docker ps` in the box == `docker ps` on the host.

**`docker = "nested"`**

```toml
docker = "nested"
```

Install `docker-ce` inside the box. `--init` is added (systemd). No host socket is mounted. True isolation.

</details>

### Declarative `Boxfile.toml`

A single TOML file declares your box's intent. Run `cbox up` or `cbox apply` to make it real. Idempotent by design—second apply skips unchanged steps. Portable—copy to another host, run `cbox up`, same box.

```toml
name = "web-dev"
image = "fedora-toolbox:latest"
packages = ["git", "ripgrep"]
docker = "host"

[[mounts]]
host = "/home/me/code"
guest = "/code"

[[provision]]
type = "shell"
run = "rustup default stable"

[[provision]]
type = "copy"
src = "./dotfiles/.bashrc"
dst = "/home/me/.bashrc"
```

### The TUI cockpit

Run `cbox` with no arguments on a TTY and you get a full terminal UI. List your boxes, inspect one, create via a wizard, apply provisioning with live progress, edit Boxfiles in your `$EDITOR`, and enter boxes—all with arrow keys and a few key presses.

The TUI reuses the exact same `core::` logic as the CLI. **Zero duplication**, zero drift. Feature-gated by default; build lean with `--no-default-features` if you prefer CLI only.

```bash
cbox
# → launches the terminal cockpit
```

Key bindings: `↑↓` move, `c` create, `d` destroy, `a` apply, `e` edit, `enter` inspect/enter, `?` doctor, `q` quit.

---

## Install

### Recommended: containerized build (clean-host guarantee)

The project ships a `.devcontainer` with the Rust toolchain. All builds, tests, and linting happen inside a container—your host stays pristine. The finished binary is extracted with `make install` or `make dist`, not read from a bare-host `target/` directory.

```bash
# First time only
make dev-init

# Build and install
make install
# → installed cbox to ~/.local/bin/cbox
#   override with: make install PREFIX=/usr/local

# Or just drop the binary in ./dist
make dist
# → ./dist/cbox
```

**Other make targets:**
```bash
make build         # debug build
make release       # optimized build
make test          # run all 129 tests (zero real distrobox needed)
make check         # full CI gate: fmt + lint + build + test
make shell         # interactive shell inside the dev container
make clean         # remove build artifacts
make nuke          # full reset (remove image + volumes)
```

Do **not** run bare `cargo` on the host — the Makefile is the supported path.

### Runtime prerequisites

On the host, once:

```bash
# Install distrobox (≥1.6 required)
sudo dnf install distrobox

# Install a backend (podman preferred, or docker)
sudo dnf install podman
# or: sudo dnf install docker
```

Then verify:
```bash
cbox doctor
```

---

## Quickstart

### 1. Health check
```bash
cbox doctor
```

Output:
```
✓ distrobox 1.8.2.4 (supported)
✓ podman 5.8.2 (selected backend)
```

### 2. Create a box
```bash
cbox create web-dev -i fedora-toolbox:latest
```

Or use a Boxfile:
```bash
cbox up web-dev --file Boxfile.toml
```

### 3. List your boxes
```bash
cbox list
```

```
NAME      STATUS    IMAGE                              DOCKER  CBOX?
web-dev   running   fedora-toolbox:latest             none    ✓
old-box   exited    ubuntu-toolbox:22.04               host    ✓
```

Or JSON:
```bash
cbox list --json | jq .
```

### 4. Enter a box
```bash
cbox enter web-dev
# → real TTY, job control, full shell. Ctrl-D to exit.
```

### 5. Apply provisioning
```bash
cbox apply web-dev
```

```
Applying Boxfile for "web-dev" …
  packages   ✓ up to date (git, ripgrep, rust)
  provision  [0] shell  ✓ skipped (unchanged)
✓ Box "web-dev" is up to date (0 steps ran)
```

Changes something in the Boxfile? Run `apply` again—it diffs and converges only what changed.

---

## Reference

<details>
<summary><strong>Command reference</strong></summary>

All commands honor global flags: `--json`, `-q`/`--quiet`, `-v` (show argv), `-vv` (stream child output), `--no-color`, `-y`/`--yes` (skip confirms), `--dry-run`, `--backend podman|docker`.

| Command | Aliases | Purpose | Key flags |
|---------|---------|---------|-----------|
| `cbox create <NAME>` | — | Create a box imperatively or from a Boxfile | `-i/--image`, `-p/--package` (repeatable), `-m/--mount` (repeatable), `--docker none\|host\|nested`, `--home`, `--hostname`, `--init`, `--pull`, `--file`, `--dry-run` |
| `cbox list` | — | List boxes (human table or `--json`) | `-a/--all` (include non-cbox boxes), `--json` |
| `cbox rm <NAME>...` | `destroy` | Remove boxes (confirm unless `-y`) | `-f/--force`, `--rm-home`, `-y`, `--all` |
| `cbox enter <NAME>` | `use` | Enter a box interactively | `--root`, `--clean-path` |
| `cbox inspect <NAME>` | `show` | Inspect a box (human panel or `--json`) | `--json`, `--raw` |
| `cbox edit <NAME>` | — | Edit a box's Boxfile in `$EDITOR` | `--file <PATH>` |
| `cbox apply <NAME>` | — | Converge a box to its Boxfile | `--file`, `--force`, `--redo <IDX>`, `--no-provision`, `--recreate`, `--dry-run`, `--json` |
| `cbox up <NAME>` | — | Create-if-absent then apply | All create + apply flags |
| `cbox doctor` | — | Preflight: distrobox + backend health | `--json` |
| `cbox` (no args) | `cbox tui` | Launch the TUI (TTY only) | — |

</details>

<details>
<summary><strong>Exit codes</strong></summary>

| Code | Meaning |
|------|---------|
| 0 | Success |
| 64 | Usage error (bad CLI args, invalid name, `--json` on interactive) |
| 65 | Data error (invalid Boxfile, missing source for copy step) |
| 69 | Unavailable (box does not exist) |
| 70 | Software error (distrobox missing, or TUI built without `tui` feature) |
| 74 | I/O error (spawn/capture failure, guest state-file corruption) |
| 75 | Temporary failure (backend unreachable) |
| 125 | Backend non-zero (wrapped distrobox exited non-zero) |

</details>

<details>
<summary><strong>Boxfile schema</strong></summary>

```toml
# Boxfile.toml — declarative box manifest (Vagrant-for-distrobox)

# --- identity (required) ---
name = "web-dev"                                      # string, must match ^[a-zA-Z0-9][a-zA-Z0-9_.-]*$

# --- base image (optional) ---
image = "fedora-toolbox:latest"                       # string, default registry.fedoraproject.org/fedora-toolbox:latest

# --- packages (optional) ---
packages = ["git", "ripgrep", "fd-find"]             # list<string>, default []

# --- docker spectrum (optional) ---
docker = "none"                                       # enum: "none" (default) | "host" | "nested"

# --- host↔guest mounts (optional) ---
[[mounts]]
host = "/home/me/code"                               # string, required, absolute path
guest = "/code"                                       # string, required, absolute path
mode = "rw"                                           # enum: "ro" | "rw" (default)

# --- sandbox hardening (optional, meaningful only for docker="none") ---
[sandbox]
unshare = ["netns", "ipc"]                           # list<enum> or "all"
                                                      # enum values: netns, ipc, process, devsys, groups
                                                      # default: []
init = false                                          # bool, default false (systemd in box)

# --- box runtime knobs (optional) ---
[box]
home = ""                                             # string path, default "" (unset). custom home dir in box
hostname = ""                                         # string, default "" (unset). custom hostname
pull = false                                          # bool, default false. pull image before create

# --- provisioning (optional) ---
[[provision]]
type = "shell"                                        # enum: "shell" | "copy"
run = "rustup default stable"                         # string, required if type="shell"

[[provision]]
type = "copy"
src = "./dotfiles/.bashrc"                           # string, required if type="copy", relative to Boxfile dir
dst = "/home/me/.bashrc"                             # string, required if type="copy", absolute path
```

| Path | Type | Default | Required | Notes |
|------|------|---------|----------|-------|
| `name` | string | — | **yes** | Must match regex |
| `image` | string | `registry.fedoraproject.org/fedora-toolbox:latest` | no | Registry prefix auto-added if missing |
| `packages` | list<string> | `[]` | no | Installed at create time |
| `docker` | enum | `"none"` | no | `"none"` (decoupled), `"host"` (host Docker visible), `"nested"` (private DinD) |
| `mounts[].host` | string | — | yes-in-entry | Absolute path on host |
| `mounts[].guest` | string | — | yes-in-entry | Absolute path inside box |
| `mounts[].mode` | enum | `"rw"` | no | `"ro"` (read-only) or `"rw"` (read-write) |
| `sandbox.unshare` | list<enum> or `"all"` | `[]` | no | Namespace options (meaningful with `docker="none"`) |
| `sandbox.init` | bool | `false` | no | Enable systemd inside box |
| `box.home` | string | `""` | no | Custom home directory path in box |
| `box.hostname` | string | `""` | no | Custom hostname inside box |
| `box.pull` | bool | `false` | no | Force pull image at create time |
| `provision[].type` | enum | — | yes-in-entry | `"shell"` or `"copy"` |
| `provision[].run` | string | — | if `type="shell"` | Shell command |
| `provision[].src` | string | — | if `type="copy"` | Source file (on host), relative to Boxfile dir |
| `provision[].dst` | string | — | if `type="copy"` | Destination path (in box), must be absolute |

</details>

<details>
<summary><strong>Idempotency & the apply flow</strong></summary>

**`cbox up`** (create-if-absent, then apply):
```bash
cbox up web-dev --file Boxfile.toml
```

If `web-dev` doesn't exist, it's created. Then every `[[provision]]` step runs (first time). Subsequent `cbox up` or `cbox apply` are **idempotent**: steps whose hash matches the stored state are **skipped**. Only changed steps re-run.

**`cbox apply`** (converge an existing box):
```bash
cbox apply web-dev
```

Diffs the Boxfile against the live box:
- **Incremental changes** (added packages, new provision steps): run and record.
- **Recreate-class changes** (image changed, docker mode changed, mounts changed): prompt `Recreate "web-dev"? [y/n]` (skip with `-y` or `--recreate`).

**Dry-run:**
```bash
cbox apply web-dev --dry-run
```

Prints the plan (which steps would SKIP or RUN, which fields differ) without executing anything.

</details>

<details>
<summary><strong>TUI screens</strong></summary>

- **List (home)**: table of boxes (NAME | STATUS | IMAGE | DOCKER | CBOX?). Arrow keys to move, `c` to create, `d` to destroy, `a` to apply, `e` to edit, `enter` to inspect or enter (if running), `?` for doctor, `q` to quit.
- **Detail (inspect)**: key/value panel for a selected box. `e` to edit its Boxfile, `a` to apply, `enter` to enter.
- **Create wizard**: step through name, image, packages, docker-mode picker, confirm.
- **Apply / up progress**: per-step list (idx | type | status | duration). SKIP (dim), RAN (green), COPIED (green), FAILED (red).
- **Doctor panel**: distrobox version + backend health. Auto-pops if backend is unreachable.
- **Boxfile editor**: hands off to your `$EDITOR` (alt-screen suspended), revalidates on save, restores the TUI.

</details>

<details>
<summary><strong>Architecture & design</strong></summary>

**Layering rule (enforced):** `cli → core → {dbox, boxfile}` + `tui → core` only. No backwards dependencies. This makes the TUI a pure add-on; zero core changes needed for Phase 3.

**Seams:**
- **`DistroboxRunner` trait** (`src/dbox/runner.rs`): `RealRunner` spawns `distrobox`; `MockRunner` is a test double. All commands call through this seam. Makes **100% of command logic unit-testable** against `MockRunner` in CI without any real distrobox installed.
- **`ProvisionStateStore` trait** (`src/core/state_store.rs`): state lives guest-side in `~/.local/state/cbox/provision.json`. Swappable for tests.

**Three phases:**
- **Phase 1 (CLI lifecycle):** Seven subcommands + DistroboxRunner seam + docker-spectrum knob.
- **Phase 2 (Provisioning):** `cbox apply` + `cbox up` + idempotent per-step provisioning.
- **Phase 3 (TUI):** `cbox` or `cbox tui` launches a TEA (Model–Message–update–view) cockpit. Reuses the exact `core::` functions.

**Test coverage:**
- **129 tests**, all mock-driven.
- **Unit tests** (`tests/argv_builder.rs`): pure flag-mapping functions.
- **Integration tests** (`tests/create.rs`, `tests/list.rs`, etc.): commands against `MockRunner`.
- **CI gate** (`G-NO-NET`): full test suite passes with zero distrobox/podman/docker installed.

</details>

---

## Honest notes

**`cbox` wraps the real `distrobox`** by spawning it—it does not reimplement distrobox. Your knowledge transfers directly.

**Linux-only.** distrobox is Linux-only; so is cbox.

**Runtime requirements:** `distrobox ≥ 1.6` and a backend (`podman` preferred, or `docker`).

**Sandbox is not a security boundary.** The `none` docker mode means **decoupled from the host container runtime**, not isolated. distrobox runs `--privileged --security-opt label=disable` by default. `cbox` adds optional `--unshare-*` hardening, but true isolation requires a separate VM.

**Tests use mocks.** All 129 tests run without any real distrobox installed. Real-host validation happens via optional smoke tests (`#[ignore]`, manual run).

**No secrets support yet.** v1.0 does not support embedded secrets. `[[provision]].run` is plain text in the Boxfile (trust boundary = your Boxfile). For sensitive data, store it outside the Boxfile (e.g., in `$HOME/.env`, mounted read-only) and source it in a run step.

**No distrobox-export yet.** v1.0 focuses on the box itself. `distrobox-export` integration is a future feature.

**Inserting provision steps shifts indices.** The idempotency key is `(index, content-hash)`. Inserting a step in the middle shifts downstream indices → those steps re-run. This is by design and acceptable because provisioning steps should be idempotent (Vagrant-style contract).

---

## Releases & versioning

Pre-built Linux binaries are published to [GitHub Releases](https://github.com/Rynaro/cardboard-box/releases) for every version. Four targets:

- `x86_64-unknown-linux-gnu` (glibc ≥ 2.28)
- `x86_64-unknown-linux-musl` (fully static)
- `aarch64-unknown-linux-gnu` (glibc ≥ 2.28)
- `aarch64-unknown-linux-musl` (fully static)

Versioning follows [SemVer](https://semver.org/) driven by [Conventional Commits](https://www.conventionalcommits.org/). See [RELEASING.md](RELEASING.md) for the full versioning policy and artifact verification instructions.

---

## Contributing

All three implementation phases are complete and stable. Contributions welcome—open issues for bugs or feature ideas.

**Development workflow:** Use `make dev-init` && `make check` to run the full test + lint suite locally (in a container). All work is mocked; no real distrobox needed.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full contribution guide including the required Conventional Commits PR-title format.

---

## License

MIT. See `LICENSE` file.

---

**Rynaro** — June 2026.
