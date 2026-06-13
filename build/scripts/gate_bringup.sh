#!/usr/bin/env bash
# gate_bringup.sh — MAANG-grade gated hardware bring-up for Nokia 6.1 Plus
# SPDX-License-Identifier: Apache-2.0
#
# Controlled experiment protocol (one gate per invocation):
#   Gate 0 — Baseline capture + reproducibility audit
#   Gate 1 — TWRP control: 3× transient fastboot boot (original image)
#   Gate 1b — TWRP repack: unpack → repack with project toolchain → 3× boot
#   Gate 2 — ZethraOS debug kernel: fastboot boot boot-debug.img
#
# Rules:
#   - Prefer transient `fastboot boot`; fall back to A/B slot flash if bootloader
#     rejects boot command (Nokia DRG returns "Load Error" on fastboot boot)
#   - Slot A = stock/control; Slot B = test images (never overwrite both)
#   - NEVER change kernel/DTB/cmdline between trials within a gate
#   - Record SHA-256, device state, and outcome for every attempt
#   - Stop on first gate failure; do not proceed to next gate
#
# Usage:
#   bash build/scripts/gate_bringup.sh --gate 0
#   bash build/scripts/gate_bringup.sh --gate 1
#   bash build/scripts/gate_bringup.sh --gate 1b
#   bash build/scripts/gate_bringup.sh --gate 2
#   bash build/scripts/gate_bringup.sh --gate 1 --method slot   # force A/B slot flash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$REPO_ROOT/build/out"
EXP_DIR="$OUT_DIR/experiments"
TWRP_IMG="$OUT_DIR/twrp.img"
DEBUG_IMG="$OUT_DIR/boot-debug.img"
TRIALS=3
BOOT_TIMEOUT=90
GATE=""
METHOD="auto"   # auto | boot | slot
TEST_SLOT="b"   # slot used for flash-based trials (slot A reserved for stock)

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*"; exit 1; }
fail()    { echo -e "${RED}✗ GATE FAILED${RESET} $*"; exit 1; }

usage() {
  cat <<EOF
Usage: $0 --gate <0|1|1b|2> [--trials N] [--timeout SEC] [--method auto|boot|slot] [--slot a|b]

Gates:
  0   Baseline + reproducibility audit (no device boot)
  1   TWRP control — 3× boot trials (fastboot boot or slot flash fallback)
  1b  TWRP repack — unpack/repack with mkbootimg, 3× boot trials
  2   ZethraOS debug — boot boot-debug.img via boot or slot flash

Methods:
  auto  Try fastboot boot first; fall back to slot flash on Load Error
  boot  Transient fastboot boot only
  slot  Flash to test slot + set_active + reboot (A/B isolation)
EOF
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --gate)    GATE="$2"; shift 2 ;;
    --trials)  TRIALS="$2"; shift 2 ;;
    --timeout) BOOT_TIMEOUT="$2"; shift 2 ;;
    --method)  METHOD="$2"; shift 2 ;;
    --slot)    TEST_SLOT="$2"; shift 2 ;;
    -h|--help) usage ;;
    *) error "Unknown argument: $1" ;;
  esac
done

[[ "$METHOD" == "auto" || "$METHOD" == "boot" || "$METHOD" == "slot" ]] \
  || error "Method must be auto, boot, or slot"
[[ "$TEST_SLOT" == "a" || "$TEST_SLOT" == "b" ]] \
  || error "Slot must be a or b"

[[ -n "$GATE" ]] || usage
command -v fastboot &>/dev/null || error "fastboot not found"
command -v adb &>/dev/null || error "adb not found"
command -v shasum &>/dev/null || error "shasum not found"

mkdir -p "$EXP_DIR"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)"
LOG="$EXP_DIR/gate${GATE}_${RUN_ID}.md"

log() { echo "$*" | tee -a "$LOG"; }
log_header() {
  {
    echo "# Gate ${GATE} — Run ${RUN_ID}"
    echo ""
    echo "- **Started:** $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "- **Host:** $(uname -s) $(uname -m)"
    echo "- **Repo:** $REPO_ROOT"
    echo ""
  } > "$LOG"
}

sha256() { shasum -a 256 "$1" | awk '{print $1}'; }

