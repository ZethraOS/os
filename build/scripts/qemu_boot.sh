#!/usr/bin/env bash
# qemu_boot.sh — Boot AetherOS in QEMU for local development and testing
# SPDX-License-Identifier: Apache-2.0
#
# Usage: bash build/scripts/qemu_boot.sh [--debug] [--no-kvm]
#
# Requirements:
#   apt-get install -y qemu-system-aarch64 gcc-aarch64-linux-gnu

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
KERNEL_IMAGE="$REPO_ROOT/build/out/Image.gz"
WORK_DIR=$(mktemp -d)
DEBUG=false
KVM=""
MEMORY="1G"
CPUS=4

while [[ $# -gt 0 ]]; do
  case "$1" in
    --debug)   DEBUG=true;      shift ;;
    --no-kvm)  KVM="-no-accel"; shift ;;
    --mem)     MEMORY="$2";     shift 2 ;;
    *) echo "Unknown: $1"; exit 1 ;;
  esac
done

echo "==> AetherOS QEMU Boot"
echo "    Kernel: $KERNEL_IMAGE"
echo "    Memory: $MEMORY, CPUs: $CPUS"

# ─── Build minimal initramfs with aetherd ─────────────────────────────────────
echo "--> Building initramfs..."
mkdir -p "$WORK_DIR"/{bin,sbin,etc/aether/units,proc,sys,run/aether,dev,tmp}

# Copy aetherd if built
AETHERD="$REPO_ROOT/target/aarch64-unknown-linux-gnu/release/aetherd"
if [[ -f "$AETHERD" ]]; then
  cp "$AETHERD" "$WORK_DIR/sbin/aetherd"
else
  echo "    [WARN] aetherd not built — using stub init"
  cat > "$WORK_DIR/sbin/aetherd" << 'STUB'
#!/bin/sh
echo "[aetherd-stub] AetherOS minimal init"
mount -t proc proc /proc 2>/dev/null || true
mount -t sysfs sysfs /sys 2>/dev/null || true
mount -t devtmpfs devtmpfs /dev 2>/dev/null || true
echo "[aetherd-stub] Filesystems mounted"
echo "AetherOS Boot: OK"
exec /bin/sh
STUB
  chmod +x "$WORK_DIR/sbin/aetherd"
fi

# Write a simple unit file for boot test
cat > "$WORK_DIR/etc/aether/units/boot-complete.unit.toml" << 'EOF'
name = "boot-complete"
description = "Boot completion marker"
after = []
exec_start = "/bin/sh -c 'echo AETHEROS_BOOT_OK > /run/aether/boot_status'"
restart = "never"
EOF

# Init script
cat > "$WORK_DIR/init" << 'INITEOF'
#!/bin/sh
echo ""
echo " ╔═══════════════════════════════╗"
echo " ║       AetherOS v0.1.0        ║"
echo " ║   AI-Native Mobile OS        ║"
echo " ╚═══════════════════════════════╝"
echo ""
exec /sbin/aetherd
INITEOF
chmod +x "$WORK_DIR/init"

# Pack initramfs
INITRAMFS="$WORK_DIR/initramfs.cpio.gz"
(cd "$WORK_DIR" && find . -not -name "*.cpio.gz" | cpio -oH newc 2>/dev/null | gzip > "$INITRAMFS")
echo "    Initramfs: $(du -sh "$INITRAMFS" | cut -f1)"

# ─── Check for kernel image ───────────────────────────────────────────────────
if [[ ! -f "$KERNEL_IMAGE" ]]; then
  echo ""
  echo "[WARN] Kernel image not found at $KERNEL_IMAGE"
  echo "       Run: bash build/scripts/build_kernel.sh first"
  echo ""
  echo "       Alternatively, download a pre-built ARM64 kernel for testing:"
  echo "       wget https://github.com/aetheros/prebuilts/releases/latest/download/Image.gz"
  echo ""
  echo "Exiting (no kernel)."
  rm -rf "$WORK_DIR"
  exit 1
fi

# ─── QEMU command ─────────────────────────────────────────────────────────────
APPEND="console=ttyAMA0 rdinit=/init loglevel=7 panic=5"
[[ "$DEBUG" == "true" ]] && APPEND="$APPEND kgdboc=ttyAMA0"

echo "--> Booting QEMU..."
echo ""

exec qemu-system-aarch64 \
  -M virt \
  -cpu cortex-a57 \
  -smp $CPUS \
  -m $MEMORY \
  -kernel "$KERNEL_IMAGE" \
  -initrd "$INITRAMFS" \
  -append "$APPEND" \
  -nographic \
  -serial mon:stdio \
  -no-reboot \
  -netdev user,id=net0 \
  -device virtio-net-device,netdev=net0 \
  -device virtio-rng-device \
  $KVM
