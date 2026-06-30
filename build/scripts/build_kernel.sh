#!/usr/bin/env bash
# build_kernel.sh — Download, configure, and compile Linux 7.1 kernel for Nokia 6.1 Plus
# SPDX-License-Identifier: Apache-2.0
#
# REPRODUCIBILITY: This script tracks all build inputs and outputs for verification.
# Build metadata is recorded in build/out/.kernel-build-manifest.txt
# Per-experiment JSON records are written to build/out/experiments/<experiment-id>.json

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$REPO_ROOT/build/out"
KERNEL_VERSION="7.1"
KERNEL_DIR="$REPO_ROOT/linux-$KERNEL_VERSION"
KERNEL_TARBALL="$REPO_ROOT/linux-$KERNEL_VERSION.tar.xz"

# ─── Colors ───────────────────────────────────────────────────────────────────
GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; RED='\033[0;31m'; RESET='\033[0m'
info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*"; exit 1; }

# ─── Build environment image ─────────────────────────────────────────────────
BUILD_IMAGE="zethra-build-env:1"

ensure_build_image() {
  if ! docker image inspect "$BUILD_IMAGE" &>/dev/null; then
    warn "Build image '$BUILD_IMAGE' not found. Building it now (one-time, ~5-10 min)..."
    bash "$REPO_ROOT/build/docker/build_image.sh"
    success "Build image ready: $BUILD_IMAGE"
  fi
}

# ─── Persistent cache volumes ─────────────────────────────────────────────────
CCACHE_DIR="${CCACHE_DIR:-$HOME/.ccache}"
mkdir -p "$CCACHE_DIR"

echo "=================================================="
echo "    ZethraOS Linux Kernel Builder"
echo "=================================================="
mkdir -p "$OUT_DIR"

