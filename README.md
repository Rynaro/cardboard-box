# The Cardboard Box (`cbox`)

**A cozy distrobox manager** — Vagrant-inspired declarative box provisioning, a beautiful terminal cockpit, and a clean CLI for everyone.

`cbox` raises `distrobox` from an advanced-user tool to "just works": single static binary, zero external scripting, and a docker-access spectrum from fully-decoupled to host-Docker-visible to isolated-nested. **v1.0 is feature-complete.** All three phases committed: CLI lifecycle, Boxfile-driven provisioning, and a TUI that dogfoots the same mechanisms.

---

## Status

**v1.0 → v3.0 complete.** Three phases, 129 tests, all green on `G-NO-NET` (zero real distrobox in CI).

- **Phase 1 (v1.0):** CLI lifecycle — `create`, `list`, `rm`/`destroy`, `enter`/`use`, `inspect`/`show`, `edit`, `doctor`. Boxfile schema & the docker-mode spectrum.
- **Phase 2 (v2.0):** Provisioning engine — `apply` & `up` with idempotent per-step execution, Boxfile↔live diffing, incremental convergence.
- **Phase 3 (v3.0):** TUI — `cbox` (no args) or `cbox tui` launches a cozy terminal cockpit (list, inspect, create wizard, apply progress, Boxfile editor).

**Honest notes:**
- `cbox` wraps the real `distrobox` CLI by spawning it — it does not reimplement distrobox.
- Linux-only (distrobox is Linux-only).
- Requires `distrobox ≥ 1.6` and a backend (`podman` preferred, or `docker`).
- "Sandbox" (`docker = none`) means **decoupled from the host container runtime**, not a security boundary. distrobox runs `--privileged --security-opt label=disable` by default; `cbox` adds optional `--unshare-*` hardening, but isolation is still limited. Honest framing in docs.
- Tests use mock/stub runners; real-host validation is via smoke tests (`#[ignore]`, manual run).

---

## Highlights

### Cozy, just-works CLI
Seven subcommands: `create`, `list`, `rm` (alias `destroy`), `enter` (alias `use`), `inspect` (alias `show`), `edit`, `doctor`, plus `apply` and `up` for provisioning. Global `--json` for scripting, `--dry-run` to preview, `-v` for debugging.

### The docker-access spectrum: `docker = none | host | nested`
**Exactly one legible knob** across the sandbox↔host-docker tradeoff:

- **`none`** (default): no special container-runtime access. Box is decoupled. Optionally harden with `--unshare-*` (netns, ipc, process, etc.).
- **`host`**: mount the host's container socket into the box (podman or docker, auto-detected). Containers you create inside the box appear in `docker ps` on the host — "I want my host Docker here."
- **`nested`**: a private Docker daemon inside the box. Containers are isolated, not visible on the host. True DinD.

Each mode is a **named bundle of exact flags** — no guessing. Boxfile field `docker = "host"` or `docker = "nested"` expands to the right mounts/packages/flags at create time.

### Vagrant-style declarative provisioning
A `Boxfile.toml` (TOML, not YAML) declares the box's intent once, and `cbox up` and `cbox apply` make it real:

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

Idempotent by design. Second `apply` skips unchanged steps. Boxfiles are portable — copy to another host, run `cbox up`, same box.

### TUI: the cozy terminal cockpit
**`cbox` (no args)** on a TTY launches the cockpit. List your boxes, inspect, create a new one via a wizard, apply provisioning with live step progress, edit Boxfiles in your `$EDITOR`, and `enter` a box with a real TTY. Everything the CLI does, but beautifully interactively.

The TUI reuses the exact same `core::` logic as the CLI — **zero duplication**, zero chance of drift. Feature-gated (on by default); build lean with `--no-default-features` if you prefer the CLI only.

---

## Install / Build

### Recommended: containerized build (the clean-host guarantee)

The project ships a `.devcontainer` with the Rust toolchain. All builds, tests, and linting happen inside a container; your host stays pristine. Because of that, build artifacts live in a named Docker volume — **not** in a `target/` directory on your host — so the finished binary is *extracted* with `make install` (or `make dist`) rather than read out of `target/`.

