#!/usr/bin/env bash
# auto_flash_and_reboot.sh — Automatically detects fastboot device, flashes boot-debug.img, sets active Slot B, and reboots.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BOOT_IMG="$REPO_ROOT/build/out/boot-debug.img"

if [[ ! -f "$BOOT_IMG" ]]; then
  echo "[ERROR] Debug boot image not found at $BOOT_IMG"
  exit 1
fi

echo "=================================================="
echo "    Auto-Flash Boot Debug Image to Slot B"
echo "=================================================="
echo "Waiting for device to appear in fastboot mode..."

while true; do
  DEV=$(fastboot devices | awk '{print $1}')
  if [[ -n "$DEV" ]]; then
    echo "[INFO] Device detected: $DEV"
    break
  fi
  sleep 1
done

echo "[INFO] Flashing boot_b with $BOOT_IMG..."
fastboot flash boot_b "$BOOT_IMG"

echo "[INFO] Setting active slot to B..."
fastboot set_active b

echo "[INFO] Rebooting device..."
fastboot reboot

echo "[SUCCESS] Device flashed and rebooted!"
echo "Now wait for the device to boot and check for /dev/tty.usbmodem* on your host."
