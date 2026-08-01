#!/usr/bin/env bash
# =============================================================================
# ZethraOS Minimal Initramfs Builder
# Purpose: Build a tiny serial-debug initramfs (<2MB) for panel bring-up.
#          Includes ONLY: busybox (stripped), init script, qbootctl,
#          Qcom GPU firmware blobs (for DPU probe), and nothing else.
# Output:  build/out/initramfs-minimal.cpio.gz
# =============================================================================
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$REPO_ROOT/build/out"
WORK_DIR="$REPO_ROOT/build/out/minimal_rootfs"

info()    { echo "==> $*"; }
success() { echo "✓  $*"; }
warn()    { echo "⚠  $*"; }
err()     { echo "ERR $*" >&2; exit 1; }

# ── Sanity checks ─────────────────────────────────────────────────────────────
[[ -f "$OUT_DIR/busybox" ]]   || err "Missing $OUT_DIR/busybox — run full build first"
[[ -f "$OUT_DIR/qbootctl" ]]  || err "Missing $OUT_DIR/qbootctl — run qbootctl build first"

info "Building minimal initramfs at: $WORK_DIR"
rm -rf "$WORK_DIR"

# ── Directory skeleton (POSIX minimal) ─────────────────────────────────────────
for d in bin sbin dev proc sys sys/kernel/debug sys/kernel/config \
          tmp run mnt mnt/persist \
          lib/firmware/qcom \
          dev/disk/by-partlabel; do
  mkdir -p "$WORK_DIR/$d"
done

# ── Busybox: copy and create only the applets we need ─────────────────────────
info "Copying busybox..."
cp "$OUT_DIR/busybox" "$WORK_DIR/bin/busybox"
chmod 755 "$WORK_DIR/bin/busybox"

# Essential symlinks only — every other applet wastes nothing (busybox is one binary)
for applet in sh cat echo ls mount umount mkdir mknod sleep dmesg grep sed \
              awk cut wc ln sync find xargs sort; do
  ln -sf busybox "$WORK_DIR/bin/$applet"
done

# ── qbootctl ──────────────────────────────────────────────────────────────────
info "Copying qbootctl..."
cp "$OUT_DIR/qbootctl" "$WORK_DIR/sbin/qbootctl"
chmod 755 "$WORK_DIR/sbin/qbootctl"

# ── Qcom GPU firmware blobs (required for DPU/GPU probe on SDM636) ─────────────
info "Copying Qcom firmware blobs..."
FW_SRC="$REPO_ROOT/linux-7.1/drivers/firmware"  # try kernel tree first
INITRAMFS_FW_SRC="$(cd "$REPO_ROOT" && find . -name 'a512_zap.elf' 2>/dev/null | head -1 | xargs dirname 2>/dev/null || true)"

# Blobs from existing initramfs (already verified they exist there)
EXISTING_FW="/tmp/initramfs_inspect/rootfs/lib/firmware/qcom"
if [[ -d "$EXISTING_FW" ]]; then
  cp "$EXISTING_FW"/a512_zap.* "$WORK_DIR/lib/firmware/qcom/" 2>/dev/null || true
  cp "$EXISTING_FW"/a530_*.fw   "$WORK_DIR/lib/firmware/qcom/" 2>/dev/null || true
  success "Firmware blobs copied from extracted initramfs"
else
  warn "Firmware blobs not found in $EXISTING_FW — DPU probe may defer"
fi

# ── /dev nodes (minimal set — devtmpfs will populate more at runtime) ──────────
info "Creating static /dev nodes..."
# mknod requires root or CAP_MKNOD; we create them as device-less placeholders
# that get overlaid by devtmpfs at runtime.  Include console/null/kmsg/watchdog.
( cd "$WORK_DIR/dev"
  # These are created so symlinks from /dev/ttyMSM0 etc. work before devtmpfs
  mknod -m 600 console   c 5 1   2>/dev/null || true
  mknod -m 666 null      c 1 3   2>/dev/null || true
  mknod -m 600 ttyMSM0   c 252 0 2>/dev/null || true
  mknod -m 600 ttyGS0    c 243 0 2>/dev/null || true
  mknod -m 600 watchdog  c 10 130 2>/dev/null || true
  mknod -m 644 kmsg      c 1 11  2>/dev/null || true
)

