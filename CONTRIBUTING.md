# Contributing to cbox

Thank you for your interest in contributing to `cbox`.

---

## Development workflow

All builds and tests run inside a container (the clean-host guarantee):

```bash
make dev-init   # one-time: build the toolchain image + named volumes + install git hooks
make check      # full gate: fmt + clippy (both feature configs) + build (debug + release) + test
```

Never run bare `cargo` on the host. Use `make` targets — see the `Makefile` for
the full list and `README.md` for detailed documentation.

### Pre-push hook

`make dev-init` installs a pre-push git hook (via `git config core.hooksPath .githooks`)
that runs `make check` before every push. This catches fmt/clippy/test failures before
they reach CI.

- If the `cbox-dev` image is not built, the hook tells you to run `make dev-init` rather
  than producing a cryptic Docker error.
- To bypass the hook for a single push: `git push --no-verify`.
- To install (or re-install) the hook without a full `dev-init`: `make hooks`.

---

## Commit convention (required)

`cbox` uses **Conventional Commits** and **squash-merge**. The PR title becomes
the commit subject on `main`, and `release-please` reads it to compute the next
SemVer version and generate `CHANGELOG.md`.

**Your PR title must match:**

```
<type>[optional scope]: <short description>
```

**Allowed types:**

| Type | When to use | Triggers release? |
|---|---|---|
| `feat` | A new user-visible feature | yes — minor bump |
| `fix` | A bug fix | yes — patch bump |
| `perf` | A performance improvement | yes — patch bump |
| `docs` | Documentation only | no |
| `chore` | Maintenance, dependency bumps, etc. | no |
| `ci` | CI/CD changes | no |
| `build` | Build system changes | no |
| `refactor` | Code restructuring (no behaviour change) | no |
| `test` | Adding or fixing tests | no |
| `style` | Formatting, whitespace (no logic change) | no |
| `revert` | Reverting a prior commit | patch bump |

**Breaking changes** — append `!` to the type, or add a `BREAKING CHANGE:` footer:

```
feat!: rename --docker flag to --mode
```

Pre-1.0 (`0.x`), breaking changes bump the **minor** version (not the major),
per the `bump-minor-pre-major` setting. Post-1.0, they bump the major.

A PR-title check workflow (`.github/workflows/pr-title.yml`) enforces this
automatically. If your PR fails the check, update the PR title — not the commit.

### Examples

```
feat: add cbox export command
fix: handle empty Boxfile gracefully
feat(tui): add doctor panel keyboard shortcut
docs: update quickstart in README
chore: bump ratatui to 0.29
ci: add actionlint step to make check
```

For more detail on the SemVer mapping and the release process, see [RELEASING.md](RELEASING.md).

---

## Pull request checklist

- [ ] `make check` passes locally (or in CI on the PR).
- [ ] PR title follows Conventional Commits (the PR-title check will catch it if not).
- [ ] New behaviour is covered by tests (all tests are mock-driven; no real distrobox needed).
- [ ] If you added a new CLI flag, update the argv builder tests in `tests/argv_builder.rs`.

---

## License

By contributing you agree that your contribution is released under the [MIT License](LICENSE).
