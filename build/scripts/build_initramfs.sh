#!/usr/bin/env bash
# build_initramfs.sh — Build ZethraOS initramfs for Nokia 6.1 Plus
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$REPO_ROOT/build/out"
STAGE_DIR="$REPO_ROOT/build/out/.stage_initramfs"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR"

# ─── Colors ───────────────────────────────────────────────────────────────────
GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; RESET='\033[0m'
info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }

echo "=================================================="
echo "    ZethraOS Initramfs Builder"
echo "=================================================="
mkdir -p "$OUT_DIR"

# ─── Step 1: Cross-compile Rust userspace ─────────────────────────────────────
info "Compiling Rust userspace..."

if [[ "$OSTYPE" == "darwin"* ]]; then
  # On macOS, build inside Docker to ensure correct cross-compiler alignment
  info "macOS detected — compiling inside Docker container..."
  
  # Ensure docker is running
  if ! docker ps &>/dev/null; then
    echo "Error: Docker is not running or not accessible."
    exit 1
  fi
  
  docker run --rm \
    -v "$REPO_ROOT:/workspace" \
    -w /workspace \
    rust:slim bash -c "
      apt-get update && \
      apt-get install -y gcc-aarch64-linux-gnu busybox-static && \
      rustup target add aarch64-unknown-linux-musl && \
      CC_aarch64_unknown_linux_musl=aarch64-linux-gnu-gcc cargo build --release --target aarch64-unknown-linux-musl && \
      mkdir -p /workspace/build/out && \
      cp /usr/bin/busybox /workspace/build/out/busybox
    "
else
  # On Linux, compile on host if tools exist, fallback to Docker
  if command -v cargo &>/dev/null && rustup target list | grep -q "aarch64-unknown-linux-musl (installed)"; then
    info "Host compilation tools found. Building on host..."
    CC_aarch64_unknown_linux_musl=aarch64-linux-gnu-gcc cargo build --release --target aarch64-unknown-linux-musl
    if command -v busybox &>/dev/null; then
      mkdir -p "$OUT_DIR"
      cp "$(which busybox)" "$OUT_DIR/busybox"
    else
      warn "busybox not found on host, fetching via Docker..."
      docker run --rm \
        -v "$REPO_ROOT:/workspace" \
        -w /workspace \
        rust:slim bash -c "
          apt-get update && \
          apt-get install -y busybox-static && \
          mkdir -p /workspace/build/out && \
          cp /usr/bin/busybox /workspace/build/out/busybox
        "
    fi
  else
    warn "Host tools missing or misconfigured. Falling back to Docker..."
    docker run --rm \
      -v "$REPO_ROOT:/workspace" \
      -w /workspace \
      rust:slim bash -c "
        apt-get update && \
        apt-get install -y gcc-aarch64-linux-gnu busybox-static && \
        rustup target add aarch64-unknown-linux-musl && \
        CC_aarch64_unknown_linux_musl=aarch64-linux-gnu-gcc cargo build --release --target aarch64-unknown-linux-musl && \
        mkdir -p /workspace/build/out && \
        cp /usr/bin/busybox /workspace/build/out/busybox
      "
  fi
fi

success "Rust userspace compiled successfully."

# ─── Step 2: Assemble staging directory ───────────────────────────────────────
info "Assembling initramfs structure..."

# Create core directory layout
mkdir -p "$STAGE_DIR"/{bin,sbin,etc/zethra/units,proc,sys,run/zethra,dev,tmp,usr/bin,usr/lib/zethra/init,mnt/persist}

# Copy busybox and create standard shell symlinks
if [[ -f "$OUT_DIR/busybox" ]]; then
  cp "$OUT_DIR/busybox" "$STAGE_DIR/bin/busybox"
  chmod +x "$STAGE_DIR/bin/busybox"
  # Create symlinks
  ln -sf busybox "$STAGE_DIR/bin/sh"
  ln -sf busybox "$STAGE_DIR/bin/ls"
  ln -sf busybox "$STAGE_DIR/bin/mkdir"
  ln -sf busybox "$STAGE_DIR/bin/cat"
  ln -sf busybox "$STAGE_DIR/bin/mount"
  ln -sf busybox "$STAGE_DIR/bin/mknod"
  ln -sf busybox "$STAGE_DIR/bin/echo"
  ln -sf busybox "$STAGE_DIR/bin/chmod"
  success "Added static busybox and created basic symlinks (/bin/sh, ls, mkdir, cat, mount, mknod, echo, chmod)"
else
  echo "Error: busybox binary not found at $OUT_DIR/busybox"
  exit 1
fi

# Copy zethrad init binary
ZETHRAD_BIN="$REPO_ROOT/target/aarch64-unknown-linux-musl/release/zethrad"
if [[ -f "$ZETHRAD_BIN" ]]; then
  cp "$ZETHRAD_BIN" "$STAGE_DIR/sbin/zethrad"
  success "Copied zethrad init to /sbin/zethrad"
