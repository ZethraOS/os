#!/usr/bin/env bash
# build_and_boot.sh — Automated Nokia 6.1 Plus build, flash, and boot test
# SPDX-License-Identifier: Apache-2.0
#
# This script:
#   1. Runs pre-flight reproducibility check
#   2. Builds kernel (with reproducibility manifest)
#   3. Builds initramfs (with ADB support)
#   4. Packs boot image
#   5. Flashes to device
#   6. Monitors early console output
#
# Prerequisites:
#   - Device connected via USB with Developer Options + OEM Unlocking enabled
#   - All build tools installed (gcc, aarch64-linux-gnu-gcc, rustc, cargo, etc.)
#
# Usage:
#   bash build/scripts/build_and_boot.sh [OPTIONS]
#
# Options:
#   --dry-run              Show what would run without executing
#   --skip-flash           Build but don't flash (test builds only)
#   --skip-check           Skip pre-flight reproducibility check
#   --verbose              Show detailed output from each build step
#   --no-monitor           Don't monitor serial console after flash
#   --timeout SECONDS      How long to wait for boot messages (default: 30)
#   --clean                Clean previous build artifacts first

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPTS_DIR="$REPO_ROOT/build/scripts"
OUT_DIR="$REPO_ROOT/build/out"

# Options
DRY_RUN=false
SKIP_FLASH=false
SKIP_CHECK=false
VERBOSE=false
NO_MONITOR=false
BOOT_TIMEOUT=30
CLEAN_FIRST=false

# Colours
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*"; }
fail()    { echo -e "${RED}✗${RESET}  $*"; exit 1; }
dryrun()  { echo -e "${YELLOW}[DRY-RUN]${RESET} $*"; }
section() { echo -e "\n${BOLD}${CYAN}$*${RESET}"; }

run() {
  if [[ "$DRY_RUN" == true ]]; then
    dryrun "$@"
  else
    "$@"
  fi
}

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)      DRY_RUN=true; shift ;;
    --skip-flash)   SKIP_FLASH=true; shift ;;
    --skip-check)   SKIP_CHECK=true; shift ;;
    --verbose)      VERBOSE=true; shift ;;
    --no-monitor)   NO_MONITOR=true; shift ;;
    --timeout)      BOOT_TIMEOUT="$2"; shift 2 ;;
    --clean)        CLEAN_FIRST=true; shift ;;
    *)              error "Unknown option: $1"; exit 1 ;;
  esac
done

# ─── Header ───────────────────────────────────────────────────────────────────
echo ""
echo "╔════════════════════════════════════════════════════════════╗"
echo "║   ZethraOS Nokia 6.1 Plus — Automated Build & Boot Test   ║"
echo "╚════════════════════════════════════════════════════════════╝"
echo ""
echo "Target:        Nokia 6.1 Plus (TA-1103) / SDM636"
echo "Date:          $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
echo "Repo:          $REPO_ROOT"
echo "Mode:          $([ "$DRY_RUN" = "true" ] && echo "DRY-RUN (no changes)" || echo "LIVE (will build and flash)")"
echo ""

