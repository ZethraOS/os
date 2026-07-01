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

# ─── Step 2: Copy Defconfig & DTS ─────────────────────────────────────────────
info "Copying zethra_defconfig into kernel configs..."
cp "$REPO_ROOT/kernel/zethra_defconfig" "$KERNEL_DIR/arch/arm64/configs/zethra_defconfig"

if [[ -f "$REPO_ROOT/kernel/dts/sdm636-nokia-frt.dts" ]]; then
  info "Copying sdm636-nokia-frt.dts into kernel DTS directory..."
  cp "$REPO_ROOT/kernel/dts/sdm636-nokia-frt.dts" "$KERNEL_DIR/arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dts"
fi

# ─── REPRODUCIBILITY: Record Input Manifest ───────────────────────────────────
info "Recording build inputs for reproducibility verification..."
MANIFEST_FILE="$OUT_DIR/.kernel-build-manifest.txt"
{
  echo "# ZethraOS Kernel Build Manifest — $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo "# Attempt: $(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo 'unknown')"
  echo "# Device: Nokia 6.1 Plus (TA-1103) / SDM636"
  echo ""
  echo "## Build Inputs"
  echo "defconfig_sha256:  $(sha256sum "$REPO_ROOT/kernel/zethra_defconfig" | awk '{print $1}')"
  echo "dts_sha256:        $(sha256sum "$KERNEL_DIR/arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dts" 2>/dev/null | awk '{print $1}' || echo 'TBD (after build)')"
  echo ""
} > "$MANIFEST_FILE"
success "Manifest started: $MANIFEST_FILE"

# ─── Step 3: Compile Kernel ───────────────────────────────────────────────────
info "Compiling kernel..."

build_in_docker() {
  info "Compiling inside Docker container (zethra-build-env)..."
  
  # Ensure docker is running
  if ! docker ps &>/dev/null; then
    echo "Error: Docker is not running or not accessible."
    exit 1
  fi
  
  docker run --rm \
    -v "$REPO_ROOT:/workspace" \
    -w /workspace \
    -e BOOT_EXTRA_CMDLINE="${BOOT_EXTRA_CMDLINE:-}" \
    zethra-build-env bash -c "
      cd linux-$KERNEL_VERSION && \
      make ARCH=arm64 zethra_defconfig && \
      make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- KBUILD_BUILD_USER=zethra KBUILD_BUILD_HOST=zethra-build KBUILD_BUILD_TIMESTAMP=\"Fri Jun 12 17:00:00 UTC 2026\" KBUILD_BUILD_VERSION=1 -j\$(nproc) Image.gz dtbs && \
      cp arch/arm64/boot/Image.gz /workspace/build/out/ && \
      cp arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dtb /workspace/build/out/ && \
      cp arch/arm64/boot/Image.gz /workspace/build/out/Image.gz-dtb && \
      cat arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dtb >> /workspace/build/out/Image.gz-dtb && \
      cd /workspace && \
      if [ -f \"build/out/initramfs.cpio.gz\" ]; then \
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
          --cmdline        \"console=ttyMSM0,115200,n8 androidboot.hardware=qcom lpm_levels.sleep_disabled=1 loop.max_part=7 buildvariant=eng panic=10 msm.separate_gpu_kms=1 \$BOOT_EXTRA_CMDLINE\" \
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
        echo 'Warning: build/out/initramfs.cpio.gz not found, skipping boot.img packing'; \
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
        --cmdline        "console=ttyMSM0,115200,n8 androidboot.hardware=qcom lpm_levels.sleep_disabled=1 loop.max_part=7 buildvariant=eng panic=10 msm.separate_gpu_kms=1 ${BOOT_EXTRA_CMDLINE:-}" \
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

# ─── REPRODUCIBILITY: Record Build Outputs ────────────────────────────────────
info "Recording build outputs for reproducibility verification..."
{
  echo "## Build Outputs"
  echo "Image.gz-dtb_sha256:  $(sha256sum "$OUT_DIR/Image.gz-dtb" 2>/dev/null | awk '{print $1}' || echo 'TBD')"
  if [[ -f "$OUT_DIR/sdm636-nokia-frt.dtb" ]]; then
    echo "dtb_sha256:           $(sha256sum "$OUT_DIR/sdm636-nokia-frt.dtb" | awk '{print $1}')"
  fi
  if [[ -f "$OUT_DIR/boot.img" ]]; then
    echo "boot.img_sha256:      $(sha256sum "$OUT_DIR/boot.img" | awk '{print $1}')"
  fi
  echo ""
  echo "## Build Environment"
  echo "kernel_version:   $(grep -m 1 '^VERSION' "$KERNEL_DIR/Makefile" | awk '{print $NF}')"
  echo "compiler:         $(aarch64-linux-gnu-gcc --version 2>/dev/null | head -1 || echo 'cross-compiler-unknown')"
  echo "build_timestamp:  $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo ""
  echo "## Known Issues (RCA Reference)"
  echo "- 51 Zethra-specific kernel symbols may not exist in mainline Linux 6.9"
  echo "- Check build logs for CONFIG_ZETHRA_* warnings (expected)"
  echo "- Next phase: backport or modularize custom features"
} >> "$MANIFEST_FILE"

success "Build manifest updated: $MANIFEST_FILE"
echo ""
info "Kernel build complete!"
echo "Manifest: $MANIFEST_FILE"
echo "Artifacts:"
for f in "$OUT_DIR"/Image.gz-dtb "$OUT_DIR"/*.dtb "$OUT_DIR"/boot.img; do
  if [[ -f "$f" ]]; then
    ls -lh "$f" | awk '{print "  " $NF " (" $5 ")"}'
  fi
done

success "Kernel compilation complete."
echo "=================================================="