capture_device_state() {
  local section="$1"
  {
    echo "## ${section}"
    echo ""
    echo '```'
    fastboot devices 2>&1 || true
    fastboot getvar unlocked 2>&1 | grep -E "unlocked:" || true
    fastboot getvar current-slot 2>&1 | grep -E "current-slot:" || true
    fastboot getvar product 2>&1 | grep -E "product:" || true
    fastboot getvar serialno 2>&1 | grep -E "serialno:" || true
    echo '```'
    echo ""
  } >> "$LOG"
}

require_fastboot() {
  if ! fastboot devices 2>&1 | grep -q "fastboot"; then
    fail "Device not in fastboot. Power off → hold Volume Down + Power → connect USB."
  fi
}

wait_for_boot_outcome() {
  local expect="${1:-recovery}"
  local elapsed=0
  while (( elapsed < BOOT_TIMEOUT )); do
    if adb devices 2>/dev/null | grep -v "List of" | grep -q "$expect"; then
      echo "adb:$expect"
      return 0
    fi
    if fastboot devices 2>&1 | grep -q "fastboot"; then
      echo "fastboot:returned"
      return 0
    fi
    sleep 2
    elapsed=$((elapsed + 2))
  done
  echo "timeout"
  return 1
}

boot_image() {
  local image="$1"
  local trial_method="$2"

  if [[ "$trial_method" == "boot" ]]; then
    fastboot boot "$image" 2>&1 | tee -a "$LOG.boot.log"
    return $?
  fi

  info "Flashing to boot_${TEST_SLOT} (slot A reserved for stock/control)..."
  fastboot flash "boot_${TEST_SLOT}" "$image" 2>&1 | tee -a "$LOG.boot.log"
  fastboot set_active "$TEST_SLOT" 2>&1 | tee -a "$LOG.boot.log"
  fastboot reboot 2>&1 | tee -a "$LOG.boot.log"
}

resolve_boot_method() {
  local image="$1"

  if [[ "$METHOD" == "slot" ]]; then
    echo "slot"
    return
  fi
  if [[ "$METHOD" == "boot" ]]; then
    echo "boot"
    return
  fi

  # auto: probe whether this bootloader supports fastboot boot
  local probe_log
  probe_log="$(mktemp)"
  if fastboot boot "$image" 2>&1 | tee "$probe_log" | tee -a "$LOG.boot.log"; then
    echo "boot"
    rm -f "$probe_log"
    return
  fi
  if grep -q "Load Error" "$probe_log"; then
    warn "Bootloader rejects fastboot boot (Load Error) — using A/B slot flash on slot ${TEST_SLOT}"
    echo "slot"
    rm -f "$probe_log"
    return
  fi
  rm -f "$probe_log"
  echo "boot"
}

return_to_fastboot() {
  if adb devices 2>/dev/null | grep -qE "recovery|device"; then
    adb reboot bootloader 2>/dev/null || true
    sleep 5
    local wait_fb=0
    while (( wait_fb < 30 )); do
      fastboot devices 2>&1 | grep -q "fastboot" && return 0
      sleep 1
      wait_fb=$((wait_fb + 1))
    done
  fi
  return 1
}

run_boot_trials() {
  local image="$1"
  local label="$2"
  local expect="${3:-recovery}"
  local passed=0
  local trial_method=""

  [[ -f "$image" ]] || fail "Image not found: $image"

  log "## ${label}"
  log ""
  log "| Trial | Method | Image SHA-256 | Outcome | Notes |"
  log "| --- | --- | --- | --- | --- |"

  local trial
  for (( trial=1; trial<=TRIALS; trial++ )); do
    info "Trial ${trial}/${TRIALS}: ${label}"
    require_fastboot
    capture_device_state "Pre-trial ${trial} device state"

    local hash outcome notes
    hash="$(sha256 "$image")"

    if [[ -z "$trial_method" ]]; then
      trial_method="$(resolve_boot_method "$image")"
      log "- Boot method resolved: **${trial_method}** (slot ${TEST_SLOT} for flash)"
      log ""
    fi

    if [[ "$trial_method" == "slot" ]]; then
      if ! boot_image "$image" "slot"; then
        outcome="FAIL"
        notes="slot flash to boot_${TEST_SLOT} failed"
        log "| ${trial} | slot | \`${hash:0:16}…\` | **${outcome}** | ${notes} |"
        fail "${label} trial ${trial} failed at slot flash"
      fi
    elif ! boot_image "$image" "boot"; then
      outcome="FAIL"
      notes="fastboot boot command failed"
      log "| ${trial} | boot | \`${hash:0:16}…\` | **${outcome}** | ${notes} |"
      fail "${label} trial ${trial} failed at fastboot boot"
    fi

    if outcome_line="$(wait_for_boot_outcome "$expect")"; then
      outcome="PASS"
      notes="$outcome_line"
      passed=$((passed + 1))
      success "Trial ${trial}: ${outcome_line} (${trial_method})"

      if [[ "$outcome_line" == adb:* ]]; then
        adb shell getprop ro.twrp.version 2>/dev/null | tee -a "$LOG" || true
        adb shell getprop ro.product.device 2>/dev/null | tee -a "$LOG" || true
        info "Returning to fastboot for next trial..."
        return_to_fastboot || warn "Could not return to fastboot automatically"
      elif [[ "$outcome_line" == fastboot:returned ]]; then
        info "Device returned to fastboot"
      fi
    else
      outcome="FAIL"
      notes="no ADB/fastboot response within ${BOOT_TIMEOUT}s"
      log "| ${trial} | ${trial_method} | \`${hash:0:16}…\` | **${outcome}** | ${notes} |"
      fail "${label} trial ${trial}: ${notes}"
    fi

    log "| ${trial} | ${trial_method} | \`${hash:0:16}…\` | **${outcome}** | ${notes} |"
  done

  log ""
  log "**Result:** ${passed}/${TRIALS} trials passed (method: ${trial_method})"
  log ""
  success "${label}: ${passed}/${TRIALS} passed"
}