```bash
# First time only — build the toolchain image + prepare volumes
make dev-init

# Build and install the binary onto your PATH (~/.local/bin by default)
make install
# → ✓ installed cbox to ~/.local/bin/cbox
#   override the location:  make install PREFIX=/usr/local   (may need sudo for system paths)

# …or just drop the binary in ./dist without touching your PATH
make dist
# → ./dist/cbox

# Compile only (artifact stays in the volume)
make build      # debug
make release    # optimized

# Run tests (zero real distrobox needed — mocks prove everything)
make test

# Lint both feature configs; format
make lint        # --all-features
make lint-lean   # --no-default-features (TUI off)
make fmt

# Everything CI gates on, in one shot
make check

# Interactive shell inside the dev container (for poking around)
make shell

# Blow away build artifacts (keeps cargo cache) / full reset
make clean
make nuke
```

The `Makefile` handles the Docker/Podman image, named volumes for the cargo cache + build artifacts, and user ID / GID mapping so any files written back to the source tree stay yours. **Do not run bare `cargo` on the host** — the Makefile is the supported path, and it's what keeps your host clean.

### Runtime prerequisites (on the host, once)

```bash
# Install distrobox (≥1.6 required)
# On Fedora:
sudo dnf install distrobox

# Install a backend (podman preferred)
sudo dnf install podman
# or docker:
sudo dnf install docker
```

Verify your setup:
```bash
cbox doctor
```

---

## Quickstart

### 1. Health check
```bash
cbox doctor
```

Output (human):
```
✓ distrobox 1.8.2.4 (supported)
✓ podman 5.8.2 (selected backend)
```

### 2. Create a simple box
```bash
cbox create web-dev -i fedora-toolbox:latest
```

Or declare it in a `Boxfile.toml`:
```toml
name = "web-dev"
image = "fedora-toolbox:latest"
packages = ["git", "ripgrep", "rust"]
docker = "none"
```

Then:
```bash
cbox up web-dev --file Boxfile.toml
```

### 3. List your boxes
```bash
cbox list
```

Table output:
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

Output (mixed):
```
Applying Boxfile for "web-dev" …
  packages   ✓ up to date (git, ripgrep, rust)
  provision  [0] shell  ✓ skipped (unchanged)
✓ Box "web-dev" is up to date (0 steps ran)
```

Changes something in the Boxfile, run `apply` again — it diffs and converges only what changed.

### 6. TUI (interactive)
```bash
cbox
# → launches the terminal cockpit (if on a TTY)
```

Or explicitly:
```bash
cbox tui
```

Key bindings: arrow keys to move, `c` to create, `d` to destroy, `a` to apply, `e` to edit, `enter` to inspect or open, `q` to quit.

---

## Command Reference

All commands honor global flags: `--json`, `-q`/`--quiet`, `-v` (show argv), `-vv` (stream child output), `--no-color`, `-y`/`--yes` (skip confirms), `--dry-run` (preview), `--backend podman|docker` (override detection).

| Command | Aliases | Purpose | Key flags |
|---------|---------|---------|-----------|
| `cbox create <NAME>` | — | Create a box imperatively or from a Boxfile | `-i/--image`, `-p/--package` (repeatable), `-m/--mount` (repeatable), `--docker none\|host\|nested`, `--home`, `--hostname`, `--init`, `--pull`, `--file`, `--dry-run` |
| `cbox list` | — | List boxes (human table or `--json` machine) | `-a/--all` (include non-cbox boxes), `--json` |
| `cbox rm <NAME>...` | `destroy` | Remove boxes (confirm unless `-y`) | `-f/--force`, `--rm-home`, `-y`, `--all` |
| `cbox enter <NAME>` | `use` | Enter a box interactively; pass commands with `-- <CMD>` | `--root`, `--clean-path` |
| `cbox inspect <NAME>` | `show` | Inspect a box (human panel or `--json`) | `--json`, `--raw` (raw backend JSON) |
| `cbox edit <NAME>` | — | Edit a box's Boxfile in `$EDITOR` (hand off to your editor, revalidate) | `--file <PATH>` (edit a Boxfile directly) |
| `cbox apply <NAME>` | — | Converge a box to its Boxfile (incremental provisioning) | `--file`, `--force`, `--redo <IDX>`, `--no-provision`, `--recreate`, `--dry-run`, `--json` |
| `cbox up <NAME>` | — | Create-if-absent then apply (the "just works" entry point) | All create + apply flags |
| `cbox doctor` | — | Preflight: distrobox + backend health | `--json` |
| `cbox` (no args) | `cbox tui` | Launch the TUI (TTY only) | `--json` rejected (interactive only) |

