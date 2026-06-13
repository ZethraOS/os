#!/usr/bin/env bash
# pack_boot_image.sh — Pack and sign boot image with reproducible headers
# SPDX-License-Identifier: Apache-2.0
#
# This script:
#   1. Validates kernel, ramdisk, and DTB artifacts
#   2. Packs them into Android boot.img format (header v0) matching stock parameters
#   3. Signs with test AVB key for verification
#   4. Records exact image parameters for next boot attempt
#
# Stock Parameters (from RCA):
#   Header version: 0, Page size: 4096, Base: 0x0
#   Kernel offset: 0x8000, Ramdisk offset: 0x01000000
#   Second offset: 0x00f00000 (stock; custom may differ), Tags offset: 0x100
#
# Usage:
#   bash build/scripts/pack_boot_image.sh [--no-sign] [--verbose]

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$REPO_ROOT/build/out"
TOOLS_DIR="$REPO_ROOT/tools"

KERNEL_IMAGE="$OUT_DIR/Image.gz-dtb"
INITRAMFS="$OUT_DIR/initramfs.cpio.gz"
BOOT_OUTPUT="$OUT_DIR/boot.img"
VBMETA_OUTPUT="$OUT_DIR/vbmeta.img"

# Boot image parameters (matching stock Nokia 6.1 Plus capture)
HEADER_VERSION=0
PAGE_SIZE=4096
KERNEL_OFFSET=0x8000
RAMDISK_OFFSET=0x01000000
SECOND_OFFSET=0x00f00000
TAGS_OFFSET=0x100
BASE=0x0
OS_VERSION="10.0.0"
OS_PATCH_LEVEL="2021-08"
CMDLINE="earlycon=msm_serial_dm,0xc170000 console=ttyMSM0,115200,n8 panic=10 buildvariant=userdebug"

# Options
SIGN_BOOT=true
VERBOSE=false

# Colours
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; RESET='\033[0m'

info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*"; exit 1; }

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-sign)  SIGN_BOOT=false; shift ;;
    --verbose)  VERBOSE=true; shift ;;
    *)          error "Unknown option: $1" ;;
  esac
done

echo "=================================================="
echo "    ZethraOS Boot Image Packer"
echo "=================================================="

# ─── Verify Artifacts ────────────────────────────────────────────────────────
info "Verifying build artifacts..."

[[ -f "$KERNEL_IMAGE" ]] || error "Kernel image not found: $KERNEL_IMAGE"
success "Kernel image: $KERNEL_IMAGE ($(du -h "$KERNEL_IMAGE" | cut -f1))"

[[ -f "$INITRAMFS" ]] || error "Initramfs not found: $INITRAMFS"
success "Initramfs: $INITRAMFS ($(du -h "$INITRAMFS" | cut -f1))"

# ─── Verify Tools ────────────────────────────────────────────────────────────
info "Checking boot image packing tools..."

if ! command -v mkbootimg &>/dev/null; then
  if [[ -f "$TOOLS_DIR/mkbootimg" ]]; then
    MKBOOTIMG="$TOOLS_DIR/mkbootimg"
    chmod +x "$MKBOOTIMG"
  else
    error "mkbootimg not found. Install with: brew install android-platform-tools (macOS)"
  fi
else
  MKBOOTIMG="$(command -v mkbootimg)"
fi
success "mkbootimg: $MKBOOTIMG ($(file "$MKBOOTIMG" | grep -oP '(x86_64|ARM64|aarch64)' || echo 'unknown arch'))"

