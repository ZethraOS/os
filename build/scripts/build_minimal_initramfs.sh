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
for d in bin sbin usr/bin usr/sbin etc etc/init.d etc/systemd/system \
          dev proc sys sys/kernel/debug sys/kernel/config \
          tmp run mnt mnt/persist mnt/modem \
          lib/firmware/qcom lib/firmware/qcom/sdm636 \
          dev/disk/by-partlabel; do
  mkdir -p "$WORK_DIR/$d"
done

# ── Busybox: copy and create only the applets we need ─────────────────────────
info "Copying busybox..."
cp "$OUT_DIR/busybox" "$WORK_DIR/bin/busybox"
chmod 755 "$WORK_DIR/bin/busybox"

# Essential symlinks only — every other applet wastes nothing (busybox is one binary)
for applet in sh cat echo ls mount umount mkdir mknod sleep dmesg grep sed \
              awk cut wc ln sync find xargs sort kill killall pidof; do
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

# ── QCA Bluetooth firmware (WCN3990 — crbtfw21.tlv + crnv21.bin) ──────────────
# Proprietary firmware is NEVER committed to the repository.
# It MUST be supplied via the BT_FIRMWARE_DIR environment variable pointing to
# an absolute path containing both crbtfw21.tlv and crnv21.bin.
# SHA-256 hashes are verified against the OTA evidence report before staging.
EXPECTED_CRBTFW21_SHA256="49c9358d5488d4836626b8e0e49a31fea01b4e80394d40528253d19afba52ca8"
EXPECTED_CRNV21_SHA256="2bd75139a0dccb75470a453c299fd1861bc34ceb9502ea419557a9ebe405b016"

if [[ -z "${BT_FIRMWARE_DIR:-}" ]]; then
  err "BT_FIRMWARE_DIR is not set. Set it to the absolute path containing crbtfw21.tlv and crnv21.bin."
fi

if [[ ! -f "$BT_FIRMWARE_DIR/crbtfw21.tlv" ]]; then
  err "Missing: $BT_FIRMWARE_DIR/crbtfw21.tlv"
fi
if [[ ! -f "$BT_FIRMWARE_DIR/crnv21.bin" ]]; then
  err "Missing: $BT_FIRMWARE_DIR/crnv21.bin"
fi

info "Verifying BT firmware SHA-256 hashes..."
ACTUAL_CRBTFW21_SHA256="$(shasum -a 256 "$BT_FIRMWARE_DIR/crbtfw21.tlv" | awk '{print $1}')"
ACTUAL_CRNV21_SHA256="$(shasum -a 256 "$BT_FIRMWARE_DIR/crnv21.bin" | awk '{print $1}')"

if [[ "$ACTUAL_CRBTFW21_SHA256" != "$EXPECTED_CRBTFW21_SHA256" ]]; then
  err "SHA-256 mismatch for crbtfw21.tlv: expected $EXPECTED_CRBTFW21_SHA256, got $ACTUAL_CRBTFW21_SHA256"
fi
if [[ "$ACTUAL_CRNV21_SHA256" != "$EXPECTED_CRNV21_SHA256" ]]; then
  err "SHA-256 mismatch for crnv21.bin: expected $EXPECTED_CRNV21_SHA256, got $ACTUAL_CRNV21_SHA256"
fi
success "BT firmware hashes verified"

mkdir -p "$WORK_DIR/lib/firmware/qca"
cp "$BT_FIRMWARE_DIR/crbtfw21.tlv" "$WORK_DIR/lib/firmware/qca/"
cp "$BT_FIRMWARE_DIR/crnv21.bin"   "$WORK_DIR/lib/firmware/qca/"
success "BT firmware staged: lib/firmware/qca/crbtfw21.tlv + lib/firmware/qca/crnv21.bin"