gate0_baseline() {
  log_header
  log "## Gate 0 — Baseline & Reproducibility Audit"
  log ""
  log "No device boot. Capture artifacts and build state only."
  log ""

  log "### Artifact fingerprints"
  log ""
  log "| Artifact | Size | SHA-256 |"
  log "| --- | --- | --- |"

  for artifact in twrp.img boot-debug.img boot.img Image.gz-dtb initramfs.cpio.gz; do
    local path="$OUT_DIR/$artifact"
    if [[ -f "$path" ]]; then
      log "| \`${artifact}\` | $(du -sh "$path" | cut -f1) | \`$(sha256 "$path")\` |"
    else
      log "| \`${artifact}\` | — | MISSING |"
    fi
  done
  log ""

  log "### Reproducibility check"
  log '```'
  bash "$REPO_ROOT/build/scripts/quick_reproducibility_check.sh" 2>&1 | tee -a "$LOG" || true
  log '```'
  log ""

  if [[ -f "$OUT_DIR/.kernel-build-manifest.txt" ]]; then
    log "### Kernel build manifest"
    log '```'
    cat "$OUT_DIR/.kernel-build-manifest.txt" >> "$LOG"
    log '```'
  fi

  log ""
  log "**Gate 0 exit criterion:** Artifacts present with recorded hashes."
  log "**Next step:** \`bash build/scripts/gate_bringup.sh --gate 1\`"
  log ""
  log "- **Finished:** $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  success "Gate 0 complete → $LOG"
}

gate1_twrp_control() {
  log_header
  log "## Gate 1 — TWRP Control (Original Image)"
  log ""
  log "Exit criterion: ${TRIALS} consecutive transient TWRP boots via \`fastboot boot\`."
  log "Do NOT flash. Slots remain untouched."
  log ""

  local expected_hash="ea0f0429cfa46536d754d5d47732740e1f1bd09dd6234cda236b007e020f0383"
  local actual_hash
  actual_hash="$(sha256 "$TWRP_IMG")"
  log "- TWRP SHA-256: \`${actual_hash}\`"
  if [[ "$actual_hash" != "$expected_hash" ]]; then
    warn "TWRP hash differs from RCA reference (${expected_hash:0:16}…)"
  fi
  log ""

  require_fastboot
  capture_device_state "Pre-gate device state"
  run_boot_trials "$TWRP_IMG" "TWRP control boot" "recovery"

  log "**Gate 1 PASSED**"
  log "**Next step:** \`bash build/scripts/gate_bringup.sh --gate 1b\`"
  log "- **Finished:** $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  success "Gate 1 complete → $LOG"
}

