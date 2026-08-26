---
name: cbox-boxfile
description: Inspect an existing project, compose or update its Boxfile.toml for cbox, and validate it with the real cbox parser.
license: MIT
compatibility: Requires cbox with the validate command; targets Linux projects using distrobox.
metadata:
  project: cardboard-box
  artifact: Boxfile.toml
---

# Compose a cbox Boxfile

Use this skill when the user wants a development box described by
`Boxfile.toml`. The deliverable is the project file, not an agent configuration
file and not a command that creates a live box.

## Workflow

1. Inspect the repository before writing. Read its language manifests, lockfiles,
   documented setup commands, expected services, and existing container/devbox
   configuration. Do not invent dependencies that the project does not need.
2. Ask only for choices that cannot be inferred safely, such as host Docker
   access or a host path to mount. Prefer a minimal file when information is
   missing.
3. Compose `Boxfile.toml` using [the field reference](references/boxfile.md).
   Start from [the minimal example](examples/minimal/Boxfile.toml), then add only
   project-backed fields.
4. Never put credentials or tokens in the file. Declare secret names under
   `[secrets]` with `from = "keyring"`; tell the user to populate them with
   `cbox secret set <BOX> <KEY>`.
5. Make every shell provision step non-interactive, repeatable, and safe to run
   again. Prefer package declarations for OS packages. Use copy steps only for
   project-owned files whose source path is relative to the Boxfile directory.
6. Run `cbox validate --file Boxfile.toml`. Fix all errors and rerun until it
   exits 0. Treat warnings as review items; explain any warning left in place.
7. Summarize what was inferred and what the user may need to customize. Do not
   run `cbox up`, `apply`, `create`, secret commands, or backend commands unless
   the user separately asks for that state-changing action.

## Guardrails

- `name` is required and must match `^[A-Za-z0-9][A-Za-z0-9_.-]*$`.
- Host and guest mount paths must be absolute; every guest path must be unique.
- `docker = "none"` is the safe default. Use `host` or `nested` only when the
  project actually needs it and the user accepts the coupling/security tradeoff.
- Prefer `isolated = true` for a project-specific home. An explicit `box.home`
  takes precedence.
- Sandbox unshare values are `netns`, `ipc`, `process`, `devsys`, and `groups`,
  or the string `"all"`. Combining unshare with Docker access produces a warning.
- Plain `[env]` values are committed to source control. Secret values never are.
- Unknown top-level fields are warnings, while typos inside `[box]`, `[sandbox]`,
  and secret entries are errors. Do not rely on ignored future fields.
- Validation is syntax and schema validation only. It does not prove that an
  image, package, host path, or provisioning command exists.

## Output quality

Keep the Boxfile readable, with comments only where they capture a real project
decision. Preserve unrelated user content when updating an existing file. The
examples in this skill are fixtures validated by cbox's own parser.