**Aliases:** `destroy` is `rm`, `use` is `enter`, `show` is `inspect`.

### Exit codes (sysexits-aligned)

| Code | Meaning |
|------|---------|
| 0 | Success |
| 64 | Usage error (bad CLI args, invalid name, `--json` on interactive) |
| 65 | Data error (invalid Boxfile, missing source for copy step, recreate required without `--recreate`) |
| 69 | Unavailable (box does not exist) |
| 70 | Software error (distrobox missing, or TUI built without `tui` feature) |
| 74 | I/O error (spawn/capture failure, guest state-file corruption) |
| 75 | Temporary failure (backend unreachable) |
| 125 | Backend non-zero (wrapped distrobox exited non-zero; child code and stderr surfaced) |

---

## The Boxfile

### Full schema (with comments)

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

### Field reference

| Path | Type | Default | Required | Notes |
|------|------|---------|----------|-------|
| `name` | string | — | **yes** | Must match regex; validated client-side before any spawn |
| `image` | string | `registry.fedoraproject.org/fedora-toolbox:latest` | no | Registry prefix auto-added if missing (Fedora default) |
| `packages` | list<string> | `[]` | no | Installed at create time via `--additional-packages` |
| `docker` | enum | `"none"` | no | `"none"` (decoupled), `"host"` (host Docker visible), `"nested"` (private Docker-in-Docker) |
| `mounts[].host` | string | — | yes-in-entry | Absolute path on host. Can be relative to Boxfile dir in `provision` copy steps only |
| `mounts[].guest` | string | — | yes-in-entry | Absolute path inside box. Must be unique per box |
| `mounts[].mode` | enum | `"rw"` | no | `"ro"` (read-only) or `"rw"` (read-write) |
| `sandbox.unshare` | list\<enum\> or `"all"` | `[]` | no | Namespace options; only meaningful with `docker="none"` |
| `sandbox.init` | bool | `false` | no | Enable systemd inside box (implies `--unshare-process`) |
| `box.home` | string | `""` | no | Custom home directory path in box (default: shared host home) |
| `box.hostname` | string | `""` | no | Custom hostname inside box |
| `box.pull` | bool | `false` | no | Force pull image at create time |
| `provision[].type` | enum | — | yes-in-entry | `"shell"` (run a command) or `"copy"` (copy file) |
| `provision[].run` | string | — | if `type="shell"` | Shell command (e.g., `"dnf install rustup"`) |
| `provision[].src` | string | — | if `type="copy"` | Source file (on host); relative to Boxfile directory |
| `provision[].dst` | string | — | if `type="copy"` | Destination path (in box); must be absolute |

### Docker modes — the spectrum explained

**`docker = "none"` (default)**

The box has **no** access to the host container runtime. Clean separation.

```toml
docker = "none"
[sandbox]
unshare = ["netns", "ipc"]  # optional hardening
```

Inside the box, `docker ps` or `podman ps` fail with "cannot connect" — no socket, no client configured. Useful for isolated development environments.

**`docker = "host"`**

Mount the **host's** container socket into the box. Containers you create inside the box are **visible on the host**.

```toml
docker = "host"
```

Auto-detection picks podman or docker based on your host's backend. Exact steps:

- **On podman host:** mount `/run/user/$UID/podman/podman.sock:/run/user/$UID/podman/podman.sock` and install `podman-remote`.
- **On docker host:** mount `/var/run/docker.sock:/var/run/docker.sock` and install `docker-cli`.

Result: `docker ps` in the box == `docker ps` on the host. Containers created in the box appear in your host's Docker Dashboard. The headline use case: "I want my Docker/Podman accessible from inside this box."

**`docker = "nested"`**

A **private** Docker daemon lives inside the box. Containers it creates are **not** visible on the host.

```toml
docker = "nested"
```

Steps:
- Install `docker-ce` (or `podman` for rootless).
- `--init` is added (systemd, so the in-box daemon can be managed).
- **No** host socket is mounted.

Result: `docker ps` in the box is isolated; the host's `docker ps` does not see those containers. True nesting. Use case: isolated CI environment, development sandbox.

### Idempotency & the apply flow

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
- **Recreate-class changes** (image changed, docker mode changed, mounts changed): prompt `Recreate "web-dev"? [y/n]` (skip with `-y` or `--recreate`). Recreate destroys + recreates; your `$HOME` is preserved (shared mount), but box-local changes are lost.