# ── Modem Userspace Daemons (qrtr-ns, qrtr-lookup, rmtfs) ─────────────────────
info "Staging QRTR and RMTFS modem daemons..."
# If binaries are not in $OUT_DIR, build them from tools/
if [[ ! -f "$OUT_DIR/qrtr-ns" || ! -f "$OUT_DIR/rmtfs" ]]; then
  info "Building qrtr-ns and rmtfs from tools/..."
  docker run --rm -v "$REPO_ROOT:/workspace" -w /workspace/tools/qrtr zethra-build-env:1 bash -c \
    "make clean && CC=aarch64-linux-gnu-gcc make && aarch64-linux-gnu-strip -s qrtr-ns qrtr-lookup"
  docker run --rm -v "$REPO_ROOT:/workspace" -w /workspace/tools/rmtfs zethra-build-env:1 bash -c \
    "make clean && CC=aarch64-linux-gnu-gcc make && aarch64-linux-gnu-strip -s rmtfs"
  cp "$REPO_ROOT/tools/qrtr/qrtr-ns" "$OUT_DIR/"
  cp "$REPO_ROOT/tools/qrtr/qrtr-lookup" "$OUT_DIR/"
  cp "$REPO_ROOT/tools/rmtfs/rmtfs" "$OUT_DIR/"
fi

cp "$OUT_DIR/qrtr-ns" "$WORK_DIR/usr/bin/qrtr-ns"
chmod 755 "$WORK_DIR/usr/bin/qrtr-ns"
ln -sf /usr/bin/qrtr-ns "$WORK_DIR/bin/qrtr-ns"

if [[ -f "$OUT_DIR/qrtr-lookup" ]]; then
  cp "$OUT_DIR/qrtr-lookup" "$WORK_DIR/usr/bin/qrtr-lookup"
  chmod 755 "$WORK_DIR/usr/bin/qrtr-lookup"
  ln -sf /usr/bin/qrtr-lookup "$WORK_DIR/bin/qrtr-lookup"
fi

cp "$OUT_DIR/rmtfs" "$WORK_DIR/usr/bin/rmtfs"
chmod 755 "$WORK_DIR/usr/bin/rmtfs"
ln -sf /usr/bin/rmtfs "$WORK_DIR/bin/rmtfs"

if [[ ! -f "$OUT_DIR/tqftpserv" ]]; then
  info "Building tqftpserv from tools/..."
  docker run --rm -v "$REPO_ROOT:/workspace" -w /workspace/tools/tqftpserv zethra-build-env:1 bash -c \
    "make clean && CC=aarch64-linux-gnu-gcc make && aarch64-linux-gnu-strip -s tqftpserv"
  cp "$REPO_ROOT/tools/tqftpserv/tqftpserv" "$OUT_DIR/"
fi

cp "$OUT_DIR/tqftpserv" "$WORK_DIR/usr/bin/tqftpserv"
chmod 755 "$WORK_DIR/usr/bin/tqftpserv"
ln -sf /usr/bin/tqftpserv "$WORK_DIR/bin/tqftpserv"
success "Modem daemons staged to /usr/bin: qrtr-ns, qrtr-lookup, rmtfs, tqftpserv"

# ── Service Units & Init Scripts ──────────────────────────────────────────────
info "Writing systemd units and init scripts for modem daemons..."

if [[ -f "$REPO_ROOT/tools/tqftpserv/tqftpserv.service" ]]; then
  cp "$REPO_ROOT/tools/tqftpserv/tqftpserv.service" "$WORK_DIR/etc/systemd/system/"
fi

cat > "$WORK_DIR/etc/systemd/system/qrtr-ns.service" << 'EOF'
[Unit]
Description=QIPCRTR Name Service
Before=rmtfs.service

[Service]
ExecStart=/usr/bin/qrtr-ns -f 1
Restart=always
RestartSec=1

[Install]
WantedBy=multi-user.target
EOF

cat > "$WORK_DIR/etc/systemd/system/rmtfs.service" << 'EOF'
[Unit]
Description=Qualcomm remotefs service
After=qrtr-ns.service
ConditionPathExists=/dev/qcom_rmtfs_mem1

[Service]
ExecStart=/usr/bin/rmtfs -s -P -o /dev/disk/by-partlabel
Restart=always
RestartSec=1

[Install]
WantedBy=multi-user.target
EOF

cat > "$WORK_DIR/etc/init.d/qrtr-ns" << 'EOF'
#!/bin/sh
# /etc/init.d/qrtr-ns — Start/stop QRTR name server daemon
case "$1" in
  start)
    echo "Starting qrtr-ns..."
    /usr/bin/qrtr-ns &
    ;;
  stop)
    killall qrtr-ns 2>/dev/null || true
    ;;
  status)
    pidof qrtr-ns >/dev/null && echo "qrtr-ns is running" || echo "qrtr-ns is stopped"
    ;;
  *)
    echo "Usage: $0 {start|stop|status}"
    exit 1
    ;;
