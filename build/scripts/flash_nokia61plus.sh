#!/usr/bin/env bash
# flash_nokia61plus.sh — Flash ZethraOS onto Nokia 6.1 Plus (TA-1103)
# SPDX-License-Identifier: Apache-2.0
#
# Target:  Nokia 6.1 Plus (TA-1103)
# SoC:     Qualcomm Snapdragon 636 (SDM636)
# CPU:     Kryo 260 — 4× Cortex-A73 + 4× Cortex-A53
# GPU:     Adreno 509
#
# Prerequisites:
#   1. Enable Developer Options → OEM Unlocking on the device
#   2. Enable Developer Options → USB Debugging
#   3. Install fastboot: brew install android-platform-tools  (macOS)
#   4. Build kernel:     bash build/scripts/build_kernel.sh
#   5. Build initramfs:  bash build/scripts/build_initramfs.sh
#
# Usage:
#   bash build/scripts/flash_nokia61plus.sh [--dry-run] [--slot a|b]
#
# WARNING: This will replace the boot partition. Your data partition is
#          NOT wiped by default. Use --wipe-data only if you intend a
#          factory reset.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BOOT_IMG="$REPO_ROOT/build/out/boot.img"
KERNEL_IMAGE="$REPO_ROOT/build/out/Image.gz"
DTB="$REPO_ROOT/build/out/sdm636-nokia-frt.dtb"
INITRAMFS="$REPO_ROOT/build/out/initramfs.cpio.gz"

# Defaults
DRY_RUN=false
SLOT="a"
WIPE_DATA=false

# ─── Colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*"; exit 1; }
dryrun()  { echo -e "${YELLOW}[DRY-RUN]${RESET} would run: $*"; }

run() {
  if [[ "$DRY_RUN" == "true" ]]; then
    dryrun "$@"
  else
    "$@"
  fi
}

# ─── Argument parsing ─────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)    DRY_RUN=true;   shift ;;
    --slot)       SLOT="$2";      shift 2 ;;
    --wipe-data)  WIPE_DATA=true; shift ;;
    *) error "Unknown argument: $1. Usage: $0 [--dry-run] [--slot a|b] [--wipe-data]" ;;
  esac
done

[[ "$SLOT" == "a" || "$SLOT" == "b" ]] || error "Slot must be 'a' or 'b', got: $SLOT"

# ─── Banner ───────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}╔══════════════════════════════════════════════╗${RESET}"
echo -e "${BOLD}║     ZethraOS — Nokia 6.1 Plus Flash Tool    ║${RESET}"
echo -e "${BOLD}║     Target: SDM636 (TA-1103) · Slot: $SLOT     ║${RESET}"
echo -e "${BOLD}╚══════════════════════════════════════════════╝${RESET}"
echo ""

[[ "$DRY_RUN" == "true" ]] && warn "DRY-RUN mode — no changes will be made to the device"
echo ""

# ─── Check prerequisites ──────────────────────────────────────────────────────
info "Checking prerequisites..."

command -v fastboot &>/dev/null || error "fastboot not found. Install with: brew install android-platform-tools"
command -v mkbootimg &>/dev/null || warn "mkbootimg not found — boot.img must be pre-built. Install: pip3 install mkbootimg"

# ─── Check build artefacts ───────────────────────────────────────────────────
info "Checking build artefacts..."

if [[ ! -f "$BOOT_IMG" ]]; then
  warn "boot.img not found at $BOOT_IMG — attempting to build it now..."

  [[ ! -f "$KERNEL_IMAGE" ]] && error "Kernel image not found at $KERNEL_IMAGE. Run: bash build/scripts/build_kernel.sh first"
  [[ ! -f "$DTB" ]]          && error "DTB not found at $DTB. Run: bash build/scripts/build_kernel.sh first"
  [[ ! -f "$INITRAMFS" ]]    && error "Initramfs not found at $INITRAMFS. Run: bash build/scripts/build_initramfs.sh first"

  info "Packing boot.img with mkbootimg..."
  # Nokia 6.1 Plus boot image parameters (from stock boot.img analysis)
  run mkbootimg \
    --kernel         "$KERNEL_IMAGE" \
    --ramdisk        "$INITRAMFS" \
    --dtb            "$DTB" \
    --pagesize       4096 \
    --base           0x80000000 \
    --kernel_offset  0x00008000 \
    --ramdisk_offset 0x01000000 \
    --second_offset  0x00f00000 \
    --tags_offset    0x00000100 \
    --dtb_offset     0x01f00000 \
    --cmdline        "console=ttyMSM0,115200n8 earlycon=msm_serial_dm,0xc170000 androidboot.hardware=qcom msm_rtb.filter=0x237 ehci-hcd.park=3 lpm_levels.sleep_disabled=1 androidboot.bootdevice=7824900.sdhci loop.max_part=7 buildvariant=eng" \
    --output         "$BOOT_IMG"

  success "boot.img packed → $BOOT_IMG ($(du -sh "$BOOT_IMG" | cut -f1))"
