#!/usr/bin/env bash
# build_kernel.sh — Download, configure, and compile Linux 7.1 kernel for Nokia 6.1 Plus
# SPDX-License-Identifier: Apache-2.0
#
# REPRODUCIBILITY: This script now tracks all build inputs and outputs for verification.
# Build metadata is recorded in build/out/.kernel-build-manifest.txt

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
    for patch_file in "$REPO_ROOT"/kernel/patches/*.patch; do
      if [[ -f "$patch_file" ]]; then
        info "Applying patch: $(basename "$patch_file")"
        patch -d "$KERNEL_DIR" -p1 < "$patch_file"
      fi
    done
    success "Kernel patches applied."
  fi
else
  info "Kernel source found at $KERNEL_DIR"
fi

# F-13 / CONFIG_FRAGMENT: If a per-experiment config fragment is provided via env var,
# merge it on top of the base defconfig now — before copying into the kernel tree.
# merge_config.sh only exists after tarball extraction (above), so this is safe here.
CONFIG_FRAGMENT="${CONFIG_FRAGMENT:-}"
if [[ -n "$CONFIG_FRAGMENT" ]]; then
  if [[ ! -f "$CONFIG_FRAGMENT" ]]; then
    error "CONFIG_FRAGMENT set but file not found: $CONFIG_FRAGMENT"
  fi
  info "Merging config fragment: $(basename "$CONFIG_FRAGMENT")"
  # Use the kernel's own merge_config.sh to correctly handle =n overrides.
  # Output goes to a temp file; we then use that as the effective defconfig.
  MERGED_DEFCONFIG="$REPO_ROOT/build/tmp/zethra_defconfig_merged"
  mkdir -p "$REPO_ROOT/build/tmp"
  ARCH=arm64 "$KERNEL_DIR/scripts/kconfig/merge_config.sh" \
    -O "$REPO_ROOT/build/tmp" \
    "$REPO_ROOT/kernel/zethra_defconfig" \
    "$CONFIG_FRAGMENT"
  cp "$REPO_ROOT/build/tmp/.config" "$MERGED_DEFCONFIG"
  # Copy merged result as a pre-set .config instead of using defconfig target.
  cp "$MERGED_DEFCONFIG" "$KERNEL_DIR/arch/arm64/configs/zethra_defconfig"
  success "Config fragment merged: $(basename "$CONFIG_FRAGMENT")"
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

# ─── Step 3: Compile Kernel ───────────────────────────────────────────────
build_in_docker() {
  info "Compiling inside Docker container (ubuntu:24.04)..."
  
  # Ensure docker is running
  if ! docker ps &>/dev/null; then
    echo "Error: Docker is not running or not accessible."
    exit 1
  fi

  # F-09 FIX: Verify mkbootimg binary SHA-256 before running it.
  # This binary has full control over the boot image layout — must be trusted.
  MKBOOTIMG_SHA256_EXPECTED="b09ac0a84e4dc06a77c7eb5c2d551440f36ef0f02d3f87a8c7ed246fcfe00abc"

  # F-14 FIX: Mount a persistent ccache directory to speed up rebuilds.
  # First build ~35 min; subsequent builds with ccache ~3-5 min.
  CCACHE_DIR="${CCACHE_DIR:-$HOME/.ccache}"
  mkdir -p "$CCACHE_DIR"

  # Capture FULL_CMDLINE for Docker interpolation (escape for bash -c heredoc)
  DOCKER_CMDLINE="$FULL_CMDLINE"

  docker run --rm \
    -v "$REPO_ROOT:/workspace" \
    -v "$CCACHE_DIR:/ccache" \
    -e CCACHE_DIR=/ccache \
    -e EXPERIMENT_ID="$EXPERIMENT_ID" \
    -w /workspace \
    ubuntu:24.04 bash -c "
      apt-get update -qq && \
      DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
        ccache wget xz-utils gcc-aarch64-linux-gnu bc libssl-dev flex bison make python3 python3-pip && \
      wget -q -O /usr/bin/mkbootimg https://launchpadlibrarian.net/810765814/mkbootimg && \
      chmod +x /usr/bin/mkbootimg && \
      ACTUAL_MKBOOTIMG_SHA256=\"\$(sha256sum /usr/bin/mkbootimg | awk '{print \$1}')\" && \
      if [ \"\$ACTUAL_MKBOOTIMG_SHA256\" != \"$MKBOOTIMG_SHA256_EXPECTED\" ]; then \
        echo 'ERROR: mkbootimg SHA-256 MISMATCH' && exit 1; \
      fi && \
      echo \"mkbootimg SHA-256 verified: \$ACTUAL_MKBOOTIMG_SHA256\" && \
      cd linux-$KERNEL_VERSION && \
      make ARCH=arm64 zethra_defconfig && \
      make ARCH=arm64 CROSS_COMPILE=\"ccache aarch64-linux-gnu-\" KBUILD_BUILD_USER=zethra KBUILD_BUILD_HOST=zethra-build KBUILD_BUILD_TIMESTAMP=\"Fri Jun 12 17:00:00 UTC 2026\" KBUILD_BUILD_VERSION=1 -j\$(nproc) Image.gz dtbs && \
      cp arch/arm64/boot/Image.gz /workspace/build/out/ && \
      cp arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dtb /workspace/build/out/ && \
      cp arch/arm64/boot/Image.gz /workspace/build/out/Image.gz-dtb && \
      cat arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dtb >> /workspace/build/out/Image.gz-dtb && \
      cd /workspace && \
      if [ -f 'build/out/initramfs.cpio.gz' ]; then \
        echo 'Packing boot.img inside Docker...' && \
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