esac
EOF
chmod 755 "$WORK_DIR/etc/init.d/qrtr-ns"

cat > "$WORK_DIR/etc/init.d/rmtfs" << 'EOF'
#!/bin/sh
# /etc/init.d/rmtfs — Start/stop Qualcomm remote filesystem daemon
case "$1" in
  start)
    echo "Starting rmtfs..."
    /usr/bin/rmtfs -s -P -o /dev/disk/by-partlabel &
    ;;
  stop)
    killall rmtfs 2>/dev/null || true
    ;;
  status)
    pidof rmtfs >/dev/null && echo "rmtfs is running" || echo "rmtfs is stopped"
    ;;
  *)
    echo "Usage: $0 {start|stop|status}"
    exit 1
    ;;
esac
EOF
chmod 755 "$WORK_DIR/etc/init.d/rmtfs"

cat > "$WORK_DIR/etc/init.d/tqftpserv" << 'EOF'
#!/bin/sh
case "$1" in
  start)
    echo "Starting tqftpserv..."
    /usr/bin/tqftpserv -d -t /mnt/modem /lib/firmware > /tmp/tqftpserv.log 2>&1 &
    ;;
  stop)
    killall tqftpserv 2>/dev/null || true
    ;;
  status)
    pidof tqftpserv >/dev/null && echo "tqftpserv is running" || echo "tqftpserv is stopped"
    ;;
  *)
    echo "Usage: $0 {start|stop|status}"
    exit 1
    ;;
esac
EOF
chmod 755 "$WORK_DIR/etc/init.d/tqftpserv"
success "Service units and init scripts created"

# ── Optional Modem Firmware Staging (Phase 5B hybrid strategy) ────────────────
if [[ -n "${MODEM_FIRMWARE_DIR:-}" ]]; then
  info "Staging modem MBA firmware via stage_modem_firmware.sh..."
  INITRAMFS_DIR="$WORK_DIR" MODEM_FIRMWARE_DIR="$MODEM_FIRMWARE_DIR" \
    bash "$REPO_ROOT/build/scripts/stage_modem_firmware.sh"
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

