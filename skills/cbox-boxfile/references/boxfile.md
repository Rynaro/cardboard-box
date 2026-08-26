# Boxfile.toml field reference

```toml
name = "project-dev"                 # required string
image = "registry.fedoraproject.org/fedora-toolbox:latest"
packages = ["git", "make"]          # strings; default []
docker = "none"                      # none | host | nested

[[mounts]]
host = "/absolute/host/path"
guest = "/workspace"
mode = "rw"                          # ro | rw; default rw

[sandbox]
unshare = ["netns", "ipc"]          # "all" or a list
init = false

[box]
home = ""                            # optional absolute/custom home
hostname = ""
pull = false
isolated = true

[[provision]]
type = "shell"
run = "make bootstrap"               # required for shell

[[provision]]
type = "copy"
src = "./config/example.conf"        # relative to Boxfile directory
dst = "/home/dev/.config/app.conf"   # required for copy

[env]
LOG_LEVEL = "info"                   # plaintext only

[secrets]
API_TOKEN = { persist = false, from = "keyring" }
```

Defaults: `image` is Fedora Toolbox latest, lists/maps are empty, Docker is
disabled, mount mode is read-write, and booleans are false except secret
`persist`, which defaults to true.

Limits enforced by the parser: at most 64 combined env and secret entries; keys
are at most 128 characters and must be POSIX environment names; env values are
at most 4096 bytes. A key cannot appear in both `[env]` and `[secrets]`.

Validate without creating or inspecting a box:

```sh
cbox validate --file Boxfile.toml
cbox --json validate --file Boxfile.toml
```

Invalid TOML or schema data exits 65. Machine-readable success and failure are
available with the global `--json` flag.