**Dry-run:**
```bash
cbox apply web-dev --dry-run
```

Prints the plan (which steps would SKIP or RUN, which fields differ) without executing anything.

---

## TUI

### Launch

```bash
cbox
# or explicitly:
cbox tui
```

Only works on a TTY; non-interactive stdin/stdout falls back to printing help + exit code 64.

### Screens

- **List** (home): table of boxes (NAME | STATUS | IMAGE | DOCKER | CBOX?). Arrow keys to move, `c` to create, `d` to destroy, `a` to apply, `e` to edit, `enter` to inspect or enter (if running), `?` for doctor, `q` to quit.
- **Detail (inspect)**: key/value panel for a selected box. `e` to edit its Boxfile, `a` to apply, `enter` to enter.
- **Create wizard**: step through name, image, packages, docker-mode picker, confirm.
- **Apply / up progress**: per-step list (idx | type | status | duration). SKIP (dim), RAN (green), COPIED (green), FAILED (red).
- **Doctor panel**: distrobox version + backend health. Auto-pops if backend is unreachable.
- **Boxfile editor**: hands off to your `$EDITOR` (alt-screen is suspended), revalidates on save, restores the TUI.

### Key hints

Bottom status bar shows context hints. `↑↓ move · enter inspect · c create · d destroy · a apply · e edit · ? doctor · q quit`.

---

## Development

### Targets

```bash
make dev-init      # one-time: build toolchain image + named volumes
make build         # debug build (artifact stays in the volume)
make release       # optimized release build (artifact stays in the volume)
make install       # build release + copy binary to PREFIX/bin (default ~/.local/bin)
make dist          # build release + copy binary to ./dist/cbox
make test          # run all tests (zero real distrobox; mocks + MockRunner)
make lint          # clippy --all-features (tui on)
make lint-lean     # clippy --no-default-features (tui off — catches feature-flag bugs)
make fmt           # cargo fmt
make fmt-check     # check formatting without modifying
make check         # full CI gate: fmt-check + lint + lint-lean + build + test
make shell         # interactive bash inside the dev container
make clean         # remove build artifacts (keep cargo cache)
make nuke          # remove image + all named volumes (full reset)
```

### Test suite

**129 tests**, all mock-driven:
- **Unit tests** (`tests/argv_builder.rs`): pure flag-mapping functions, deterministic golden.
- **Integration tests** (`tests/create.rs`, `tests/list.rs`, etc.): commands against `MockRunner` (no real distrobox spawn).
- **Acceptance criteria** (AC-*): GIVEN/WHEN/THEN per spec, matching sections §7 of each spec document.
- **Smoke tests** (`#[ignore]`): optional real-distrobox checks (manual run, gated on host having distrobox). Run with `make smoke` or `cargo test -- --ignored --test-threads 1`.

**CI gate:** `G-NO-NET` — full test suite passes with zero distrobox/podman/docker installed. The runner trait seam + MockRunner prove all logic without external tools.

### Architecture

```
main.rs → cli/ (clap parse + output)
             └→ core/ (logic, DistroboxRunner seam)
                     ├→ dbox/ (process wrapper, argv builders)
                     └→ boxfile/ (Boxfile model, validation)
             └→ tui/ (ratatui cockpit, reuses core/)
                    └→ (no cli imports; only core + boxfile)
```

**Layering rule (enforced):** `cli → core → {dbox, boxfile}` + `tui → core` only. No `tui → cli`, no backwards deps. This makes the TUI a pure add-on; zero core changes in Phase 3.

**Seams:**
- **`DistroboxRunner` trait** (`src/dbox/runner.rs`): `RealRunner` spawns `distrobox`; `MockRunner` is a programmable test double. All commands call through this seam.
- **`ProvisionStateStore` trait** (`src/core/state_store.rs`): state lives guest-side in `~/.local/state/cbox/provision.json` (P2). Swappable for tests.

### Error handling

- **Core/dbox layers:** typed `CboxError` + `RunnerError` enums (thiserror); each variant carries an exit code.
- **`main.rs` edge:** map to `anyhow::Context` for ergonomic rendering; exit via `std::process::exit(code)`.
- **Rule:** a wrapped child's non-zero exit is **never** silently swallowed. It becomes `CboxError::backend_error(code, stderr_tail, argv)` → exit 125, so the user always sees the exact command and last 5 stderr lines.

### Dependencies

