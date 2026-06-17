# The Cardboard Box (`cbox`)

A cozy distrobox manager. Vagrant-style declarative reproducibility for your
development containers.

```
cbox create web-dev -i fedora-toolbox:latest --docker host
cbox enter web-dev
cbox list
cbox inspect web-dev --json
cbox doctor
```

## What it does

`cbox` wraps [distrobox](https://github.com/89luca89/distrobox) — the excellent
tool for running Linux containers with host integration. `cbox` adds:

- **Boxfile.toml** — a declarative manifest (think `Vagrantfile`) that describes
  your box: image, packages, mounts, and docker access mode.
- **The docker spectrum** — a single `docker = none|host|nested` knob that maps
  to the right socket mounts and package bundles automatically.
- **Cozy output** — consistent chrome, `--json` machine output, and `--no-color`
  support everywhere.
- **Testable architecture** — all distrobox invocations go through a `DistroboxRunner`
  trait, so the full command logic runs in CI without real distrobox.

## Requirements

- `distrobox` >= 1.6 on PATH
- `podman` (preferred) or `docker`
- Linux (distrobox is Linux-only)

## Build

```bash
# Containerized build — host stays clean:
make build          # cargo build in the cbox-dev container
make test           # cargo test
make check          # fmt + clippy + build + test (full CI gate)
make shell          # interactive shell in the toolchain container
```

## Commands (Phase 1)

| Command | Description |
|---|---|
| `cbox create [NAME] [flags]` | Create a box (imperative or from Boxfile) |
| `cbox create --file Boxfile.toml` | Declarative create |
| `cbox list [--json]` | List boxes |
| `cbox rm <NAME> [-f] [-y]` | Remove a box |
| `cbox enter <NAME> [-- CMD...]` | Enter a box (interactive) |
| `cbox inspect <NAME> [--json]` | Inspect a box |
| `cbox edit [NAME\|--file PATH]` | Edit the Boxfile for a box |
| `cbox doctor [--json]` | Check environment |

### Global flags

```
--json        Machine output (JSON to stdout; chrome to stderr)
-q/--quiet    Suppress cozy chrome
-v/--verbose  Show spawned argv (-v) or stream child output (-vv)
--no-color    Disable ANSI (auto-detected from TTY/NO_COLOR)
-y/--yes      Skip confirmations
--dry-run     Print would-be argv, execute nothing
--backend     Override backend detection (podman|docker)
```

## Boxfile.toml

```toml
name  = "web-dev"
image = "fedora-toolbox:latest"
packages = ["git", "ripgrep", "fd-find"]
docker = "none"   # none | host | nested

[[mounts]]
host  = "/home/me/code"
guest = "/code"
mode  = "rw"

[sandbox]
unshare = ["netns", "ipc"]

[box]
home = ""
hostname = ""

[[provision]]
type = "shell"
run  = "rustup default stable"   # Phase 2: applied by `cbox apply`
```

### docker = none | host | nested

| Mode | What it means |
|---|---|
| `none` (default) | No container runtime access; optional `--unshare-*` hardening |
| `host` | Mounts the host podman/docker socket into the box (containers visible on host) |
| `nested` | Docker-in-Docker: private daemon inside the box, not visible on host |

## Exit codes

| Code | Meaning |
|---|---|
| 0 | OK |
| 64 | Bad usage / invalid name |
| 65 | Boxfile validation error |
| 69 | Box not found |
| 70 | distrobox missing |
| 74 | Spawn/IO failure |
| 75 | Backend unreachable |
| 125 | distrobox/backend exited non-zero |

## Architecture

```
main.rs  →  cli/  (clap parse + render)
              ↓
           core/  (front-end-agnostic logic; TUI reuses this in Phase 3)
              ↓
         dbox/   (DistroboxRunner trait + RealRunner + MockRunner)
         boxfile/ (Boxfile.toml model, validation, docker_mode flag bundles)
```

## Phases

- **Phase 1 (this):** CLI lifecycle — all 7 subcommands, Boxfile parse+validate,
  docker spectrum flag mapping, MockRunner test seam.
- **Phase 2 (planned):** `cbox apply` / `cbox up` — execute `[[provision]]` steps,
  idempotent re-apply.
- **Phase 3 (planned):** TUI that dogfoods the same `core::` functions.