else
  echo "Error: zethrad binary not found at $ZETHRAD_BIN"
  exit 1
fi

# Copy other system service binaries
for svc in zethra-networkd zethra-sensord zethra-otad zethra-telephonyd zethra-compositor zethra-sandbox zethra-ai-daemon; do
  SVC_BIN="$REPO_ROOT/target/aarch64-unknown-linux-musl/release/$svc"
  if [[ -f "$SVC_BIN" ]]; then
    # Copy to /usr/bin/
    cp "$SVC_BIN" "$STAGE_DIR/usr/bin/$svc"
    
    # Copy to /usr/lib/zethra/<clean_name>/ as expected by unit files
    clean_name="${svc#zethra-}"
    mkdir -p "$STAGE_DIR/usr/lib/zethra/$clean_name"
    cp "$SVC_BIN" "$STAGE_DIR/usr/lib/zethra/$clean_name/$svc"
    
    info "Added $svc to /usr/bin/ and /usr/lib/zethra/$clean_name/"
  fi
done

# ─── RCA FIX: Add ADB daemon for early kernel debugging ───────────────────────
info "Adding ADB support to initramfs (RCA improvement: adbd was missing)..."
mkdir -p "$STAGE_DIR/bin" "$STAGE_DIR/etc/adb"

# Try to find adbd binary or compile it
if command -v adbd &>/dev/null; then
  cp "$(which adbd)" "$STAGE_DIR/bin/adbd"
  success "Copied system adbd to initramfs"
elif [[ -f "$OUT_DIR/adbd" ]]; then
  cp "$OUT_DIR/adbd" "$STAGE_DIR/bin/adbd"
  success "Copied pre-built adbd to initramfs"
else
  warn "adbd not available (optional for now); ADB support may be limited without it"
fi

# Create ADB init script
cat > "$STAGE_DIR/etc/adb/adbd_init" << 'EOF'
#!/bin/sh
# Early ADB daemon startup — enables kernel debugging even before PID 1 fully boots
echo "[adbd_init] Starting ADB daemon for early kernel debugging..."
mkdir -p /run/adb
if [[ -f /bin/adbd ]]; then
  /bin/adbd &
  echo "[adbd_init] ADB daemon started (PID $!)"
else
  echo "[adbd_init] WARNING: adbd binary not found; USB/ADB unavailable"
fi
EOF
chmod +x "$STAGE_DIR/etc/adb/adbd_init"