# ─── Step 0: Optional Clean ────────────────────────────────────────────────────
if [[ "$CLEAN_FIRST" == true ]]; then
  section "Step 0: Cleaning Previous Build"
  info "Removing kernel source and build artifacts..."
  run rm -rf "$REPO_ROOT/linux-6.9" "$OUT_DIR"/*
  success "Clean complete"
  echo ""
fi

# ─── Step 1: Pre-Flight Check ─────────────────────────────────────────────────
if [[ "$SKIP_CHECK" != true ]]; then
  section "Step 1: Pre-Flight Reproducibility Check (30 sec)"
  
  info "Verifying build system is ready..."
  if ! run bash "$SCRIPTS_DIR/quick_reproducibility_check.sh"; then
    fail "Pre-flight check FAILED. Fix issues before proceeding."
  fi
  success "Pre-flight check PASSED ✓"
  echo ""
else
  warn "Skipping pre-flight check (--skip-check)"
  echo ""
fi

# ─── Step 2: Build Kernel ────────────────────────────────────────────────────
section "Step 2: Building Kernel (15-30 minutes)"

BUILD_LOG="$OUT_DIR/build_kernel.log"
mkdir -p "$OUT_DIR"

info "Starting kernel build..."
if [[ "$VERBOSE" == true ]]; then
  run bash "$SCRIPTS_DIR/build_kernel.sh" --pin-check
else
  if ! run bash "$SCRIPTS_DIR/build_kernel.sh" --pin-check > "$BUILD_LOG" 2>&1; then
    error "Kernel build FAILED"
    tail -50 "$BUILD_LOG" >&2
    fail "See full log: $BUILD_LOG"
  fi
  info "Build log: $BUILD_LOG"
fi

if [[ -f "$OUT_DIR/Image.gz-dtb" ]]; then
  success "Kernel built: $(du -h "$OUT_DIR/Image.gz-dtb" | awk '{print $1}')"
  if [[ -f "$OUT_DIR/.kernel-build-manifest.txt" ]]; then
    success "Reproducibility manifest created"
  fi
else
  fail "Kernel image not found at: $OUT_DIR/Image.gz-dtb"
fi
echo ""

# ─── Step 3: Build Initramfs ──────────────────────────────────────────────────
section "Step 3: Building Initramfs (5-10 minutes)"

INITRAMFS_LOG="$OUT_DIR/build_initramfs.log"

info "Starting initramfs build..."
if [[ "$VERBOSE" == true ]]; then
  run bash "$SCRIPTS_DIR/build_initramfs.sh"
else
  if ! run bash "$SCRIPTS_DIR/build_initramfs.sh" > "$INITRAMFS_LOG" 2>&1; then
    error "Initramfs build FAILED"
    tail -50 "$INITRAMFS_LOG" >&2
    fail "See full log: $INITRAMFS_LOG"
  fi
  info "Build log: $INITRAMFS_LOG"
fi

if [[ -f "$OUT_DIR/initramfs.cpio.gz" ]]; then
  success "Initramfs built: $(du -h "$OUT_DIR/initramfs.cpio.gz" | awk '{print $1}')"
else
  fail "Initramfs not found at: $OUT_DIR/initramfs.cpio.gz"
fi
echo ""

# ─── Step 4: Pack Boot Image ──────────────────────────────────────────────────
section "Step 4: Packing Boot Image (1-2 minutes)"

PACK_LOG="$OUT_DIR/pack_boot_image.log"

info "Creating boot.img..."
if [[ "$VERBOSE" == true ]]; then
  run bash "$SCRIPTS_DIR/pack_boot_image.sh"
else
  if ! run bash "$SCRIPTS_DIR/pack_boot_image.sh" > "$PACK_LOG" 2>&1; then
    error "Boot image packing FAILED"
    tail -50 "$PACK_LOG" >&2
    fail "See full log: $PACK_LOG"
  fi
  info "Build log: $PACK_LOG"
fi

if [[ -f "$OUT_DIR/boot.img" ]]; then
  success "Boot image packed: $(du -h "$OUT_DIR/boot.img" | awk '{print $1}')"
  if [[ -f "$OUT_DIR/.boot-pack-manifest.txt" ]]; then
    success "Boot manifest created"
  fi
else
  fail "Boot image not found at: $OUT_DIR/boot.img"
fi
echo ""

# ─── Step 5: Verify Device ───────────────────────────────────────────────────
section "Step 5: Verifying Device Connection"

if [[ "$SKIP_FLASH" != true ]]; then
  info "Checking for connected device..."
  
  # Check adb devices
  if ! adb devices | grep -q "device$"; then
    fail "No device found in adb. Is USB debugging enabled?"
  fi
  success "Device detected via ADB"
  
  # Check fastboot
  info "Checking fastboot availability..."
  if ! command -v fastboot &>/dev/null; then
    fail "fastboot not found. Install: brew install android-platform-tools"
  fi
  success "fastboot available"
else
  warn "Skipping device verification (--skip-flash)"
fi
echo ""

# ─── Step 6: Flash to Device ──────────────────────────────────────────────────
if [[ "$SKIP_FLASH" != true ]]; then
  section "Step 6: Flashing Boot Image to Device"
  
  info "Rebooting device to fastboot mode..."
  run adb reboot bootloader
  
  info "Waiting for bootloader..."
  sleep 5
  
  if ! fastboot devices | grep -q ".*fastboot"; then
    warn "Device not in fastboot. Retrying..."
    sleep 5
  fi
  
  if ! fastboot devices | grep -q ".*fastboot"; then
    fail "Device not in fastboot mode. Try manual: Power + Volume Down for 10 sec"
  fi
  success "Device in fastboot mode"
  
  info "Flashing boot partition..."
  FLASH_LOG="$OUT_DIR/flash.log"
  if ! run fastboot flash boot "$OUT_DIR/boot.img" > "$FLASH_LOG" 2>&1; then
    error "Flash FAILED"
    cat "$FLASH_LOG" >&2
    fail "See full log: $FLASH_LOG"
  fi
  success "Boot partition flashed"
  
  info "Rebooting device..."
  run fastboot reboot
  success "Device rebooting..."
  
  info "Waiting for device to boot..."
  sleep 5
  echo ""
else
  warn "Skipping device flash (--skip-flash)"
  echo ""
fi

# ─── Step 7: Monitor Early Console ────────────────────────────────────────────
if [[ "$NO_MONITOR" != true ]] && [[ "$SKIP_FLASH" != true ]]; then
  section "Step 7: Monitoring Early Console Output"
  
  info "Collecting kernel boot logs via ADB..."
  info "Waiting up to $BOOT_TIMEOUT seconds for device ADB availability..."
  
  MONITOR_LOG="$OUT_DIR/boot_output.log"
  
  # Wait for device
  WAITED=0
  while ! adb devices | grep -q "device$"; do
    if [[ $WAITED -gt $BOOT_TIMEOUT ]]; then
      warn "Device not responding to ADB after ${BOOT_TIMEOUT}s (might still be booting)"
      break
    fi
    sleep 1
    ((WAITED++)) || true
  done
  
  if adb devices | grep -q "device$"; then
    success "Device ADB available ✓"
    
    info "Capturing kernel boot output..."
    if run adb shell dmesg > "$MONITOR_LOG" 2>&1; then
      success "Boot logs captured: $(wc -l < "$MONITOR_LOG") lines"
      
      # Show first 30 lines (early kernel messages)
      echo ""
      info "Early kernel messages (first 30 lines):"
      echo "─────────────────────────────────────────"
      head -30 "$MONITOR_LOG" | sed 's/^/  /'
      echo "─────────────────────────────────────────"
      
      # Check for success indicators
      if grep -q "earlycon:" "$MONITOR_LOG"; then
        success "✓ Early console detected"
      fi
      if grep -q "zethrad\|PID 1" "$MONITOR_LOG"; then
        success "✓ PID 1 initialization detected"
      fi
      if grep -q "Kernel panic\|BUG:" "$MONITOR_LOG"; then
        warn "⚠️  Kernel panic detected in logs"
      fi
      
      info "Full boot log: $MONITOR_LOG"
    else
      warn "Could not capture dmesg"
    fi
  else
    warn "Device not responding to ADB (may be in bootloader or hung)"
    info "Manual verification steps:"
    echo "  1. Check serial console at 115200 baud (if adapter connected)"
    echo "  2. Try: adb shell dmesg"
    echo "  3. Check: adb shell cat /proc/last_kmsg (after reboot)"
  fi
  echo ""
fi

# ─── Final Summary ────────────────────────────────────────────────────────────
section "Build & Boot Test Complete ✓"

echo ""
echo "Summary of generated artifacts:"
echo "  Build manifests:"
ls -lh "$OUT_DIR"/.*.txt 2>/dev/null | awk '{print "    " $9 " (" $5 ")"}' || echo "    (none)"
echo ""
echo "  Final artifacts:"
ls -lh "$OUT_DIR"/{Image.gz-dtb,initramfs.cpio.gz,boot.img} 2>/dev/null | awk '{print "    " $9 " (" $5 ")"}' || echo "    (none)"
echo ""

if [[ "$DRY_RUN" == true ]]; then
  echo "☐ DRY-RUN MODE: No actual changes were made"
else
  echo "✓ Build, flash, and boot test complete!"
fi

echo ""
if [[ "$SKIP_FLASH" == true ]]; then
  info "To flash to device:"
  echo "  fastboot devices  # Reboot device to bootloader"
  echo "  fastboot flash boot $OUT_DIR/boot.img"
  echo "  fastboot reboot"
fi

echo ""
info "Next steps:"
echo "  1. Check device UART serial console (115200 baud) for early kernel output"
echo "  2. Run: adb shell dmesg | head -50"
echo "  3. Check for panic/hang: adb shell cat /proc/last_kmsg"
echo ""
info "For troubleshooting: See docs/BUILD_TROUBLESHOOTING.md"
echo ""
