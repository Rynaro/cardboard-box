# Cardboard Box dev toolchain — keeps the host clean.
# All cargo build/test/lint runs inside this image (see ../Makefile).
# Pinned to a 1.x stable; bump deliberately.
FROM rust:1-slim-bookworm

# Components the validation gates need (clippy for G-BUILD lint, rustfmt for fmt).
RUN rustup component add clippy rustfmt

# cbox shells out to distrobox/podman/docker at *runtime* on the host; none are
# needed to BUILD the binary, so the build image stays minimal. Integration tests
# use the MockRunner seam and require no container backend (gate G-NO-NET).

WORKDIR /work

# CARGO_HOME and the target dir are mounted as named volumes at runtime so nothing
# is written into the bind-mounted source tree except Cargo.lock (see Makefile).
ENV CARGO_HOME=/cargo