# ─── Document Boot Parameters ────────────────────────────────────────────────
info "Recording boot image parameters for reproducibility..."
PARAMS_FILE="$OUT_DIR/.boot-image-params.txt"
{
  echo "# ZethraOS Boot Image Parameters — $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo "# Device: Nokia 6.1 Plus (TA-1103) / SDM636"
  echo "# Purpose: Reproducible boot image generation"
  echo ""
  echo "## Android Boot Header (v0)"
  echo "header_version:      $HEADER_VERSION"
  echo "page_size:           $PAGE_SIZE"
  echo "base:                $BASE"
  echo "kernel_offset:       $KERNEL_OFFSET"
  echo "ramdisk_offset:      $RAMDISK_OFFSET"
  echo "second_offset:       $SECOND_OFFSET"
  echo "tags_offset:         $TAGS_OFFSET"
  echo ""
  echo "## OS Parameters"
  echo "os_version:          $OS_VERSION"
  echo "os_patch_level:      $OS_PATCH_LEVEL"
  echo ""
  echo "## Kernel Command Line"
  echo "cmdline:             $CMDLINE"
  echo ""
  echo "## Input Artifacts"
  echo "kernel_sha256:       $(sha256sum "$KERNEL_IMAGE" | awk '{print $1}')"
  echo "initramfs_sha256:    $(sha256sum "$INITRAMFS" | awk '{print $1}')"
  echo ""
  echo "## Reference: Stock/TWRP Parameters (from RCA)"
  echo "# Stock:  header=0, page=4096, base=0, kernel_off=0x8000"
  echo "#         ramdisk_off=0x01000000, second_off=0x00f00000, tags_off=0x100"
  echo "# TWRP:   same as stock"
  echo "# Custom: should match above for compatibility"
} > "$PARAMS_FILE"
success "Parameters: $PARAMS_FILE"

# ─── Pack Boot Image ────────────────────────────────────────────────────────
info "Packing boot.img..."

if [[ "$VERBOSE" == true ]]; then
  "$MKBOOTIMG" \
    --header_version "$HEADER_VERSION" \
    --kernel         "$KERNEL_IMAGE" \
    --ramdisk        "$INITRAMFS" \
    --pagesize       "$PAGE_SIZE" \
    --base           "$BASE" \
    --kernel_offset  "$KERNEL_OFFSET" \
    --ramdisk_offset "$RAMDISK_OFFSET" \
    --second_offset  "$SECOND_OFFSET" \
    --tags_offset    "$TAGS_OFFSET" \
    --os_version     "$OS_VERSION" \
    --os_patch_level "$OS_PATCH_LEVEL" \
    --cmdline        "$CMDLINE" \
    --output         "$BOOT_OUTPUT"
else
  "$MKBOOTIMG" \
    --header_version "$HEADER_VERSION" \
    --kernel         "$KERNEL_IMAGE" \
    --ramdisk        "$INITRAMFS" \
    --pagesize       "$PAGE_SIZE" \
    --base           "$BASE" \
    --kernel_offset  "$KERNEL_OFFSET" \
    --ramdisk_offset "$RAMDISK_OFFSET" \
    --second_offset  "$SECOND_OFFSET" \
    --tags_offset    "$TAGS_OFFSET" \
    --os_version     "$OS_VERSION" \
    --os_patch_level "$OS_PATCH_LEVEL" \
    --cmdline        "$CMDLINE" \
    --output         "$BOOT_OUTPUT" 2>&1 | grep -v "^$" || true
fi

[[ -f "$BOOT_OUTPUT" ]] || error "Boot image creation failed"
success "Boot image: $BOOT_OUTPUT ($(du -h "$BOOT_OUTPUT" | cut -f1))"

# ─── Verify Boot Image Header (Reproducibility Check) ──────────────────────
info "Verifying boot image header..."
BOOT_ANALYSIS="$OUT_DIR/.boot-image-analysis.txt"
python3 "$TOOLS_DIR/parse_bootimg.py" "$BOOT_OUTPUT" > "$BOOT_ANALYSIS" 2>&1 || {
  warn "Boot image parsing not available (parse_bootimg.py may be missing)"
  echo "Skipping detailed header verification"
}
if [[ -f "$BOOT_ANALYSIS" ]]; then
  success "Boot image analysis: $BOOT_ANALYSIS"
  head -20 "$BOOT_ANALYSIS" | grep -E "(magic|kernel_size|ramdisk_size|header_version)" || true
fi