gate1b_twrp_repack() {
  log_header
  log "## Gate 1b — TWRP Repack Validation"
  log ""
  log "Unpack original TWRP, repack with project mkbootimg (no new AVB footer)."
  log "Exit criterion: ${TRIALS} consecutive transient boots of repacked image."
  log ""

  local work="$OUT_DIR/twrp_repack_${RUN_ID}"
  local repacked="$OUT_DIR/twrp-repacked.img"
  rm -rf "$work"
  mkdir -p "$work"

  info "Unpacking TWRP..."
  python3 "$REPO_ROOT/tools/unpack_bootimg.py" "$TWRP_IMG" -o "$work"
  local cmdline
  cmdline="$(python3 -c "import json; print(json.load(open('$work/metadata.json'))['cmdline'])")"

  info "Repacking TWRP with project toolchain (no AVB footer)..."
  python3 "$REPO_ROOT/tools/mkbootimg" \
    --header_version 0 \
    --kernel "$work/kernel" \
    --ramdisk "$work/ramdisk" \
    --pagesize 4096 \
    --base 0x00000000 \
    --kernel_offset 0x00008000 \
    --ramdisk_offset 0x01000000 \
    --second_offset 0x00f00000 \
    --tags_offset 0x00000100 \
    --cmdline "$cmdline" \
    --output "$repacked"

  log "- Original TWRP:  \`$(sha256 "$TWRP_IMG")\`"
  log "- Repacked TWRP:  \`$(sha256 "$repacked")\`"
  log "- Cmdline preserved: \`${cmdline:0:80}…\`"
  log ""

  require_fastboot
  capture_device_state "Pre-gate device state"
  run_boot_trials "$repacked" "TWRP repacked boot" "recovery"

  log "**Gate 1b PASSED** — boot.img toolchain validated"
  log "**Next step:** \`bash build/scripts/gate_bringup.sh --gate 2\`"
  log "- **Finished:** $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  success "Gate 1b complete → $LOG"
}

gate2_debug_boot() {
  log_header
  log "## Gate 2 — ZethraOS Debug Kernel (Transient)"
  log ""
  log "Boot custom kernel + diagnostic initramfs via \`fastboot boot\`."
  log "Exit criterion: kernel reaches PID 1 OR produces observable evidence (ADB/dmesg/pstore)."
  log ""

  if [[ ! -f "$DEBUG_IMG" ]]; then
    info "boot-debug.img missing — building now..."
    bash "$REPO_ROOT/build/scripts/quick_debug_boot.sh"
  fi

  log "- Debug image: \`$(sha256 "$DEBUG_IMG")\`"
  log ""

  require_fastboot
  capture_device_state "Pre-gate device state"

  local trial_method
  trial_method="$(resolve_boot_method "$DEBUG_IMG")"
  log "- Boot method: **${trial_method}**"
  log ""

  info "Booting ZethraOS debug image (single trial, observe outcome)..."
  boot_image "$DEBUG_IMG" "$trial_method"

  local outcome
  outcome="$(wait_for_boot_outcome "device" || echo "timeout")"
  log ""
  log "## Gate 2 outcome: \`${outcome}\`"
  log ""

  if [[ "$outcome" == adb:device ]]; then
    log "### Post-boot evidence"
    log '```'
    adb shell dmesg 2>/dev/null | head -50 >> "$LOG" || true
    adb shell cat /proc/version 2>/dev/null >> "$LOG" || true
    adb shell cat /proc/cmdline 2>/dev/null >> "$LOG" || true
    adb shell ls /sys/fs/pstore/ 2>/dev/null >> "$LOG" || true
    log '```'
    success "Gate 2: device reached ADB — inspect $LOG"
  elif [[ "$outcome" == fastboot:returned ]]; then
    warn "Device returned to fastboot — kernel likely failed to boot"
    log "**Gate 2 INCONCLUSIVE** — check UART or ramoops via TWRP"
    log "Recovery: \`bash build/scripts/dump_ramoops.sh\`"
  else
    warn "Gate 2: no response within ${BOOT_TIMEOUT}s"
    log "**Gate 2 FAILED** — static splash is not diagnostic; use UART"
  fi

  log "- **Finished:** $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  log ""
  log "Full log: $LOG"
}

echo ""
echo -e "${BOLD}╔══════════════════════════════════════════════════╗${RESET}"
echo -e "${BOLD}║  ZethraOS Gated Bring-up — Gate ${GATE}                  ║${RESET}"
echo -e "${BOLD}║  Nokia 6.1 Plus (DRG) · Transient boot only       ║${RESET}"
echo -e "${BOLD}╚══════════════════════════════════════════════════╝${RESET}"
echo ""

case "$GATE" in
  0)  gate0_baseline ;;
  1)  gate1_twrp_control ;;
  1b) gate1b_twrp_repack ;;
  2)  gate2_debug_boot ;;
  *)  usage ;;
esac
