# ─── ZethraOS Build Environment ──────────────────────────────────────────────
# Pre-baked image containing every tool needed to:
#   1. Build Linux 7.1 kernel for Nokia 6.1 Plus (SDM636/aarch64)
#   2. Cross-compile Rust userspace services (aarch64-unknown-linux-musl)
#   3. Package initramfs + boot.img (mkbootimg + avbtool)
#
# Build once:  bash build/docker/build_image.sh
# Run via:     build_kernel.sh / build_initramfs.sh (automatically)
#
# Persistent cache volumes mounted at runtime (NOT baked in):
#   ~/.cargo/registry  → /root/.cargo/registry   (crate downloads)
#   ~/.cargo/git       → /root/.cargo/git         (git dependencies)
#   ~/.ccache          → /ccache                  (compiler cache)
#   <repo>/target      → /workspace/target        (cargo build artefacts)
# ─────────────────────────────────────────────────────────────────────────────

FROM debian:bookworm-slim AS base

# Prevent apt from blocking on interactive prompts
ENV DEBIAN_FRONTEND=noninteractive
ENV TZ=UTC

# ─── System packages ─────────────────────────────────────────────────────────
# Installed once at image-build time — never re-downloaded per build run.
RUN apt-get update -qq && \
    apt-get install -y -qq --no-install-recommends \
        # Kernel cross-compilation
        gcc-aarch64-linux-gnu \
        make bc libssl-dev flex bison \
        python3 python3-pip python3-pyelftools \
        # Initramfs packaging
        busybox-static cpio gzip \
        # Build tools
        ccache curl wget xz-utils ca-certificates \
        # Native C runtime — provides Scrt1.o, crti.o needed for host build scripts
        # on arm64 Docker where aarch64-linux-gnu-gcc lacks its own sysroot.
        libc6-dev \
        # musl toolchain for aarch64-unknown-linux-musl Rust target
        # Provides musl-gcc which wraps native gcc with musl headers and runtime.
        musl-tools \
        # Debugging / verification utilities
        file binutils-aarch64-linux-gnu \
        binutils xxd \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# ─── Rust toolchain ──────────────────────────────────────────────────────────
# Bake the exact toolchain + cross-target so 'rustup target add' never runs
# during a build. The stable toolchain pin comes from BUILD_ARG for auditability.
ARG RUST_VERSION=stable
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=/usr/local/cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y \
        --no-modify-path \
        --profile minimal \
        --default-toolchain ${RUST_VERSION} \
        --target aarch64-unknown-linux-musl && \
    # Verify cross target is installed
    rustup target list --installed | grep aarch64-unknown-linux-musl && \
    # Print versions for audit trail
    rustc --version && cargo --version && \
    # Warm the registry index once so first cargo build is faster
    cargo search --limit 0 serde 2>/dev/null || true

# ─── mkbootimg (Qualcomm boot image packer) ──────────────────────────────────
# Downloaded once at image-build time, SHA-256 verified.
# If kernel.org ever changes this binary, the build fails loudly here — not at
# runtime inside a flash loop.
ARG MKBOOTIMG_URL=https://launchpadlibrarian.net/810765814/mkbootimg
# SHA-256 verified 2026-06-30 by direct download and inspection.
# File is a Python 3 script (AOSP copyright 2015) — architecture-independent.
# If this ever mismatches, re-verify at: https://launchpad.net/ubuntu/+source/android-sdk-meta
ARG MKBOOTIMG_SHA256=b682574d42fd5c5cb852e7e76996b0e460808f10f47d81fc01e7f6db1697da4e

RUN curl -fsSL -o /usr/local/bin/mkbootimg "${MKBOOTIMG_URL}" && \
    ACTUAL="$(sha256sum /usr/local/bin/mkbootimg | awk '{print $1}')" && \
    if [ "${ACTUAL}" != "${MKBOOTIMG_SHA256}" ]; then \
        echo "ERROR: mkbootimg SHA-256 MISMATCH" && \
        echo "  expected: ${MKBOOTIMG_SHA256}" && \
        echo "  actual:   ${ACTUAL}" && \
        exit 1; \
    fi && \
    echo "mkbootimg SHA-256 verified: ${ACTUAL}" && \
    chmod +x /usr/local/bin/mkbootimg

# ─── ccache configuration ────────────────────────────────────────────────────
# CCACHE_DIR is a runtime volume mount — see run_*.sh scripts.
# Set sane defaults baked into the image.
ENV CCACHE_DIR=/ccache
ENV CCACHE_MAXSIZE=5G
ENV CCACHE_COMPRESS=1
ENV CCACHE_COMPRESSLEVEL=6

# Symlink ccache for aarch64 cross-compiler wrappers
RUN for tool in gcc g++ cpp; do \
        ln -sf /usr/bin/ccache /usr/local/bin/aarch64-linux-gnu-${tool}; \
    done

# ─── Runtime environment ─────────────────────────────────────────────────────
WORKDIR /workspace

# Labels for traceability
LABEL org.opencontainers.image.title="ZethraOS Build Environment"
LABEL org.opencontainers.image.description="Cross-compile environment for Nokia 6.1 Plus (SDM636)"
LABEL org.opencontainers.image.source="https://github.com/ZethraOS/os"
ARG BUILD_DATE
LABEL org.opencontainers.image.created="${BUILD_DATE}"
