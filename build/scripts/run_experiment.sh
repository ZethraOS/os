#!/usr/bin/env bash
# run_experiment.sh — Controlled experiment runner for the 6-image baseline matrix
# SPDX-License-Identifier: Apache-2.0
#
# Usage:
#   bash build/scripts/run_experiment.sh --image 01-headless
#   bash build/scripts/run_experiment.sh --image 05-drm-verbose --extra-cmdline "drm.debug=0x3f"
#   bash build/scripts/run_experiment.sh --image 06-fbdev-lockless --extra-cmdline "fb.lockless_register_fb=1"
#
# What this script does:
#   0. Verify initramfs exists (Step 0 prerequisite)
#   1. Snapshot slot state (F-11: pre-flash fastboot getvar all)
#   2. Build kernel with config fragment + cmdline (calls build_kernel.sh)
#   3. Flash vbmeta_b + boot_b to Slot B, set_active b (resets retry counter)
#   4. Reboot device
#   5. Run time_reboot_cycle.py to measure result (F-10: correct 120s message)
#   6. Attempt ACM liveness check after 120s timeout (F-12)
#   7. Write result back into the experiment JSON record
#   8. Append summary row to the human-readable ledger

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$REPO_ROOT/build/out"
EXP_DIR="$OUT_DIR/experiments"
FRAGMENTS_DIR="$REPO_ROOT/kernel/experiments"
LEDGER="$EXP_DIR/ledger.md"
SCRIPTS_DIR="$REPO_ROOT/build/scripts"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*"; exit 1; }

# ─── Argument parsing ─────────────────────────────────────────────────────────
IMAGE_NAME=""
EXTRA_CMDLINE=""
SLOT="b"
BOOT_TIMEOUT=120
DRY_RUN=false

usage() {
  cat <<EOF
Usage: $0 --image <name> [--extra-cmdline <str>] [--slot a|b] [--timeout <sec>] [--dry-run]

Available images (kernel/experiments/):
  01-headless       DRM=n, FBDEV=n — control baseline
  02-drm-nodisp     DRM=y, DSI=n, FBDEV=n — DRM core only
  03-drm-fbdev      DRM=y, DSI=y, FBDEV=y, FBCON=n — fbdev without fbcon
  04-drm-fbcon      Full stack — the known-crash configuration
  05-drm-verbose    Full stack + drm.debug=0x3f via --extra-cmdline
  06-fbdev-lockless Full stack + fb.lockless_register_fb=1 via --extra-cmdline

Examples:
  $0 --image 01-headless
  $0 --image 05-drm-verbose --extra-cmdline "drm.debug=0x3f"
  $0 --image 06-fbdev-lockless --extra-cmdline "fb.lockless_register_fb=1"
EOF
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --image)         IMAGE_NAME="$2"; shift 2 ;;
    --extra-cmdline) EXTRA_CMDLINE="$2"; shift 2 ;;
    --slot)          SLOT="$2"; shift 2 ;;
    --timeout)       BOOT_TIMEOUT="$2"; shift 2 ;;
    --dry-run)       DRY_RUN=true; shift ;;
    -h|--help)       usage ;;
    *) error "Unknown argument: $1. Use --help for usage." ;;
  esac
done

[[ -n "$IMAGE_NAME" ]] || { warn "No image specified."; usage; }
[[ "$SLOT" == "a" || "$SLOT" == "b" ]] || error "Slot must be 'a' or 'b'."

FRAGMENT="$FRAGMENTS_DIR/${IMAGE_NAME}.config.frag"
[[ -f "$FRAGMENT" ]] || error "Config fragment not found: $FRAGMENT"

mkdir -p "$EXP_DIR"

echo ""
echo -e "${BOLD}╔══════════════════════════════════════════════════════╗${RESET}"
echo -e "${BOLD}║   ZethraOS Controlled Experiment Runner              ║${RESET}"
echo -e "${BOLD}║   Image: ${IMAGE_NAME}$(printf '%*s' $((42 - ${#IMAGE_NAME})) '')║${RESET}"
echo -e "${BOLD}╚══════════════════════════════════════════════════════╝${RESET}"
echo ""

# ─── Step 0: Verify initramfs exists (must-fix F-22) ─────────────────────────
info "Step 0: Verifying initramfs..."
if [[ ! -f "$OUT_DIR/initramfs.cpio.gz" ]]; then
  error "initramfs.cpio.gz not found at $OUT_DIR/initramfs.cpio.gz
  Run Step 0 first: bash build/scripts/build_initramfs.sh"
fi
INITRAMFS_SHA256="$(sha256sum "$OUT_DIR/initramfs.cpio.gz" | awk '{print $1}')"
success "initramfs found: $INITRAMFS_SHA256"

# ─── Step 1: Pre-flash slot state snapshot (F-11) ────────────────────────────
info "Step 1: Capturing pre-flash device state..."
PREFLASH_LOG="$EXP_DIR/${IMAGE_NAME}_$(date -u +'%Y%m%dT%H%M%SZ')_preflash.txt"

