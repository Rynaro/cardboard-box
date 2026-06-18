# Cardboard Box — containerized dev workflow.
#
# Every cargo invocation runs inside the `cbox-dev` image so the HOST stays clean:
#   - Rust toolchain lives in the image (not installed on the host).
#   - Build artifacts live in the `cbox_target` named volume (never on the host FS).
#   - The cargo registry/cache lives in the `cbox_cargo` named volume.
#   - The source tree is bind-mounted; only Cargo.lock is written back (as YOU, not root).
#
# First-time setup:   make dev-init
# Then:               make build | make test | make lint | make fmt | make shell
#
# NOTE: do NOT run bare `cargo` on the host — that defeats the clean-host guarantee.

IMAGE   := cbox-dev
UID     := $(shell id -u)
GID     := $(shell id -g)
ENGINE  ?= docker

# Common docker-run incantation: run as the host user, mount source + volumes.
# Target dir is mounted at /target (OUTSIDE the /work bind mount) so Docker never
# creates a root-owned `target/` stub inside the source tree — the host stays pristine.
RUN := $(ENGINE) run --rm \
	--user $(UID):$(GID) \
	-e CARGO_HOME=/cargo \
	-e CARGO_TARGET_DIR=/target \
	-v cbox_cargo:/cargo \
	-v cbox_target:/target \
	-v "$(CURDIR)":/work \
	-w /work \
	$(IMAGE)

.PHONY: dev-init image volumes build release dist install test lint lint-lean fmt fmt-check check hooks shell clean nuke

## One-time: build the toolchain image, prepare writable named volumes, and install git hooks.
dev-init: image volumes hooks
	@echo "✓ dev environment ready — try: make build"

image:
	$(ENGINE) build -t $(IMAGE) .devcontainer

# Named volumes are root-owned on creation; chown them to the host user so the
# --user-constrained cargo can write cache + artifacts into them.
volumes:
	-$(ENGINE) volume create cbox_cargo >/dev/null
	-$(ENGINE) volume create cbox_target >/dev/null
	$(ENGINE) run --rm -v cbox_cargo:/cargo -v cbox_target:/target $(IMAGE) \
		sh -c 'mkdir -p /cargo /target && chown -R $(UID):$(GID) /cargo /target'

build:
	$(RUN) cargo build

release:
	$(RUN) cargo build --release

# The release binary is built inside the cbox_target volume (clean-host guarantee),
# so it must be EXTRACTED to be usable. `dist` drops it in ./dist; `install` copies
# it onto your PATH (override PREFIX to change where).
PREFIX ?= $(HOME)/.local

dist: release
	@mkdir -p "$(CURDIR)/dist"
	$(ENGINE) run --rm --user $(UID):$(GID) \
		-v cbox_target:/target -v "$(CURDIR)/dist":/out \
		$(IMAGE) cp /target/release/cbox /out/cbox
	@echo "✓ binary extracted to ./dist/cbox"

install: release
	@mkdir -p "$(PREFIX)/bin"
	$(ENGINE) run --rm --user $(UID):$(GID) \
		-v cbox_target:/target -v "$(PREFIX)/bin":/out \
		$(IMAGE) cp /target/release/cbox /out/cbox
	@echo "✓ installed cbox to $(PREFIX)/bin/cbox  (ensure $(PREFIX)/bin is on your PATH)"

test:
	$(RUN) cargo test

lint:
	$(RUN) cargo clippy --all-targets --all-features -- -D warnings

## Lint the lean (TUI-off) build too, so the feature matrix can't regress.
lint-lean:
	$(RUN) cargo clippy --all-targets --no-default-features -- -D warnings

fmt:
	$(RUN) cargo fmt

fmt-check:
	$(RUN) cargo fmt --check

## Everything CI would gate on (G-BUILD/G-UNIT/G-MOCK/G-NO-NET): fmt + clippy
## (both feature configs) + debug build + release build + tests.
check: fmt-check lint lint-lean build release test

## Wire .githooks/ as the repo's git hook directory (idempotent).
## Bypass any individual hook with `git push --no-verify`.
hooks:
	git config core.hooksPath .githooks
	@echo "✓ git hooks wired — .githooks/pre-push will run make check before every push"

## Interactive shell in the toolchain container (for poking around).
shell:
	$(ENGINE) run --rm -it --user $(UID):$(GID) \
		-e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/target \
		-v cbox_cargo:/cargo -v cbox_target:/target \
		-v "$(CURDIR)":/work -w /work $(IMAGE) bash

## Remove build artifacts (the target volume); keeps the cargo cache.
clean:
	-$(ENGINE) volume rm cbox_target >/dev/null 2>&1 || true

## Remove the image and ALL named volumes (full reset).
nuke: clean
	-$(ENGINE) volume rm cbox_cargo >/dev/null 2>&1 || true
	-$(ENGINE) rmi $(IMAGE) >/dev/null 2>&1 || true
