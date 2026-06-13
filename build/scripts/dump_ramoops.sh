#!/usr/bin/env bash
# dump_ramoops.sh — Wait for fastboot, boot TWRP, and extract console-ramoops to debug early panics
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TWRP_IMG="$REPO_ROOT/build/out/twrp.img"
OUT_DIR="$REPO_ROOT/build/out"

# Colors
GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; RESET='\033[0m'
info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }

echo "=================================================="
echo "    ZethraOS Ramoops Debug Extractor"
echo "=================================================="
info "Waiting for device to enter fastboot mode..."

# Loop waiting for fastboot
while ! fastboot devices | grep -q "fastboot"; do
  sleep 0.5
done
success "Device detected in fastboot mode."

info "Booting TWRP recovery to access ramoops..."
fastboot boot "$TWRP_IMG"

info "Waiting for ADB connection..."
while ! adb devices | grep -q "recovery"; do
  sleep 1
done
success "ADB detected. Waiting 5s for filesystems..."
sleep 5

info "Checking /sys/fs/pstore..."
adb shell "mount -t pstore pstore /sys/fs/pstore 2>/dev/null || true"

info "Listing pstore contents:"
adb shell "ls -la /sys/fs/pstore || true"

# Extract files
info "Extracting ramoops..."
rm -f "$OUT_DIR/console-ramoops.log" "$OUT_DIR/dmesg-ramoops-0.log"

if adb shell "[ -f /sys/fs/pstore/console-ramoops ]"; then
  adb pull /sys/fs/pstore/console-ramoops "$OUT_DIR/console-ramoops.log"
  success "Dumped console-ramoops to: build/out/console-ramoops.log"
else
  warn "No console-ramoops found."
fi

if adb shell "[ -f /sys/fs/pstore/dmesg-ramoops-0 ]"; then
  adb pull /sys/fs/pstore/dmesg-ramoops-0 "$OUT_DIR/dmesg-ramoops-0.log"
  success "Dumped dmesg-ramoops-0 to: build/out/dmesg-ramoops-0.log"
else
  warn "No dmesg-ramoops-0 found."
fi

# Also dump standard dmesg of the recovery session just in case
adb shell "dmesg" > "$OUT_DIR/twrp_dmesg.log" 2>/dev/null || true

success "Logs successfully extracted!"
echo "=================================================="