if ! fastboot getvar all 2>&1 | tee "$PREFLASH_LOG"; then
  error "Device not in fastboot mode. Connect device and run: adb reboot bootloader"
fi

# Extract current slot and retry counts
CURRENT_SLOT="$(grep "current-slot" "$PREFLASH_LOG" | head -1 | grep -o '[ab]' || echo 'unknown')"
RETRY_B="$(grep "slot-retry-count:b" "$PREFLASH_LOG" | grep -o '[0-9]*$' || echo 'unknown')"
RETRY_A="$(grep "slot-retry-count:a" "$PREFLASH_LOG" | grep -o '[0-9]*$' || echo 'unknown')"

info "  Current slot: $CURRENT_SLOT | Slot A retries: $RETRY_A | Slot B retries: $RETRY_B"

# Safety gate: warn if target slot's retry count is critically low
if [[ "$SLOT" == "b" && "$RETRY_B" =~ ^[0-9]+$ && "$RETRY_B" -le 1 ]]; then
  warn "Slot B retry count is critically low ($RETRY_B)."
  warn "Slot B may become unbootable after this experiment."
  warn "To reset: fastboot set_active b (will be done automatically after flash)"
fi

success "Pre-flash state saved: $(basename "$PREFLASH_LOG")"

if [[ "$DRY_RUN" == "true" ]]; then
  warn "DRY RUN: Stopping before build/flash. Pre-flash state captured."
  exit 0
fi

# ─── Step 2: Build kernel with config fragment ────────────────────────────────
info "Step 2: Building kernel (EXPERIMENT_NAME=$IMAGE_NAME)..."
export EXPERIMENT_NAME="$IMAGE_NAME"
export CONFIG_FRAGMENT="$FRAGMENT"
export BOOT_EXTRA_CMDLINE="$EXTRA_CMDLINE"

bash "$SCRIPTS_DIR/build_kernel.sh"

# Capture the EXPERIMENT_ID that build_kernel.sh set
# (build_kernel.sh exports it; re-derive if not available in current shell)
GIT_HASH="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo 'unknown')"
BUILD_TIMESTAMP="$(ls -t "$EXP_DIR"/*.json 2>/dev/null | head -1 | xargs basename 2>/dev/null | sed 's/\.json//' || echo 'unknown')"
EXP_JSON="$(ls -t "$EXP_DIR"/img-${IMAGE_NAME}-*.json 2>/dev/null | head -1 || echo '')"
[[ -n "$EXP_JSON" ]] || error "Experiment JSON not found in $EXP_DIR. Build may have failed."

EXPERIMENT_ID="$(basename "$EXP_JSON" .json)"
info "Experiment ID: $EXPERIMENT_ID"

# Verify boot.img was produced
[[ -f "$OUT_DIR/boot.img" ]] || error "boot.img not found after build. Check build logs."
BOOT_SHA256="$(sha256sum "$OUT_DIR/boot.img" | awk '{print $1}')"
success "boot.img ready: $BOOT_SHA256"

# ─── Step 3: Flash to Slot B ─────────────────────────────────────────────────
info "Step 3: Flashing to slot $SLOT..."
FLASH_TIMESTAMP="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"

# Flash vbmeta first (disables AVB verification)
if [[ -f "$OUT_DIR/vbmeta.img" ]]; then
  fastboot flash "vbmeta_${SLOT}" "$OUT_DIR/vbmeta.img" 2>&1 | tee -a "$PREFLASH_LOG"
  success "vbmeta_${SLOT} flashed"
else
  warn "vbmeta.img not found — run pack_boot_image.sh or build_kernel.sh first"
fi

fastboot flash "boot_${SLOT}" "$OUT_DIR/boot.img" 2>&1 | tee -a "$PREFLASH_LOG"
success "boot_${SLOT} flashed"

# F-04 FIX: set_active resets slot-retry-count to default (7 on Qualcomm).
# This must be called after every flash, even if target slot is already active.
fastboot set_active "$SLOT" 2>&1 | tee -a "$PREFLASH_LOG"
success "Active slot set to ${SLOT} (retry counter reset)"

# ─── Step 4: Reboot into test image ──────────────────────────────────────────
info "Step 4: Rebooting device..."
fastboot reboot 2>&1 | tee -a "$PREFLASH_LOG"
REBOOT_TIME="$(date +%s)"
success "Device rebooting. Experiment clock started."

# ─── Step 5: Time the reboot cycle ───────────────────────────────────────────
info "Step 5: Measuring boot outcome (timeout: ${BOOT_TIMEOUT}s)..."
TIMING_JSON="$EXP_DIR/${EXPERIMENT_ID}_timing.json"

TIMING_RESULT="$(python3 "$REPO_ROOT/build/scripts/time_reboot_cycle.py" \
  --timeout "$BOOT_TIMEOUT" \
  --experiment-id "$EXPERIMENT_ID" \
  --output "$TIMING_JSON" 2>&1)"