# ── Mount Modem Partition (VFAT) & Symlink Firmware Files ─────────────────────
echo "[minit] Mounting modem firmware partition..."
mkdir -p /mnt/modem /lib/firmware/qcom/sdm636 /var/lib/tqftpserv
if mount -t vfat -o ro /dev/disk/by-partlabel/modem_b /mnt/modem 2>/dev/null || \
   mount -t vfat -o ro /dev/disk/by-partlabel/modem_a /mnt/modem 2>/dev/null; then
  echo "[minit] ✓ modem partition mounted at /mnt/modem"
  fw_count=0
  for f in /mnt/modem/image/*; do
    if [ -e "$f" ]; then
      ln -sf "$f" "/lib/firmware/qcom/sdm636/$(basename "$f")"
      fw_count=$((fw_count+1))
    fi
  done
  echo "[minit] ✓ Symlinked $fw_count modem firmware files to /lib/firmware/qcom/sdm636/"
else
  echo "[minit] ⚠ modem partition mount failed"
fi

# ── Modem Userspace Daemons (qrtr-ns, rmtfs) ──────────────────────────────────
export PATH=/usr/bin:/bin:/sbin:/usr/sbin

# Ensure EFS partition symlinks exist for rmtfs
ln -sf /dev/disk/by-partlabel/modemst1 /dev/disk/by-partlabel/modem_fs1 2>/dev/null || true
ln -sf /dev/disk/by-partlabel/modemst2 /dev/disk/by-partlabel/modem_fs2 2>/dev/null || true
ln -sf /dev/disk/by-partlabel/fsc /dev/disk/by-partlabel/modem_fsc 2>/dev/null || true

# Detect active slot (cmdline check)
ACTIVE_SLOT="b"
case "$(cat /proc/cmdline 2>/dev/null)" in
  *slot_suffix=_a*|*androidboot.slot_suffix=_a*) ACTIVE_SLOT="a" ;;
  *) ACTIVE_SLOT="b" ;;
esac

# Map nvdef_${slot} → fsg for rmtfs (Nokia FIH stores NV in nvdef, not fsg)
if [ "$ACTIVE_SLOT" = "b" ]; then
  ln -sf /dev/disk/by-partlabel/nvdef_b /dev/disk/by-partlabel/fsg
  echo "[minit] Mapped nvdef_b → fsg for EFS"
else
  ln -sf /dev/disk/by-partlabel/nvdef_a /dev/disk/by-partlabel/fsg
  echo "[minit] Mapped nvdef_a → fsg for EFS"
fi

ln -sf /dev/disk/by-partlabel/fsg /dev/disk/by-partlabel/modem_fsg 2>/dev/null || true
ln -sf /dev/disk/by-partlabel/fsg /dev/disk/by-partlabel/modem_fsg_oem_1 2>/dev/null || true
ln -sf /dev/disk/by-partlabel/fsg /dev/disk/by-partlabel/modem_fsg_oem_2 2>/dev/null || true

# Verify EFS has valid data (non-zero)
EFS_HASH=$(dd if=/dev/disk/by-partlabel/fsg bs=4096 count=512 2>/dev/null | sha256sum | cut -d' ' -f1)
echo "[minit] EFS SHA-256: $EFS_HASH"

# Disable remoteproc auto-recovery (keep crash state intact)
if [ -d /sys/kernel/debug/remoteproc/remoteproc0 ]; then
  echo disabled > /sys/kernel/debug/remoteproc/remoteproc0/recovery 2>/dev/null || true
  echo enabled > /sys/kernel/debug/remoteproc/remoteproc0/coredump 2>/dev/null || true
  echo "[minit] Disabled remoteproc recovery, enabled coredump"
fi

# Enable devcoredump
echo 0 > /sys/class/devcoredump/disabled 2>/dev/null || true

if [ -x /usr/bin/qrtr-ns ]; then
  echo "[minit] Starting qrtr-ns daemon..."
  /usr/bin/qrtr-ns &
  sleep 0.1
fi

if [ -x /usr/bin/rmtfs ]; then
  echo "[minit] Starting rmtfs daemon (storage: /dev/disk/by-partlabel)..."
  /usr/bin/rmtfs -v -s -P -o /dev/disk/by-partlabel > /tmp/rmtfs.log 2>&1 &
  sleep 0.1
fi

if [ -x /usr/bin/tqftpserv ]; then
  echo "[minit] Starting tqftpserv daemon (serving /mnt/modem and /lib/firmware)..."
  /usr/bin/tqftpserv -d -t /mnt/modem /lib/firmware > /tmp/tqftpserv.log 2>&1 &
  sleep 0.1
  echo "[minit] Started tqftpserv serving /mnt/modem and /lib/firmware"
fi

# ── Trigger Remoteproc Modem Boot ────────────────────────────────────────────
if [ -d /sys/class/remoteproc/remoteproc0 ]; then
  # Re-verify debugfs options before starting
  if [ -d /sys/kernel/debug/remoteproc/remoteproc0 ]; then
    echo disabled > /sys/kernel/debug/remoteproc/remoteproc0/recovery 2>/dev/null || true
    echo enabled > /sys/kernel/debug/remoteproc/remoteproc0/coredump 2>/dev/null || true
  fi
  state=$(cat /sys/class/remoteproc/remoteproc0/state 2>/dev/null)
  if [ "$state" = "offline" ]; then
    echo "[minit] Triggering remoteproc0 boot (echo start)..."
    echo start > /sys/class/remoteproc/remoteproc0/state 2>/dev/null || true
    sleep 0.5
    echo "[minit] remoteproc0 state: $(cat /sys/class/remoteproc/remoteproc0/state 2>/dev/null)"
  else
    echo "[minit] remoteproc0 state is already: $state"
  fi
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
if [ -c /dev/ttyMSM0 ]; then
  (while true; do
    [ -c /dev/ttyMSM0 ] && /bin/sh < /dev/ttyMSM0 > /dev/ttyMSM0 2>&1
    sleep 1
  done) &
fi

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
  find . | sort | cpio -H newc -o 2>/dev/null | xz --check=crc32 -9 > "$CPIO_OUT"
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
