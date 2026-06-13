#!/usr/bin/env bash
# quick_debug_boot.sh — Quickly repack boot.img with a minimal diagnostic init
# No kernel recompilation needed; just rebuilds initramfs + repacks boot.img
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$REPO_ROOT/build/out"
KERNEL_IMAGE="$OUT_DIR/Image.gz-dtb"

GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; RED='\033[0;31m'; RESET='\033[0m'
info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
die()     { echo -e "${RED}[FATAL]${RESET} $*" >&2; exit 1; }

echo "=================================================="
echo "    ZethraOS Quick Debug Boot Builder"
echo "=================================================="

# Verify kernel image exists
[[ -f "$KERNEL_IMAGE" ]] || die "Kernel image not found at $KERNEL_IMAGE. Run build_kernel.sh first."
[[ -f "$OUT_DIR/busybox" ]] || die "Static busybox not found at $OUT_DIR/busybox."

# Create minimal diagnostic initramfs
STAGE_DIR=$(mktemp -d)
trap "rm -rf $STAGE_DIR" EXIT

info "Building minimal diagnostic initramfs..."

# Create directory structure
mkdir -p "$STAGE_DIR"/{bin,sbin,dev,proc,sys,run,tmp,data}

# Copy static busybox with essential symlinks
cp "$OUT_DIR/busybox" "$STAGE_DIR/bin/busybox"
chmod +x "$STAGE_DIR/bin/busybox"

for cmd in sh ls cat mkdir mount umount mknod echo chmod dmesg sleep reboot poweroff grep head tail wc; do
  ln -sf busybox "$STAGE_DIR/bin/$cmd"
done

# Create a comprehensive diagnostic /init script
cat > "$STAGE_DIR/init" << 'INITEOF'
#!/bin/sh
#
# ZethraOS Diagnostic Init — Minimal boot for debugging
# This init mounts essential filesystems, dumps diagnostics,
# then attempts to bring up USB gadget for ADB access.
#

# Mount essential filesystems FIRST
mount -t proc proc /proc
mount -t sysfs sys /sys
mount -t devtmpfs dev /dev
mount -t tmpfs tmp /run
mount -t tmpfs tmp /tmp
mkdir -p /dev/pts
mount -t devpts devpts /dev/pts
mkdir -p /sys/fs/pstore
mount -t pstore pstore /sys/fs/pstore 2>/dev/null || true

# Redirect all output to console
exec > /dev/console 2>&1

echo "========================================"
echo " ZethraOS Diagnostic Boot v0.2.0-debug"
echo "========================================"
echo "[diag] Kernel booted! PID 1 (/init) is running."
echo "[diag] Kernel version: $(cat /proc/version)"
echo "[diag] Uptime: $(cat /proc/uptime)"
echo "[diag] Cmdline: $(cat /proc/cmdline)"
echo "[diag] Memory info:"
head -5 /proc/meminfo
echo "[diag] CPU info:"
head -10 /proc/cpuinfo
echo "[diag] Mounted filesystems:"
cat /proc/mounts
echo "[diag] /dev contents:"
ls /dev/ 2>/dev/null || echo "  (cannot list /dev)"
echo "[diag] Block devices:"
cat /proc/partitions 2>/dev/null || echo "  (no partitions)"
echo "[diag] Pstore files:"
ls -la /sys/fs/pstore/ 2>/dev/null || echo "  (no pstore)"

# Save dmesg to a known location
echo "[diag] Saving kernel dmesg to /tmp/boot_dmesg.log..."
dmesg > /tmp/boot_dmesg.log 2>/dev/null || echo "[diag] dmesg not available"

# Save dmesg and console-ramoops to the physical persist partition as a fallback for TWRP extraction
echo "[diag] Waiting for eMMC devices to populate..."
sleep 2
echo "[diag] Attempting to save dmesg and pstore to persist partition..."
mkdir -p /mnt/persist
if mount -t ext4 /dev/block/mmcblk0p73 /mnt/persist 2>/dev/null || \
   mount -t ext4 /dev/mmcblk0p73 /mnt/persist 2>/dev/null || \
   mount -t ext4 /dev/block/mmcblk1p73 /mnt/persist 2>/dev/null || \
   mount -t ext4 /dev/mmcblk1p73 /mnt/persist 2>/dev/null; then
  echo "[diag] Mounted persist partition! Saving boot log..."
  dmesg > /mnt/persist/zethra_boot.log 2>&1
  if [ -f /sys/fs/pstore/console-ramoops ]; then
    echo "[diag] Found console-ramoops! Saving to /persist/console-ramoops.log..."
    cat /sys/fs/pstore/console-ramoops > /mnt/persist/console-ramoops.log 2>&1
  else
    echo "[diag] No console-ramoops found in pstore"
  fi
  sync
  umount /mnt/persist
  echo "[diag] Boot log successfully written to /persist/zethra_boot.log"
else
  echo "[diag] WARNING: Could not mount persist partition"
fi

# Try to bring up USB gadget for ADB
echo "[diag] Attempting to bring up USB gadget for ADB..."

# Configure USB ConfigFS gadget (ADB function)
mkdir -p /sys/kernel/config
if mount -t configfs configfs /sys/kernel/config 2>/dev/null; then
  echo "[diag] ConfigFS mounted"
elif [ -d /sys/kernel/config/usb_gadget ]; then
  echo "[diag] ConfigFS already mounted"
else
  echo "[diag] WARNING: Cannot mount ConfigFS"
fi