echo "$TIMING_RESULT"

OUTCOME="unknown"
TIMING_SEC="null"
if echo "$TIMING_RESULT" | grep -q "TIMEOUT"; then
  OUTCOME="booted"
  TIMING_SEC="$BOOT_TIMEOUT+"
elif echo "$TIMING_RESULT" | grep -q "returned to fastboot"; then
  TIMING_SEC="$(echo "$TIMING_RESULT" | grep -oE '[0-9]+\.[0-9]+' | head -1 || echo 'null')"
  if python3 -c "exit(0 if float('${TIMING_SEC:-999}') < 20 else 1)" 2>/dev/null; then
    OUTCOME="crash-loop"
  else
    OUTCOME="unexpected-fastboot"
  fi
fi

# ─── Step 6: ACM liveness check (F-12) ───────────────────────────────────────
ACM_ALIVE="false"
if [[ "$OUTCOME" == "booted" ]]; then
  info "Step 6: Checking ACM console liveness..."
  # Give the USB ACM device a moment to enumerate
  sleep 3
  ACM_DEVICE="$(ls /dev/tty.usbmodem* 2>/dev/null | head -1 || ls /dev/ttyACM* 2>/dev/null | head -1 || echo '')"
  if [[ -n "$ACM_DEVICE" ]]; then
    success "ACM console enumerated: $ACM_DEVICE"
    ACM_ALIVE="true"
  else
    warn "ACM console did NOT enumerate after 120s timeout."
    warn "Device may be hung in a silent state (not a clean boot)."
    OUTCOME="hung-silent"
  fi
else
  info "Step 6: Skipping ACM check (device returned to fastboot)."
fi

# ─── Step 7: Update experiment JSON with results ─────────────────────────────
info "Step 7: Recording results..."
python3 - <<PYEOF
import json, sys
path = "$EXP_JSON"
try:
    with open(path) as f:
        rec = json.load(f)
    rec["result"]["flash_timestamp"] = "$FLASH_TIMESTAMP"
    rec["result"]["timing_seconds"]  = "$TIMING_SEC"
    rec["result"]["outcome"]         = "$OUTCOME"
    rec["result"]["acm_alive"]       = $([[ "$ACM_ALIVE" == "true" ]] && echo "True" || echo "False")
    rec["result"]["preflash_log"]    = "$(basename "$PREFLASH_LOG")"
    rec["result"]["slot_b_retries_before"] = "$RETRY_B"
    with open(path, "w") as f:
        json.dump(rec, f, indent=2)
    print(f"  Updated: {path}")
except Exception as e:
    print(f"  WARNING: Could not update JSON: {e}", file=sys.stderr)
PYEOF

# ─── Step 8: Append to human-readable ledger (F-18) ──────────────────────────
if [[ ! -f "$LEDGER" ]]; then
  cat > "$LEDGER" <<MDEOF
# ZethraOS Experiment Ledger
## Branch: fix/nokia-phase2-controlled-bringup
## Device: Nokia 6.1 Plus (TA-1103) / SDM636

| Experiment ID | Image | Outcome | Timing | ACM | Slot B Retries (pre) | Timestamp |
|---------------|-------|---------|--------|-----|----------------------|-----------|
MDEOF
fi

echo "| $EXPERIMENT_ID | $IMAGE_NAME | $OUTCOME | ${TIMING_SEC}s | $ACM_ALIVE | $RETRY_B | $FLASH_TIMESTAMP |" >> "$LEDGER"

# ─── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}╔══════════════════════════════════════════════════════╗${RESET}"
echo -e "${BOLD}║   Experiment Complete                                ║${RESET}"
echo -e "${BOLD}╚══════════════════════════════════════════════════════╝${RESET}"
echo "  ID:       $EXPERIMENT_ID"
echo "  Image:    $IMAGE_NAME"
echo "  Outcome:  $OUTCOME"
echo "  Timing:   ${TIMING_SEC}s"
echo "  ACM:      $ACM_ALIVE"
echo "  JSON:     $EXP_JSON"
echo "  Ledger:   $LEDGER"
echo ""

case "$OUTCOME" in
  booted)
    success "✅ PASSED — Device stayed up for ${BOOT_TIMEOUT}s+ and ACM responded."
    ;;
  crash-loop)
    warn "❌ CRASH LOOP — Device returned to fastboot in ${TIMING_SEC}s (< 20s threshold)."
    warn "   Check pstore on next boot: adb shell cat /sys/fs/pstore/console-ramoops-0"
    ;;
  hung-silent)
    warn "⚠️  HUNG — Device did not crash-loop but ACM console did not enumerate."
    warn "   Device may be suspended or USB not initialized."
    ;;
  unexpected-fastboot)
    warn "⚠️  UNEXPECTED — Device returned to fastboot after ${TIMING_SEC}s (> 20s, < 120s)."
    warn "   This may indicate a clean shutdown rather than a crash."
    ;;
esac
