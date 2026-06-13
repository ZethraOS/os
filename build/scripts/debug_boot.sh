#!/usr/bin/env bash
# debug_boot.sh — Automate booting custom kernel, waiting for user force-reboot, and extracting crash logs from TWRP
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BOOT_IMG="$REPO_ROOT/build/out/boot.img"
TWRP_IMG="$REPO_ROOT/build/out/twrp.img"
OUT_DIR="$REPO_ROOT/build/out"

# Colors
GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; RESET='\033[0m'
info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }

echo "=================================================="
echo "    ZethraOS Kernel Boot Debugger"
echo "=================================================="

# 1. Check if device is in TWRP/Recovery or bootloader
if adb devices | grep -q "recovery"; then
  info "Device detected in recovery. Rebooting to bootloader..."
  adb reboot bootloader
elif fastboot devices | grep -q "fastboot"; then
  info "Device detected in fastboot mode."
else
  warn "No device detected in recovery or fastboot. Please connect/reboot device."
  exit 1
fi

info "Waiting for bootloader mode..."
while ! fastboot devices | grep -q "fastboot"; do
  sleep 0.5
done
success "Device ready in bootloader."

# 2. Boot custom kernel
info "Booting custom kernel: $BOOT_IMG ..."
fastboot boot "$BOOT_IMG"
success "Boot command sent. Device is now starting the custom kernel."

# Wait for the device to disconnect from fastboot
info "Waiting for device to disconnect from fastboot..."
sleep 3
while fastboot devices | grep -q "fastboot"; do
  sleep 0.5
done
success "Device disconnected."

echo -e "\n${YELLOW}!!! ACTION REQUIRED !!!${RESET}"
echo "1. The phone will display the 'androidone' logo and hang."
echo "2. Wait 10-15 seconds for the kernel to attempt booting (and write ramoops if it crashes)."
echo "3. Hold Volume Down + Power buttons simultaneously until the screen goes black."
echo "4. Continue holding until it enters FASTBOOT mode again, then release."
echo "Waiting for phone to enter fastboot mode after crash..."

# 3. Wait for fastboot mode again
while ! fastboot devices | grep -q "fastboot"; do
  sleep 1
done
success "Phone returned to fastboot mode!"

# 4. Boot TWRP
info "Booting TWRP to collect crash logs..."
fastboot boot "$TWRP_IMG"

info "Waiting for TWRP ADB connection..."
while ! adb devices | grep -q "recovery"; do
  sleep 1
done
success "TWRP booted and ADB is active."

info "Waiting 5 seconds for systems to settle..."
sleep 5

# 5. Extract logs
info "Mounting pstore..."
adb shell "mount -t pstore pstore /sys/fs/pstore 2>/dev/null || true"

info "Listing pstore files:"
adb shell "ls -la /sys/fs/pstore || true"

rm -f "$OUT_DIR/console-ramoops.log" "$OUT_DIR/dmesg-ramoops-0.log"

if adb shell "[ -f /sys/fs/pstore/console-ramoops ]"; then
  adb pull /sys/fs/pstore/console-ramoops "$OUT_DIR/console-ramoops.log"
  success "Successfully dumped console-ramoops to: build/out/console-ramoops.log"
else
  warn "No console-ramoops found."
fi

if adb shell "[ -f /sys/fs/pstore/dmesg-ramoops-0 ]"; then
  adb pull /sys/fs/pstore/dmesg-ramoops-0 "$OUT_DIR/dmesg-ramoops-0.log"
  success "Successfully dumped dmesg-ramoops-0 to: build/out/dmesg-ramoops-0.log"
else
  warn "No dmesg-ramoops-0 found."
fi

adb shell "dmesg" > "$OUT_DIR/twrp_dmesg.log" 2>/dev/null || true
echo "=================================================="
