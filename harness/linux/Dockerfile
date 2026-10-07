# Linux image for the lint and test runs of ncp.
#
# ncp ships Linux releases as static musl binaries. Thus this image builds
# and tests the musl target. Build the image for one platform at a time.
# The musl-gcc of Debian makes code only for the architecture of the image.
#
# Use harness/linux/run.sh or the mise tasks "linux:*". Do not use a
# "syntax" line: that line pulls a frontend image from a second source.

# The digest is the multi-arch index of debian:trixie-slim. The pin keeps
# each build on the same base. Get a new digest with:
#   docker buildx imagetools inspect debian:trixie-slim
FROM debian:trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f AS toolchain

# gcc and libc6-dev link the build scripts and the proc macros. These run on
# the host target (gnu), not on the musl target. ring compiles C code for
# the musl target, so it needs musl-gcc from musl-tools. curl downloads
# rustup-init. cargo and rustup need ca-certificates for TLS. The tests run
# the kill program to find if a server process is alive, and procps gives
# that program. The build has no terminal, so debconf must not ask
# questions.
RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install --yes --no-install-recommends \
        ca-certificates \
        curl \
        gcc \
        libc6-dev \
        musl-tools \
        procps \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# The pinned rustup-init and its SHA-256 sums come from static.rust-lang.org.
# A changed file stops the build.
ARG RUSTUP_VERSION=1.29.1
ARG RUSTUP_SHA256_ARM64=15f6e4ce9f583b929c996c91562bad6d4454f3281de858b02cdfdef615fac433
ARG RUSTUP_SHA256_AMD64=dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71

# run.sh reads the Rust version from rust-toolchain.toml. The image and the
# host thus use the same compiler, and rustup does not download a second
# toolchain at run time. The image adds only the musl target of its own
# architecture, because musl-gcc cannot compile for a different one. This
# keeps the image smaller.
ARG RUST_VERSION

RUN test -n "$RUST_VERSION" \
    && case "$(dpkg --print-architecture)" in \
        arm64) arch=aarch64 sum="$RUSTUP_SHA256_ARM64" ;; \
        amd64) arch=x86_64 sum="$RUSTUP_SHA256_AMD64" ;; \
        *) echo "unsupported architecture" >&2; exit 1 ;; \
    esac \
    && curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
        --output /tmp/rustup-init \
        "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${arch}-unknown-linux-gnu/rustup-init" \
    && echo "${sum}  /tmp/rustup-init" | sha256sum --check --strict - \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --no-modify-path --profile minimal \
        --default-toolchain "$RUST_VERSION" \
        --component clippy,rustfmt \
        --target "${arch}-unknown-linux-musl" \
    && rm /tmp/rustup-init

# cc-rs does not find musl-gcc without help. Each image uses only the
# variable for its own architecture.
ENV CC_aarch64_unknown_linux_musl=musl-gcc \
    CC_x86_64_unknown_linux_musl=musl-gcc \
    CARGO_TARGET_DIR=/target

# run.sh mounts named volumes on these directories, and runs as the user of
# the host. A new volume copies the mode of the directory from the image, so
# all users must be able to write to it.
RUN mkdir -p "$CARGO_HOME/registry" "$CARGO_TARGET_DIR" \
    && chmod 0777 "$CARGO_HOME/registry" "$CARGO_TARGET_DIR"

WORKDIR /work