else
  success "boot.img found → $BOOT_IMG ($(du -sh "$BOOT_IMG" | cut -f1))"
fi

# ─── Detect device in fastboot mode ──────────────────────────────────────────
info "Waiting for device in fastboot mode..."
echo ""
echo -e "  ${YELLOW}Power off the Nokia 6.1 Plus, then hold:${RESET}"
echo -e "  ${BOLD}  Volume Down + Power${RESET}  until the fastboot screen appears."
echo -e "  Then connect via USB-C."
echo ""

if [[ "$DRY_RUN" == "false" ]]; then
  fastboot wait-for-device
  success "Device detected"
fi

# ─── Check bootloader unlock status ──────────────────────────────────────────
info "Checking bootloader unlock status..."

if [[ "$DRY_RUN" == "false" ]]; then
  UNLOCK_STATUS=$(fastboot getvar unlocked 2>&1 | grep "unlocked:" | awk '{print $2}')
  if [[ "$UNLOCK_STATUS" != "yes" ]]; then
    echo ""
    error "Bootloader is LOCKED. Unlock it first:
    1. On the device: Settings → About Phone → tap 'Build Number' 7 times
    2. Settings → Developer Options → Enable 'OEM Unlocking'
    3. Boot to fastboot: Power off → hold Volume Down + Power
    4. Run: fastboot flashing unlock
    5. Confirm on device screen (this WIPES all user data)
    6. Re-run this script"
  fi
  success "Bootloader is unlocked"
fi

# ─── Print current slot info ──────────────────────────────────────────────────
if [[ "$DRY_RUN" == "false" ]]; then
  CURRENT_SLOT=$(fastboot getvar current-slot 2>&1 | grep "current-slot:" | awk '{print $2}')
  info "Current active slot: $CURRENT_SLOT → flashing to slot: $SLOT"
fi

# ─── Flash boot partition ─────────────────────────────────────────────────────
info "Flashing boot.img to slot ${SLOT}..."
run fastboot flash "boot_${SLOT}" "$BOOT_IMG"
success "boot_${SLOT} flashed"

# ─── Set active slot ─────────────────────────────────────────────────────────
info "Setting active slot to ${SLOT}..."
run fastboot set_active "$SLOT"
success "Active slot set to ${SLOT}"

# ─── Optional data wipe ───────────────────────────────────────────────────────
if [[ "$WIPE_DATA" == "true" ]]; then
  warn "Wiping userdata partition as requested (--wipe-data)..."
  run fastboot erase userdata
  run fastboot erase cache 2>/dev/null || true
  success "Userdata wiped"
fi

# ─── Reboot ───────────────────────────────────────────────────────────────────
echo ""
info "Rebooting device..."
run fastboot reboot
echo ""
echo -e "${GREEN}${BOLD}✓ ZethraOS flash complete!${RESET}"
echo ""
echo -e "  Watch boot output via:"
echo -e "  ${CYAN}adb logcat -s zethrad${RESET}   (after device boots into ADB mode)"
echo -e "  ${CYAN}adb shell dmesg -w${RESET}       (kernel ring buffer live)"
echo ""

# ─── Rollback instructions ───────────────────────────────────────────────────
echo -e "${YELLOW}Rollback (if device won't boot):${RESET}"
echo -e "  Boot to fastboot → hold Volume Down + Power"
OTHER_SLOT="b"
[[ "$SLOT" == "b" ]] && OTHER_SLOT="a"
echo -e "  ${CYAN}fastboot set_active ${OTHER_SLOT}${RESET}   (switch back to previous slot)"
echo -e "  ${CYAN}fastboot reboot${RESET}"
echo ""