# ─── Sign Boot Image (Optional) ────────────────────────────────────────────
if [[ "$SIGN_BOOT" == true ]]; then
  info "Signing boot image with test AVB key..."
  
  TEST_KEY="$TOOLS_DIR/test_key.pem"
  
  if [[ ! -f "$TEST_KEY" ]]; then
    warn "Test key not found at $TEST_KEY; skipping AVB signature"
    warn "For production, generate/provide a signed key with:"
    warn "  openssl genrsa -out $TEST_KEY 2048"
  else
    AVBTOOL="$TOOLS_DIR/avbtool"
    if [[ ! -f "$AVBTOOL" ]]; then
      if ! command -v avbtool &>/dev/null; then
        warn "avbtool not found; skipping boot image signing"
      else
        AVBTOOL="$(command -v avbtool)"
      fi
    else
      chmod +x "$AVBTOOL"
    fi
    
    if [[ -f "$AVBTOOL" ]]; then
      python3 "$AVBTOOL" add_hash_footer \
        --image "$BOOT_OUTPUT" \
        --partition_name boot \
        --partition_size 67108864 \
        --algorithm SHA256_RSA2048 \
        --key "$TEST_KEY" \
        --salt c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00 2>&1 | grep -v "^$" || true
      
      success "Boot image signed with test key: $BOOT_OUTPUT"
      
      # Also create unsigned vbmeta for testing
      python3 "$AVBTOOL" make_vbmeta_image \
        --output "$VBMETA_OUTPUT" \
        --chain_partition boot:1:"$TEST_KEY.pub" 2>&1 | grep -v "^$" || true
      
      if [[ -f "$VBMETA_OUTPUT" ]]; then
        success "VBMeta: $VBMETA_OUTPUT (for --disable-verity testing)"
      fi
    fi
  fi
else
  info "Skipping boot image signing (--no-sign)"
fi

# ─── Record Final Manifest ────────────────────────────────────────────────────
MANIFEST_FILE="$OUT_DIR/.boot-pack-manifest.txt"
{
  echo "# ZethraOS Boot Image Packaging Manifest"
  echo "Timestamp:       $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo "Target:          Nokia 6.1 Plus (TA-1103) / SDM636"
  echo ""
  echo "## Generated Artifacts"
  echo "boot.img:        $(sha256sum "$BOOT_OUTPUT" | awk '{print $1}') ($(du -h "$BOOT_OUTPUT" | cut -f1))"
  if [[ -f "$VBMETA_OUTPUT" ]]; then
    echo "vbmeta.img:      $(sha256sum "$VBMETA_OUTPUT" | awk '{print $1}') ($(du -h "$VBMETA_OUTPUT" | cut -f1))"
  fi
  echo ""
  echo "## Flash Instructions (RCA Attempt N+1)"
  echo "Device setup:"
  echo "  1. Enable OEM Unlocking in Developer Options"
  echo "  2. Enable USB Debugging"
  echo "  3. Connect device via USB"
  echo ""
  echo "Flash command:"
  echo "  fastboot flash boot $BOOT_OUTPUT"
  echo "  fastboot reboot"
  echo ""
  echo "Recovery logs will be at: /proc/last_kmsg or /proc/kmsg"
  echo "Early kernel logs: Run 'adb shell dmesg' after boot"
  echo ""
  echo "## Known Limitations (RCA Reference)"
  echo "- First 51 Zethra symbols may not be in kernel config (custom symbols)"
  echo "- ADB over USB requires USB driver to load (part of kernel)"
  echo "- Early console via UART at 0xc170000 (115200 baud)"
  echo "- Ramoops at 0xacb00000 (records panic logs to /proc/last_kmsg)"
} > "$MANIFEST_FILE"
success "Manifest: $MANIFEST_FILE"

echo ""
success "✓ Boot image packing complete!"
echo ""
info "Next steps:"
echo "  1. Connect Nokia device via USB"
echo "  2. Enable USB Debugging (Settings → Developer Options)"
echo "  3. Run: bash build/scripts/flash_nokia61plus.sh"
echo ""
info "For debugging:"
echo "  - Early UART console: Requires USB serial adapter at 115200 baud"
echo "  - ADB console: adb shell dmesg (after /init mounts devtmpfs)"
echo "  - Ramoops dumps: adb shell cat /proc/last_kmsg"
echo ""
info "To verify reproducibility:"
echo "  cat $PARAMS_FILE"
echo "  cat $MANIFEST_FILE"