# ── /init script ──────────────────────────────────────────────────────────────
info "Writing /init script..."
cat > "$WORK_DIR/init" << 'INIT_EOF'
#!/bin/sh
# =============================================================================
# ZethraOS Minimal Debug Init — Panel Bring-Up Edition
# Purpose: Boot to a live serial shell as fast as possible.
#          Mounts filesystems, feeds watchdog, sets up USB ACM serial,
#          drops to /bin/sh on ttyGS0, and marks slot boot-successful.
# NO userspace daemons. NO zethrad. NO compositor. Serial-only.
# =============================================================================

# ── Core mounts ───────────────────────────────────────────────────────────────
mount -t proc     proc    /proc
mount -t sysfs    sysfs   /sys
mount -t devtmpfs devtmpfs /dev
mount -t tmpfs    tmpfs   /tmp
mount -t tmpfs    tmpfs   /run
mount -t debugfs  none    /sys/kernel/debug  2>/dev/null || true
mkdir -p /sys/kernel/config
mount -t configfs none    /sys/kernel/config 2>/dev/null || true

echo "[minit] ZethraOS minimal debug initramfs booted"
echo "[minit] Kernel: $(uname -r) | cmdline: $(cat /proc/cmdline)"

# ── Hardware watchdog keeper ──────────────────────────────────────────────────
(while true; do
  echo a > /dev/watchdog 2>/dev/null || true
  sleep 2
done) &

# ── /dev/disk/by-partlabel symlinks (for qbootctl) ────────────────────────────
echo "[minit] Waiting for eMMC partitions..."
retries=0
while [ $retries -lt 150 ]; do
  [ -d /sys/block/mmcblk1 ] && break
  [ -d /sys/block/mmcblk0 ] && break
  sleep 0.05
  retries=$((retries+1))
done

mkdir -p /dev/disk/by-partlabel
for uevent_path in /sys/block/mmcblk*/mmcblk*p*/uevent; do
  [ -f "$uevent_path" ] || continue
  devname=$(grep "^DEVNAME=" "$uevent_path" 2>/dev/null | cut -d= -f2)
  partname=$(grep "^PARTNAME=" "$uevent_path" 2>/dev/null | cut -d= -f2)
  [ -n "$devname" ] && [ -n "$partname" ] && \
    ln -sf "/dev/$devname" "/dev/disk/by-partlabel/$partname" 2>/dev/null || true
done
echo "[minit] /dev/disk/by-partlabel: $(ls /dev/disk/by-partlabel/ 2>/dev/null | wc -l) entries"

# ── Mark current slot boot-successful ─────────────────────────────────────────
if [ -x /sbin/qbootctl ]; then
  echo "[minit] Marking boot-successful..."
  /sbin/qbootctl -m 2>&1 && echo "[minit] ✓ boot-successful written" || \
    echo "[minit] ⚠ qbootctl -m failed"
fi

# ── Save dmesg to persist partition ──────────────────────────────────────────
mkdir -p /mnt/persist
if mount -t ext4 /dev/disk/by-partlabel/persist /mnt/persist 2>/dev/null || \
   mount -t ext4 /dev/mmcblk0p73 /mnt/persist 2>/dev/null || \
   mount -t ext4 /dev/mmcblk1p73 /mnt/persist 2>/dev/null; then
  echo "[minit] persist mounted — continuous dmesg logging to /mnt/persist/zethra_boot.log"
  (while true; do dmesg > /mnt/persist/zethra_boot.log 2>&1; sync; sleep 1; done) &
else
  echo "[minit] ⚠ persist partition not mounted (dmesg not saved)"
fi