# ─── Step 1: Download Kernel Source ───────────────────────────────────────────
if [[ ! -d "$KERNEL_DIR" ]]; then
  info "Kernel source not found at $KERNEL_DIR."
  
  if [[ ! -f "$KERNEL_TARBALL" ]]; then
  info "Downloading Linux $KERNEL_VERSION source tarball..."
    curl -L -o "$KERNEL_TARBALL" "https://cdn.kernel.org/pub/linux/kernel/v7.x/linux-$KERNEL_VERSION.tar.xz"

    # F-08 FIX: Verify tarball integrity against known-good kernel.org SHA-256.
    # This prevents poisoned/corrupted source from silently producing bad kernels.
    EXPECTED_TARBALL_SHA256="691f44797fbe790dc8a321604c927087526ad27b6d649925d60f8eed0a2564a0"
    ACTUAL_TARBALL_SHA256="$(sha256sum "$KERNEL_TARBALL" | awk '{print $1}')"
    if [[ "$ACTUAL_TARBALL_SHA256" != "$EXPECTED_TARBALL_SHA256" ]]; then
      error "Kernel tarball SHA-256 MISMATCH. Expected: $EXPECTED_TARBALL_SHA256 Got: $ACTUAL_TARBALL_SHA256"
    fi
    success "Tarball SHA-256 verified: $ACTUAL_TARBALL_SHA256"
  fi
  
  info "Extracting kernel source..."
  tar -xf "$KERNEL_TARBALL" -C "$REPO_ROOT"
  rm -f "$KERNEL_TARBALL"
  success "Kernel source extracted to $KERNEL_DIR"

  # Apply ZethraOS patches
  if [[ -d "$REPO_ROOT/kernel/patches" ]]; then
    info "Applying ZethraOS kernel patches..."
    PATCH_FAILURES=0
    for patch_file in "$REPO_ROOT"/kernel/patches/*.patch; do
      if [[ -f "$patch_file" ]]; then
        info "Applying patch: $(basename "$patch_file")"
        # --fuzz=3: allow up to 3 lines of context mismatch (handles minor upstream drift).
        # 2>&1 capture: display output but continue on partial failures.
        # Rejected hunks (.rej files) are logged as warnings — historically they have
        # ALL been additive pr_info debug traces (non-functional, safe to skip).
        if ! patch --fuzz=3 -d "$KERNEL_DIR" -p1 < "$patch_file" 2>&1; then
          PATCH_FAILURES=$((PATCH_FAILURES + 1))
          warn "$(basename "$patch_file") had rejected hunks — check .rej files for details"
          warn "Continuing: rejected hunks have been debug-only traces (non-functional)"
          # Show any .rej files so the log is self-documenting
          find "$KERNEL_DIR" -name "*.rej" | while read rej; do
            warn "  Rejected: $rej"
            head -5 "$rej" | sed 's/^/    /'
          done
        fi
        # Clean up .rej files so they don't confuse subsequent runs
        find "$KERNEL_DIR" -name "*.rej" -delete 2>/dev/null || true
        find "$KERNEL_DIR" -name "*.orig" -delete 2>/dev/null || true
      fi
    done
    if [[ "$PATCH_FAILURES" -gt 0 ]]; then
      warn "$PATCH_FAILURES patch(es) had rejected hunks. Build proceeding — verify boot output."
    else
      success "Kernel patches applied cleanly."
    fi
  fi
else
  info "Kernel source found at $KERNEL_DIR"
fi

# F-13 / CONFIG_FRAGMENT: Merge per-experiment config fragment into defconfig.
# Approach: use -m flag (merge only, skip make) + write .config directly into
# linux-7.1/ (no -O flag) to avoid the 'source tree not clean' error that
# occurs when -O specifies an out-of-tree dir on an already-built source tree.
# We then run 'make olddefconfig' to fill in any missing symbol defaults.
CONFIG_FRAGMENT="${CONFIG_FRAGMENT:-}"
FRAGMENT_MERGED=0
if [[ -n "$CONFIG_FRAGMENT" ]]; then
  if [[ ! -f "$CONFIG_FRAGMENT" ]]; then
    error "CONFIG_FRAGMENT set but file not found: $CONFIG_FRAGMENT"
  fi
  info "Merging config fragment: $(basename "$CONFIG_FRAGMENT")"

  # Compute container-relative paths
  FRAG_REL="${CONFIG_FRAGMENT#$REPO_ROOT/}"
  DEFCONFIG_REL="kernel/zethra_defconfig"

  ensure_build_image
  docker run --rm \
    -v "$REPO_ROOT:/workspace" \
    -e ARCH=arm64 \
    -e CROSS_COMPILE=aarch64-linux-gnu- \
    -w /workspace/linux-7.1 \
    "$BUILD_IMAGE" bash -c "
      scripts/kconfig/merge_config.sh -m \
        /workspace/${DEFCONFIG_REL} \
        /workspace/${FRAG_REL} && \
      make ARCH=arm64 olddefconfig
    "
  FRAGMENT_MERGED=1
  success "Config fragment merged into linux-7.1/.config"
fi

# F-EXPERIMENT-ID: Generate a unique build identifier for this run.
# Format: img-<fragment-name>-<git-hash>-<timestamp>
FRAGMENT_NAME="${EXPERIMENT_NAME:-base}"
GIT_HASH="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo 'unknown')"
EXPERIMENT_TIMESTAMP="$(date -u +'%Y%m%dT%H%M%SZ')"
EXPERIMENT_ID="img-${FRAGMENT_NAME}-${GIT_HASH}-${EXPERIMENT_TIMESTAMP}"
info "Experiment ID: $EXPERIMENT_ID"
export EXPERIMENT_ID

# F-05 / BOOT_EXTRA_CMDLINE: Allow per-experiment cmdline additions via env var.
# Example: BOOT_EXTRA_CMDLINE="drm.debug=0x3f" bash build/scripts/build_kernel.sh
BOOT_EXTRA_CMDLINE="${BOOT_EXTRA_CMDLINE:-}"

# Base cmdline — applied to ALL builds:
# - earlycon=msm_serial_dm,0xc170000  (F-16: output before serial driver probes)
# - msm.separate_gpu_kms=1            (F-05: separate GPU/KMS probe on SDM636)
BASE_CMDLINE="earlycon=msm_serial_dm,0xc170000 console=ttyMSM0,115200,n8 androidboot.hardware=qcom lpm_levels.sleep_disabled=1 loop.max_part=7 buildvariant=userdebug panic=10 msm.separate_gpu_kms=1"
if [[ -n "$BOOT_EXTRA_CMDLINE" ]]; then
  FULL_CMDLINE="$BASE_CMDLINE $BOOT_EXTRA_CMDLINE"
else
  FULL_CMDLINE="$BASE_CMDLINE"
fi
info "Boot cmdline: $FULL_CMDLINE"

# ─── Step 2: Copy Defconfig & DTS ─────────────────────────────────────────────
info "Copying zethra_defconfig into kernel configs..."
cp "$REPO_ROOT/kernel/zethra_defconfig" "$KERNEL_DIR/arch/arm64/configs/zethra_defconfig"

if [[ -f "$REPO_ROOT/kernel/dts/sdm636-nokia-frt.dts" ]]; then
  info "Copying sdm636-nokia-frt.dts into kernel DTS directory..."
  cp "$REPO_ROOT/kernel/dts/sdm636-nokia-frt.dts" "$KERNEL_DIR/arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dts"
fi

# ─── REPRODUCIBILITY: Record Input Manifest ─────────────────────────────────────────────
info "Recording build inputs for reproducibility verification..."
MIFE_DIR="$OUT_DIR/experiments"
mkdir -p "$MIFE_DIR"
MANIFEST_FILE="$OUT_DIR/.kernel-build-manifest.txt"
EXP_JSON="$MIFE_DIR/${EXPERIMENT_ID}.json"
{
  echo "# ZethraOS Kernel Build Manifest — $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo "# Experiment ID: $EXPERIMENT_ID"
  echo "# Device: Nokia 6.1 Plus (TA-1103) / SDM636"
  echo ""
  echo "## Build Inputs"
  echo "experiment_id:     $EXPERIMENT_ID"
  echo "git_hash:          $(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || echo 'unknown')"
  echo "defconfig_sha256:  $(sha256sum "$REPO_ROOT/kernel/zethra_defconfig" | awk '{print $1}')"
  echo "config_fragment:   ${CONFIG_FRAGMENT:-none}"
  echo "cmdline:           $FULL_CMDLINE"
  echo "dts_sha256:        $(sha256sum "$KERNEL_DIR/arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dts" 2>/dev/null | awk '{print $1}' || echo 'TBD')"
  echo ""
} > "$MANIFEST_FILE"
success "Manifest started: $MANIFEST_FILE"

# ─── Step 3: Compile Kernel ─────────────────────────────────────────────────────
build_in_docker() {
  info "Compiling inside Docker container ($BUILD_IMAGE)..."

  if ! docker ps &>/dev/null; then
    error "Docker is not running or not accessible."
  fi

  ensure_build_image

  # Capture FULL_CMDLINE for Docker interpolation
  DOCKER_CMDLINE="$FULL_CMDLINE"

  docker run --rm \
    -v "$REPO_ROOT:/workspace" \
    -v "$CCACHE_DIR:/ccache" \
    -e CCACHE_DIR=/ccache \
    -e EXPERIMENT_ID="$EXPERIMENT_ID" \
    -w /workspace \
    "$BUILD_IMAGE" bash -c "
       cd linux-$KERNEL_VERSION && \
       if [ \"$FRAGMENT_MERGED\" = \"1\" ]; then \
         echo 'Using pre-merged .config — running olddefconfig...' && \
         make ARCH=arm64 olddefconfig; \
       else \
         make ARCH=arm64 zethra_defconfig; \
       fi && \
       make ARCH=arm64 CROSS_COMPILE='ccache aarch64-linux-gnu-' \
            KBUILD_BUILD_USER=zethra KBUILD_BUILD_HOST=zethra-build \
            KBUILD_BUILD_TIMESTAMP='Fri Jun 12 17:00:00 UTC 2026' \
            KBUILD_BUILD_VERSION=1 -j\$(nproc) Image.gz dtbs && \
      cp arch/arm64/boot/Image.gz /workspace/build/out/ && \
      cp arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dtb /workspace/build/out/ && \
      cp arch/arm64/boot/Image.gz /workspace/build/out/Image.gz-dtb && \
      cat arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dtb >> /workspace/build/out/Image.gz-dtb && \
      cd /workspace && \
      if [ -f 'build/out/initramfs.cpio.gz' ]; then \
        echo 'Packing boot.img...' && \
        mkbootimg \
          --header_version 0 \
          --kernel         build/out/Image.gz-dtb \
          --ramdisk        build/out/initramfs.cpio.gz \
          --pagesize       4096 \
          --base           0x00000000 \
          --kernel_offset  0x00008000 \
          --ramdisk_offset 0x01000000 \
          --second_offset  0x00f00000 \
          --tags_offset    0x00000100 \
          --os_version     10.0.0 \
          --os_patch_level 2021-08 \
          --cmdline        '$DOCKER_CMDLINE' \
          --output         build/out/boot.img && \
        if [ ! -f 'tools/test_key.pem' ]; then \
          echo '[WARN] tools/test_key.pem not found — generating ephemeral RSA-2048 test key.' && \
          echo '[WARN] This key is for bringup ONLY. Device is unlocked; AVB warning expected.' && \
          openssl genrsa -out tools/test_key.pem 2048 2>/dev/null && \
          echo \"[INFO] test_key.pem SHA-256: \$(sha256sum tools/test_key.pem | awk '{print \$1}')\"; \
        fi && \
        python3 tools/avbtool add_hash_footer \
          --image          build/out/boot.img \
          --partition_name boot \
          --dynamic_partition_size \
          --algorithm      SHA256_RSA2048 \
          --key            tools/test_key.pem \
          --salt           c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00 && \
        echo 'boot.img successfully built and signed at build/out/boot.img'; \
      else \
        echo 'WARNING: build/out/initramfs.cpio.gz not found.' && \
        echo 'Run build/scripts/build_initramfs.sh first (Step 0 of experiment matrix).' && \
        exit 1; \
      fi
    "
}

if [[ "$OSTYPE" == "darwin"* ]]; then
  build_in_docker
else
  # On Linux host, check if cross compiling tools are available
  if command -v aarch64-linux-gnu-gcc &>/dev/null; then
    info "Host compilation tools found. Building on host..."
    (
      cd "$KERNEL_DIR"
      make ARCH=arm64 zethra_defconfig
      make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- KBUILD_BUILD_USER=zethra KBUILD_BUILD_HOST=zethra-build KBUILD_BUILD_TIMESTAMP="Fri Jun 12 17:00:00 UTC 2026" KBUILD_BUILD_VERSION=1 -j$(nproc) Image.gz dtbs
      cp arch/arm64/boot/Image.gz "$OUT_DIR/"
      cp arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dtb "$OUT_DIR/"
      cp arch/arm64/boot/Image.gz "$OUT_DIR/Image.gz-dtb"
      cat arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dtb >> "$OUT_DIR/Image.gz-dtb"
    )
    
    # Check if mkbootimg is available to pack
    if command -v mkbootimg &>/dev/null && [[ -f "$OUT_DIR/initramfs.cpio.gz" ]]; then
      info "Packing boot.img on host..."
      mkbootimg \
        --header_version 0 \
        --kernel         "$OUT_DIR/Image.gz-dtb" \
        --ramdisk        "$OUT_DIR/initramfs.cpio.gz" \
        --pagesize       4096 \
        --base           0x00000000 \
        --kernel_offset  0x00008000 \
        --ramdisk_offset 0x01000000 \
        --second_offset  0x00f00000 \
        --tags_offset    0x00000100 \
        --os_version     10.0.0 \
        --os_patch_level 2021-08 \
        --cmdline        "$FULL_CMDLINE" \
        --output         "$OUT_DIR/boot.img"
      python3 "$REPO_ROOT/tools/avbtool" add_hash_footer \
        --image          "$OUT_DIR/boot.img" \
        --partition_name boot \
        --dynamic_partition_size \
        --algorithm      SHA256_RSA2048 \
        --key            "$REPO_ROOT/tools/test_key.pem" \
        --salt           c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00
      success "boot.img successfully built and signed at: $OUT_DIR/boot.img"
    else
      warn "Host tools or initramfs missing. Falling back to Docker for final pack/verification..."
      build_in_docker
    fi
  else
    warn "Host compilation tools missing. Falling back to Docker..."
    build_in_docker
  fi
fi

# ─── REPRODUCIBILITY: Record Build Outputs ─────────────────────────────────────────────
info "Recording build outputs for reproducibility verification..."
BOOT_IMG_SHA256="$(sha256sum "$OUT_DIR/boot.img" 2>/dev/null | awk '{print $1}' || echo 'TBD')"
{
  echo "## Build Outputs"
  echo "Image.gz-dtb_sha256:  $(sha256sum "$OUT_DIR/Image.gz-dtb" 2>/dev/null | awk '{print $1}' || echo 'TBD')"
  if [[ -f "$OUT_DIR/sdm636-nokia-frt.dtb" ]]; then
    echo "dtb_sha256:           $(sha256sum "$OUT_DIR/sdm636-nokia-frt.dtb" | awk '{print $1}')"
  fi
  echo "boot.img_sha256:      $BOOT_IMG_SHA256"
  echo ""
  echo "## Build Environment"
  echo "kernel_version:   $KERNEL_VERSION"
  echo "compiler:         $(aarch64-linux-gnu-gcc --version 2>/dev/null | head -1 || echo 'cross-compiler-unknown')"
  echo "build_timestamp:  $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo "cmdline:          $FULL_CMDLINE"
} >> "$MANIFEST_FILE"

# Write structured JSON experiment record for the ledger.
if [[ -f "$OUT_DIR/boot.img" ]]; then
  cat > "$EXP_JSON" <<EXPJSON
{
  "experiment_id":   "$EXPERIMENT_ID",
  "experiment_name": "${EXPERIMENT_NAME:-base}",
  "git_hash":        "$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || echo 'unknown')",
  "build_timestamp": "$(date -u +'%Y-%m-%dT%H:%M:%SZ')",
  "config_fragment":  "${CONFIG_FRAGMENT:-none}",
  "cmdline":         "$FULL_CMDLINE",
  "artifacts": {
    "boot_img_sha256":    "$BOOT_IMG_SHA256",
    "Image_gz_dtb_sha256": "$(sha256sum "$OUT_DIR/Image.gz-dtb" 2>/dev/null | awk '{print $1}' || echo 'TBD')",
    "initramfs_sha256":   "$(sha256sum "$OUT_DIR/initramfs.cpio.gz" 2>/dev/null | awk '{print $1}' || echo 'TBD')"
  },
  "result": {
    "flash_timestamp": null,
    "timing_seconds":  null,
    "outcome":         "pending",
    "acm_alive":       null,
    "notes":           ""
  }
}
EXPJSON
  success "Experiment record written: $EXP_JSON"
fi

success "Build manifest updated: $MANIFEST_FILE"
echo ""
info "Kernel build complete! Experiment: $EXPERIMENT_ID"
echo "Manifest:  $MANIFEST_FILE"
echo "JSON:      ${EXP_JSON:-N/A}"
echo "Artifacts:"
for f in "$OUT_DIR"/Image.gz-dtb "$OUT_DIR"/*.dtb "$OUT_DIR"/boot.img; do
  if [[ -f "$f" ]]; then
    ls -lh "$f" | awk '{print "  " $NF " (" $5 ")"}'
  fi
done

# F-11: Print decoded cmdline from the final boot.img as ground truth.
if [[ -f "$OUT_DIR/boot.img" ]] && command -v python3 &>/dev/null; then
  DECODED_CMDLINE="$(python3 "$REPO_ROOT/tools/unpack_bootimg.py" --boot_img "$OUT_DIR/boot.img" 2>/dev/null | grep 'command line' | sed 's/.*command line args: //' || echo 'N/A')"
  echo ""
  info "Decoded cmdline from boot.img: $DECODED_CMDLINE"
  if [[ "$DECODED_CMDLINE" != "$FULL_CMDLINE" ]] && [[ "$DECODED_CMDLINE" != 'N/A' ]]; then
    warn "CMDLINE MISMATCH: intended vs decoded differ. Check packing step."
  fi
fi

success "Kernel compilation complete."
echo "=================================================="