# Copy system unit files
if [[ -d "$REPO_ROOT/build/configs/units" ]]; then
  cp "$REPO_ROOT/build/configs/units"/*.toml "$STAGE_DIR/etc/zethra/units/"
  success "Copied system unit configs"
fi

# Copy GPU firmware files
if [[ -d "$REPO_ROOT/kernel/firmware/qcom" ]]; then
  info "Packaging GPU firmware blobs..."
  mkdir -p "$STAGE_DIR/lib/firmware/qcom"
  cp "$REPO_ROOT"/kernel/firmware/qcom/a530* "$STAGE_DIR/lib/firmware/qcom/"
  cp "$REPO_ROOT"/kernel/firmware/qcom/a512* "$STAGE_DIR/lib/firmware/qcom/"
  # Run validation
  (cd "$STAGE_DIR/lib/firmware/qcom" && ls | grep -E "a530|a512" > /dev/null)
  success "GPU firmware blobs packaged successfully"
else
  warn "kernel/firmware/qcom not found, GPU acceleration may fail at boot"
fi

# Create base-setup helper script
cat > "$STAGE_DIR/usr/lib/zethra/init/zethra-base-setup" << 'EOF'
#!/bin/sh
echo "[zethra-base-setup] Initializing base system partitions..."
mkdir -p /run/zethra
echo "ZETHRAOS_BOOT_OK" > /run/zethra/boot_status
echo "[zethra-base-setup] Base system setup complete."
EOF
chmod +x "$STAGE_DIR/usr/lib/zethra/init/zethra-base-setup"

# Create root /init launcher script
cat > "$STAGE_DIR/init" << 'EOF'
#!/bin/sh

# Mount essential filesystems
mount -t proc proc /proc
mount -t sysfs sys /sys
mount -t devtmpfs dev /dev
mount -t tmpfs tmp /run
mount -t debugfs debug /sys/kernel/debug 2>/dev/null || true
mkdir -p /sys/kernel/config
mount -t configfs configfs /sys/kernel/config 2>/dev/null || true

echo ""
echo " ╔═══════════════════════════════╗"
echo " ║       ZethraOS v0.2.0        ║"
echo " ║   AI-Native Mobile OS        ║"
echo " ╚═══════════════════════════════╝"
echo ""

# Early kernel debugging support
echo "[init] Kernel boot initiated — starting early diagnostics..."
echo "[init] Kernel cmdline: $(cat /proc/cmdline)"
echo "[init] Kernel log (last 50 lines):"
dmesg | tail -50 2>/dev/null || true

# Save early dmesg to persist partition (diagnostic fallback for bootloops)
echo "[init] Waiting for storage devices to populate..."
sleep 2
mkdir -p /mnt/persist
if mount -t ext4 /dev/block/mmcblk0p73 /mnt/persist 2>/dev/null || \
   mount -t ext4 /dev/mmcblk0p73 /mnt/persist 2>/dev/null || \
   mount -t ext4 /dev/block/mmcblk1p73 /mnt/persist 2>/dev/null || \
   mount -t ext4 /dev/mmcblk1p73 /mnt/persist 2>/dev/null; then
  echo "[init] Mounted persist partition. Starting background boot log daemon..."
  (
    while true; do
      dmesg > /mnt/persist/zethra_boot.log 2>&1
      sync
      sleep 1
    done
  ) &
else
  echo "[init] WARNING: Could not mount persist partition to save boot log"
fi

# Configure CDC ACM USB serial gadget for emergency debugging shell
if [ -d /sys/kernel/config/usb_gadget ]; then
  GADGET=/sys/kernel/config/usb_gadget/g1
  mkdir -p "$GADGET"
  echo 0x18D1 > "$GADGET/idVendor"
  echo 0x0001 > "$GADGET/idProduct"
  mkdir -p "$GADGET/strings/0x409"
  echo "ZethraOS" > "$GADGET/strings/0x409/manufacturer"
  echo "Nokia 6.1 Plus" > "$GADGET/strings/0x409/product"
  echo "ZETHRA000001" > "$GADGET/strings/0x409/serialnumber"

  mkdir -p "$GADGET/functions/acm.usb0"
  mkdir -p "$GADGET/configs/c.1/strings/0x409"
  echo "CDC ACM Serial" > "$GADGET/configs/c.1/strings/0x409/configuration"
  ln -sf "$GADGET/functions/acm.usb0" "$GADGET/configs/c.1/acm.usb0" 2>/dev/null || true

  UDC=$(ls /sys/class/udc/ 2>/dev/null | head -1)
  if [ -n "$UDC" ]; then
    echo "$UDC" > "$GADGET/UDC" 2>/dev/null
  fi
fi

# Spawn root shell on the USB serial port in the background
while true; do
  if [ -c /dev/ttyGS0 ]; then
    /bin/sh </dev/ttyGS0 >/dev/ttyGS0 2>&1
  fi
  sleep 1
done &

echo "[init] Launching PID 1: zethrad..."
export ZETHRA_UNITS_DIR=/etc/zethra/units
if [ -d /mnt/persist ] && grep -q "/mnt/persist" /proc/mounts; then
  exec /sbin/zethrad >/mnt/persist/zethrad.log 2>&1
else
  exec /sbin/zethrad
fi
EOF
chmod +x "$STAGE_DIR/init"

# ─── Step 3: Package initramfs ────────────────────────────────────────────────
info "Packaging initramfs.cpio.gz..."

# Set modification time of all files to a fixed timestamp for reproducibility
find "$STAGE_DIR" -exec touch -h -t 202606121700.00 {} +

INITRAMFS_OUT="$OUT_DIR/initramfs.cpio.gz"

if [[ "$OSTYPE" == "darwin"* ]]; then
  info "macOS detected — packaging initramfs inside Docker container for GNU cpio..."
  docker run --rm \
    -v "$REPO_ROOT:/workspace" \
    -v "$STAGE_DIR:/staging" \
    -w /staging \
    ubuntu:24.04 bash -c "
      apt-get update && apt-get install -y cpio && \
      find . -not -name '*.cpio.gz' | sort | cpio --reproducible --owner=0:0 -oH newc 2>/dev/null | gzip -n > /workspace/build/out/initramfs.cpio.gz
    "
else
  # On Linux host, try to use GNU cpio --reproducible and --owner if available
  if cpio --help 2>&1 | grep -q "reproducible"; then
    (cd "$STAGE_DIR" && find . -not -name "*.cpio.gz" | sort | cpio --reproducible --owner=0:0 -oH newc 2>/dev/null | gzip -n > "$INITRAMFS_OUT")
  else
    (cd "$STAGE_DIR" && find . -not -name "*.cpio.gz" | sort | cpio -oH newc 2>/dev/null | gzip -n > "$INITRAMFS_OUT")
  fi
fi

# Cleanup
rm -rf "$STAGE_DIR"

success "initramfs.cpio.gz assembled at: $INITRAMFS_OUT ($(du -sh "$INITRAMFS_OUT" | cut -f1))"
echo "=================================================="