| Crate | Purpose | Why |
|-------|---------|-----|
| `clap` (derive) | CLI parsing | L1-locked; Derive API maps 1:1 to subcommands |
| `anyhow` | Error context at `main.rs` | Ergonomic rendering |
| `thiserror` | Typed errors (core) | Matchable + embedded exit codes |
| `serde` + `toml` | Boxfile parse | L3-locked TOML; serde derives |
| `serde_json` | JSON output + backend `ps/inspect` parsing | `--json` mode + reading podman/docker JSON |
| `sha2` | Idempotency hashing (P2) | Content-hash for provision steps |
| `ratatui` + `crossterm` | TUI (P3, feature-gated) | L1-locked; feature = `"tui"` (on by default) |

**Deliberately excluded:** no `tokio`/async (sync core, worker thread for blocking), no `reqwest`/network (distrobox/backend handle pulls), no config framework (labels + Boxfile are state).

---

## Architecture & Design

### Three-phase design

**Phase 1 (v1.0 — CLI lifecycle):** Seven subcommands + the DistroboxRunner seam + the docker-spectrum knob. Foundation.

**Phase 2 (v2.0 — Provisioning engine):** `cbox apply` + `cbox up` + idempotent per-step provisioning. Guest-side state file (since container labels are immutable post-create). Incremental convergence.

**Phase 3 (v3.0 — TUI):** `cbox` or `cbox tui` launches a TEA (Model–Message–update–view) cockpit. Reuses the exact `core::` functions; zero duplication, zero drift.

### Process wrapper seam

Every distrobox/podman/docker spawn goes through `&dyn DistroboxRunner` — a trait with `run()` (Capture/DryRun modes) and `run_interactive()` (TTY pass-through). This makes **100% of command logic unit-testable** against `MockRunner` in CI without any real distrobox installed.

### Flag mapping

The `docker = none | host | nested` spectrum is a **named bundle of flags**, not a free-form field. Each mode maps to exact `distrobox create` flags (socket mounts, packages, `--init`, `--unshare-*`). Mapping lives in `src/dbox/argv.rs` (pure functions) and is tested independently before any spawn.

---

## FAQ & Known Limitations

**Q: Does `cbox` work on macOS or Windows?**

A: No. distrobox is Linux-only; so is cbox.

**Q: Is the `none` docker mode a real security sandbox?**

A: No. distrobox runs `--privileged --security-opt label=disable` by default; it shares user, home, network, IPC, PID namespaces. "Decoupled" (`none`) means **no extra host-Docker coupling**, not a security boundary. For true isolation, use a separate Linux VM.

**Q: Can I recreate a box without losing my `$HOME`?**

A: Yes. `cbox apply --recreate` destroys the container but preserves the shared `$HOME` (distrobox's default). Box-local `/usr` changes are lost, but your files in `$HOME` survive.

**Q: Why is there a TUI when the CLI is already clean?**

A: The TUI proves the `core::` seam is genuinely front-end-agnostic. It's the architectural acceptance test. Also, some users prefer interactive terminals over flags.

**Q: Why not async/tokio?**

A: The codebase is deliberately sync. A single background worker thread + channel keeps the TUI render non-blocking without async complexity. And the runner trait is sync by design — sync is simpler to reason about, simpler to test.

**Q: How do I manage secrets in provisioning?**

A: v1.0 does not support secrets. `[[provision]].run` is plain text in the Boxfile (trust boundary = your Boxfile). For sensitive data, store it outside the Boxfile (e.g., in `$HOME`/.env, mounted read-only) and source it in a `run` step.

**Q: Can I export provisioned apps to the host (like distrobox-export)?**

A: Not yet. v1.0 focuses on the box itself. `distrobox-export` integration is a future feature.

**Q: What if I insert a provision step in the middle of my list?**

A: The idempotency key is `(index, content-hash)`. Inserting a step shifts indices downstream → those steps re-run. This is acceptable because provisioning steps should be idempotent (Vagrant-style contract). Content-addressed ordering is deferred.

---

## Contributing

The project is pre-1.0 greenfield. All three phases are committed and stable. Contributions welcome — open issues for bugs or feature ideas.

**Development workflow:** Use `make dev-init` && `make check` to run the full test + lint suite locally (in a container). All work is mocked; no real distrobox needed.

---

## License

MIT. See `LICENSE` file.

---

**Henlavezzo** — June 2026.