# Try to configure CDC ACM USB serial gadget
if [ -d /sys/kernel/config/usb_gadget ]; then
  GADGET=/sys/kernel/config/usb_gadget/g1
  mkdir -p "$GADGET"
  echo 0x18D1 > "$GADGET/idVendor"    # Google
  echo 0x0001 > "$GADGET/idProduct"   # CDC ACM Serial (non-standard Google PID)
  mkdir -p "$GADGET/strings/0x409"
  echo "ZethraOS" > "$GADGET/strings/0x409/manufacturer"
  echo "Nokia 6.1 Plus (Debug)" > "$GADGET/strings/0x409/product"
  echo "ZETHRA000001" > "$GADGET/strings/0x409/serialnumber"

  # ACM function
  mkdir -p "$GADGET/functions/acm.usb0"
  mkdir -p "$GADGET/configs/c.1/strings/0x409"
  echo "CDC ACM Serial" > "$GADGET/configs/c.1/strings/0x409/configuration"
  ln -sf "$GADGET/functions/acm.usb0" "$GADGET/configs/c.1/acm.usb0" 2>/dev/null || true

  # Find the UDC (USB Device Controller)
  UDC=$(ls /sys/class/udc/ 2>/dev/null | head -1)
  if [ -n "$UDC" ]; then
    echo "[diag] Found UDC: $UDC"
    echo "$UDC" > "$GADGET/UDC" 2>/dev/null && echo "[diag] USB gadget activated!" || echo "[diag] USB gadget activation failed"
  else
    echo "[diag] WARNING: No UDC found!"
  fi
else
  echo "[diag] WARNING: USB gadget configfs not available"
fi

# Print targeted diagnostics to the screen for a photo
echo "========================================"
echo "    TARGETED DIAGNOSTICS FOR PHOTO"
echo "========================================"
echo "--- deferred (-517) probe lines ---"
dmesg | grep "\-517" || echo "no -517 lines"
echo "--- rpm/glink/mailbox/apcs lines ---"
dmesg | grep -i -E "rpm|glink|mailbox|apcs" || echo "no rpm/glink/mailbox lines"
echo "--- regulator lines ---"
dmesg | grep -i "regulator" || echo "no regulator lines"
echo "--- sdhci/sdhc/mmc lines ---"
dmesg | grep -i -E "sdhci|sdhc|mmc" || echo "no sdhci lines"
echo "--- dwc3/usb/irq lines ---"
dmesg | grep -i -E "dwc3|usb|irq" | tail -60 || echo "no usb/irq lines"
if [ -f /sys/fs/pstore/console-ramoops ]; then
  echo "--- console-ramoops (last 50 lines) ---"
  tail -50 /sys/fs/pstore/console-ramoops
fi
echo "========================================"

# Spawn root shell on the USB serial port in the background
echo "[diag] Spawning root shell on /dev/ttyGS0..."
while true; do
  if [ -c /dev/ttyGS0 ]; then
    /bin/sh </dev/ttyGS0 >/dev/ttyGS0 2>&1
  fi
  sleep 1
done &

echo "[diag] Done printing. Sleeping forever..."
while true; do
  sleep 60
done
INITEOF
chmod +x "$STAGE_DIR/init"

# Package the diagnostic initramfs
info "Packaging diagnostic initramfs..."
DEBUG_INITRAMFS="$OUT_DIR/initramfs-debug.cpio.gz"
(cd "$STAGE_DIR" && find . | cpio -oH newc 2>/dev/null | gzip -9 > "$DEBUG_INITRAMFS")
success "Diagnostic initramfs: $DEBUG_INITRAMFS ($(du -sh "$DEBUG_INITRAMFS" | cut -f1))"

# Repack boot.img with improved cmdline
info "Repacking boot.img with diagnostic initramfs and improved cmdline..."

# The improved cmdline adds critical parameters (earlycon removed to prevent clock-gating bus hangs)
CMDLINE="console=tty0 loglevel=8 ignore_loglevel androidboot.hardware=qcom androidboot.bootdevice=c0c4000.sdhci lpm_levels.sleep_disabled=1 cpuidle.off=1 buildvariant=eng printk.devkmsg=on clk_ignore_unused pd_ignore_unused nosmp initcall_debug earlyprintk panic=5 oops=panic no_console_suspend pstore.backend=ramoops ramoops.mem_address=0xacb00000 ramoops.mem_size=0x200000 ramoops.console_size=0x40000 ramoops.record_size=0x1000 ramoops.ftrace_size=0x1000 ramoops.pmsg_size=0x1000 ramoops.ecc=0 arm-smmu.disable_bypass=0 iommu.passthrough=1"


python3 "$REPO_ROOT/tools/mkbootimg" \
  --header_version 0 \
  --kernel         "$OUT_DIR/Image.gz-dtb" \
  --ramdisk        "$DEBUG_INITRAMFS" \
  --pagesize       4096 \
  --base           0x00000000 \
  --kernel_offset  0x00008000 \
  --ramdisk_offset 0x01000000 \
  --second_offset  0x00f00000 \
  --tags_offset    0x00000100 \
  --os_version     10.0.0 \
  --os_patch_level 2021-08 \
  --cmdline        "$CMDLINE" \
  --output         "$OUT_DIR/boot-debug.img"

python3 "$REPO_ROOT/tools/avbtool" add_hash_footer \
  --image          "$OUT_DIR/boot-debug.img" \
  --partition_name boot \
  --dynamic_partition_size \
  --algorithm      SHA256_RSA2048 \
  --key            "$REPO_ROOT/tools/test_key.pem"




success "Debug boot image built: $OUT_DIR/boot-debug.img"
echo "=================================================="
echo "To test: fastboot boot build/out/boot-debug.img"
echo "=================================================="