# ── USB CDC-ACM Serial Gadget ─────────────────────────────────────────────────
echo "[minit] Configuring USB CDC-ACM gadget..."
if [ -d /sys/kernel/config/usb_gadget ]; then
  GADGET=/sys/kernel/config/usb_gadget/g1
  mkdir -p "$GADGET/strings/0x409"
  mkdir -p "$GADGET/functions/acm.usb0"
  mkdir -p "$GADGET/configs/c.1/strings/0x409"
  echo 0x18D1          > "$GADGET/idVendor"
  echo 0x0001          > "$GADGET/idProduct"
  echo "ZethraOS"      > "$GADGET/strings/0x409/manufacturer"
  echo "Nokia 6.1 Plus"> "$GADGET/strings/0x409/product"
  echo "ZETHRA000001"  > "$GADGET/strings/0x409/serialnumber"
  echo "CDC ACM Debug" > "$GADGET/configs/c.1/strings/0x409/configuration"
  ln -sf "$GADGET/functions/acm.usb0" "$GADGET/configs/c.1/acm.usb0" 2>/dev/null || true

  # Wait up to 5s for UDC to appear
  udc_retries=0
  while [ -z "$(ls /sys/class/udc/ 2>/dev/null)" ] && [ $udc_retries -lt 100 ]; do
    sleep 0.05; udc_retries=$((udc_retries+1))
  done

  UDC=$(ls /sys/class/udc/ 2>/dev/null | head -1)
  if [ -n "$UDC" ]; then
    echo "$UDC" > "$GADGET/UDC" 2>/dev/null
    echo "[minit] USB ACM gadget bound to UDC: $UDC"
  else
    echo "[minit] ⚠ UDC not found — USB serial unavailable"
  fi
fi

# ── Drop into serial shell ────────────────────────────────────────────────────
echo "[minit] ✓ Init complete. Spawning shell on /dev/ttyGS0 and /dev/ttyMSM0"
echo "[minit] Run 'dmesg | grep -i drm' to inspect display driver probing"

# Shell on UART (hardware serial — always available)
(while true; do
  /bin/sh < /dev/ttyMSM0 > /dev/ttyMSM0 2>&1
  sleep 1
done) &

# Shell on USB ACM (enumerated ~3-5s after boot)
while true; do
  [ -c /dev/ttyGS0 ] && /bin/sh < /dev/ttyGS0 > /dev/ttyGS0 2>&1
  sleep 1
done
INIT_EOF

chmod 755 "$WORK_DIR/init"

# ── Print size breakdown ────────────────────────────────────────────────────────
info "Size breakdown of minimal rootfs:"
find "$WORK_DIR" -type f | sort | while read -r f; do
  sz=$(stat -f "%z" "$f" 2>/dev/null || stat -c "%s" "$f" 2>/dev/null || echo 0)
  printf "  %8d  %s\n" "$sz" "${f#$WORK_DIR/}"
done
TOTAL_BYTES=$(find "$WORK_DIR" -type f | xargs stat -f "%z" 2>/dev/null | paste -sd+ | bc 2>/dev/null || \
              find "$WORK_DIR" -type f | xargs stat -c "%s" 2>/dev/null | paste -sd+ | bc 2>/dev/null || echo "unknown")
echo ""
info "Total raw bytes: $TOTAL_BYTES"

# ── Pack the cpio archive ──────────────────────────────────────────────────────
info "Packing minimal initramfs..."
CPIO_OUT="$OUT_DIR/initramfs-minimal.cpio.gz"
(
  cd "$WORK_DIR"
  find . | sort | cpio -H newc -o 2>/dev/null | gzip -9 > "$CPIO_OUT"
)

CPIO_SIZE=$(ls -lh "$CPIO_OUT" | awk '{print $5}')
success "initramfs-minimal.cpio.gz: $CPIO_SIZE  →  $CPIO_OUT"

# ── Verify target size ─────────────────────────────────────────────────────────
CPIO_BYTES=$(stat -f "%z" "$CPIO_OUT" 2>/dev/null || stat -c "%s" "$CPIO_OUT")
if [[ "$CPIO_BYTES" -gt $((4 * 1024 * 1024)) ]]; then
  warn "initramfs is larger than 4MB — investigate!"
else
  success "initramfs is under 4MB — ✓"
fi
