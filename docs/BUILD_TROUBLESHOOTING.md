# Build Troubleshooting Guide

**Document**: ZethraOS Nokia 6.1 Plus Build System  
**Last Updated**: 2026-06-11  
**Scope**: Kernel, initramfs, and boot image build failures

---

## Quick Diagnostic Flowchart

```
Build Failed?
    ├─ Kernel compilation error?
    │  └─ → See: Kernel Build Failures
    ├─ Initramfs/Rust compilation error?
    │  └─ → See: Initramfs Build Failures
    ├─ Boot image packing error?
    │  └─ → See: Boot Image Packing Failures
    ├─ Reproducibility mismatch?
    │  └─ → See: Reproducibility Issues
    └─ Device doesn't boot?
       └─ → See: Boot Failures (Device-Side)
```

---

## Kernel Build Failures

### Issue: `make: aarch64-linux-gnu-gcc: Command not found`

**Cause**: Cross-compiler not installed or not in PATH.

**Fix**:
```bash
# macOS
brew install arm64-elf-gcc  # or similar

# Ubuntu/Debian
sudo apt-get install gcc-aarch64-linux-gnu binutils-aarch64-linux-gnu

# Verify installation
aarch64-linux-gnu-gcc --version
```

**Detailed Check**:
```bash
# Confirm the exact cross-compiler path
which aarch64-linux-gnu-gcc

# If it exists but script doesn't find it:
export CROSS_COMPILE=aarch64-linux-gnu-
bash build/scripts/build_kernel.sh
```

---

### Issue: `make: *** No rule to make target 'zethra_defconfig'`

**Cause**: Defconfig file not copied to kernel source correctly.

**Fix**:
```bash
# Verify defconfig exists
ls -la kernel/zethra_defconfig

# Manually copy to kernel tree
cp kernel/zethra_defconfig linux-6.9/arch/arm64/configs/zethra_defconfig

# Verify it's there
ls -la linux-6.9/arch/arm64/configs/zethra_defconfig

# Retry make
cd linux-6.9
make ARCH=arm64 zethra_defconfig
```

---

### Issue: `error: failed to resolve 'CONFIG_SYMBOL'` or Missing Symbols

**Cause**: Defconfig requests symbols that don't exist in this kernel version.

**Fix**:
```bash
# Check which symbols are causing issues
cd linux-6.9
make ARCH=arm64 zethra_defconfig 2>&1 | grep -i "warning\|error" | head -20

# Generate the actual config (will skip unknown symbols)
make ARCH=arm64 -j$(nproc) Image.gz-dtb 2>&1 | tee build.log

# See what actually got enabled
grep "CONFIG_ZETHRA" linux-6.9/.config
```

**Known Issue** (Not a blocker):
- 51 Zethra-specific symbols don't exist in mainline Linux 6.9
- Build will warn but **continue**
- These are custom features for later integration
- Reference: `docs/NOKIA61PLUS_BOOT_RCA.md` → "51 requested symbols"

**Verify Build Succeeded Despite Warnings**:
```bash
ls -lh linux-6.9/arch/arm64/boot/Image.gz-dtb
# Should show: -rw-r--r-- ... Image.gz-dtb (size ~8MB)

file linux-6.9/arch/arm64/boot/Image.gz-dtb
# Should show: gzip compressed data, ...
```

---

### Issue: `error: ld.lld: error: section '.init.rodata' will not fit in region 'RODATA'`

**Cause**: Kernel too large for available memory or section mismatch.

**Fix**:
```bash
# Option 1: Clean and rebuild
cd linux-6.9
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- clean
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- zethra_defconfig
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- -j$(nproc) Image.gz-dtb

# Option 2: Reduce kernel bloat (remove debug symbols if not needed)
echo "# CONFIG_DEBUG_INFO is not set" >> linux-6.9/.config
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- oldconfig

# Option 3: Check final kernel size
du -h linux-6.9/arch/arm64/boot/Image.gz-dtb
# Typical: 5-9 MB (should fit in 64 MB boot partition)
```

---

### Issue: Build Hangs (Appears Frozen)

**Cause**: Compilation taking too long or actual hang.

**Fix**:
```bash
# Check if it's still compiling
ps aux | grep -i gcc

# If compiling (many gcc processes), wait. On slow machines:
# - First build: 20-40 minutes
# - Incremental: 5-10 minutes

# Monitor progress
watch -n 1 'ps aux | grep -i gcc | wc -l'  # Shows number of build jobs

# If truly hung (no activity for 10+ minutes):
# Kill it and retry with verbose output
killall make gcc 2>/dev/null || true

# Rebuild with output to see where it hangs
bash build/scripts/build_kernel.sh --verbose 2>&1 | tee kernel_build_verbose.log
tail -f kernel_build_verbose.log  # Watch in real-time
```

---

## Initramfs Build Failures

### Issue: `Docker: not running or not accessible` (macOS)

**Cause**: Docker daemon not running; builds require Docker on macOS for cross-compilation.

**Fix**:
```bash
# Start Docker
open -a Docker

# Wait for Docker to start (~30 seconds)
sleep 30

# Verify it's running
docker ps

# Retry build
bash build/scripts/build_initramfs.sh
```

---

### Issue: `docker: command not found`

**Cause**: Docker not installed.

**Fix**:
```bash
# macOS
brew install colima  # Or use Docker Desktop

# Start colima
colima start

# Ubuntu/Debian
sudo apt-get install docker.io
sudo usermod -aG docker $USER

# Add to sudo permissions (optional)
sudo visudo
# Add: $USER ALL=(ALL) NOPASSWD: /usr/bin/docker
```

---

### Issue: `error: failed to resolve 'CONFIG_ZETHRA_*' in Rust build`

**Cause**: Rust code trying to reference custom kernel config options that don't exist.

**Fix**:
```bash
# These are expected (Zethra custom features not yet in mainline)
# The build should succeed despite warnings

# Verify initramfs was created
ls -lh build/out/initramfs.cpio.gz

# If build truly failed, check Cargo.lock:
cd services/
cargo check --target aarch64-unknown-linux-musl 2>&1 | grep error | head -5
```

---

### Issue: `cross-compiler mismatch` or Rust target not installed

**Cause**: Rust target for ARM64 not available.

**Fix**:
```bash
# Install ARM64 Rust target
rustup target add aarch64-unknown-linux-musl

# Verify
rustup target list | grep aarch64

# If using Docker (macOS), Docker will handle this automatically
```

---

### Issue: `busybox not found`

**Cause**: Static busybox binary not available in build environment.

**Fix**:
```bash
# If on Linux with busybox installed:
which busybox
# Verify it's static:
file /usr/bin/busybox | grep static

# If on macOS or busybox not available:
# Docker will fetch it automatically (inside container)

# Manual fallback: install via Docker
docker run --rm alpine:latest apk add busybox-static
docker cp $(docker ps -q):/bin/busybox build/out/busybox
```

---

## Boot Image Packing Failures

### Issue: `mkbootimg: command not found`

**Cause**: Android platform tools not installed.

**Fix**:
```bash
# macOS
brew install android-platform-tools

# Ubuntu/Debian
sudo apt-get install android-tools-mkbootimg

# Verify
mkbootimg --version

# Check where it is
which mkbootimg
```

---

### Issue: `error: invalid kernel size` or `invalid ramdisk size`

**Cause**: Boot image packing tool detected corrupt or missing artifacts.

**Fix**:
```bash
# Verify kernel image
file build/out/Image.gz-dtb
# Should show: gzip compressed data

# Verify initramfs
file build/out/initramfs.cpio.gz
# Should show: gzip compressed data

# Check sizes are reasonable
ls -lh build/out/Image.gz-dtb build/out/initramfs.cpio.gz
# Kernel: 5-9 MB
# Initramfs: 1-2 MB

# If either is missing or corrupted:
bash build/scripts/build_kernel.sh --clean
bash build/scripts/build_initramfs.sh
```

---

### Issue: `AVB signature error` or `avbtool not found`

**Cause**: Boot image signing tool missing or key file not available.

**Fix**:
```bash
# Option 1: Pack without signing (for testing)
bash build/scripts/pack_boot_image.sh --no-sign

# Option 2: Manually download avbtool
cd tools/
wget https://android.googlesource.com/platform/external/avb/+/master/avbtool.py
chmod +x avbtool.py

# Option 3: Use system avbtool if available
which avbtool

# Option 4: Use Docker to get avbtool
docker run --rm -v $(pwd):/workspace \
  ubuntu:24.04 bash -c "apt-get update && apt-get install -y android-tools-adb && which avbtool"
```

---

### Issue: Boot Image Header Mismatch

**Cause**: Generated boot image doesn't match stock parameters.

**Fix**:
```bash
# Compare headers with stock
python3 tools/parse_bootimg.py build/out/boot.img > custom.txt
python3 tools/parse_bootimg.py /path/to/stock_boot.img > stock.txt
diff custom.txt stock.txt

# If parameters differ, re-pack with explicit values
vim build/scripts/pack_boot_image.sh
# Adjust: KERNEL_OFFSET, RAMDISK_OFFSET, SECOND_OFFSET, etc.

# Reference from RCA:
# Header v0, page 4096, base 0x0
# kernel_off=0x8000, ramdisk_off=0x01000000
# second_off=0x00f00000, tags_off=0x100
```

---

## Reproducibility Issues

### Issue: Build Outputs Differ Between Runs

**Cause**: Non-deterministic build inputs (timestamps, source tree state, etc.).

**Symptoms**:
```bash
# First build
sha256sum build/out/Image.gz-dtb
# d1a2b3c4...

# Second build (same inputs)
sha256sum build/out/Image.gz-dtb
# e5f6g7h8...  ← Different hash!
```

**Fix**:
```bash
# Verify all inputs are identical
bash build/scripts/build_kernel.sh --pin-check

# Check build manifest
cat build/out/.kernel-build-manifest.txt

# Common causes of non-determinism:
# 1. Timestamps embedded in binary
# 2. Kernel source tree not clean
# 3. Different compiler version used
# 4. Non-deterministic ordering in archives

# Clean and retry
rm -rf linux-6.9 build/out/*
bash build/scripts/build_kernel.sh --clean
bash build/scripts/build_kernel.sh --pin-check

# Compare manifests
diff .kernel-build-manifest.txt.old build/out/.kernel-build-manifest.txt
```

---

### Issue: Manifest Checksum Mismatch

**Cause**: Source files changed between builds or tool versions differ.

**Fix**:
```bash
# Compare source checksums
diff <(sha256sum kernel/zethra_defconfig) \
     <(grep defconfig_sha256 build/out/.kernel-build-manifest.txt)

# If different:
git status kernel/zethra_defconfig
git diff kernel/zethra_defconfig

# If uncommitted changes, either:
# Option 1: Commit the changes
git add kernel/zethra_defconfig
git commit -m "Update: kernel config fixes"

# Option 2: Revert to last known-good state
git checkout kernel/zethra_defconfig

# Then rebuild
bash build/scripts/build_kernel.sh --pin-check
```

---

## Boot Failures (Device-Side)

### Issue: Device Not Detected by Fastboot

**Cause**: Device not in fastboot mode or driver issues.

**Symptoms**:
```bash
$ fastboot devices
# (no output)
```

**Fix**:
```bash
# Step 1: Verify device is connected
lsusb | grep -i nokia

# Step 2: Boot into fastboot mode manually
# Power off device
# Hold: Power + Volume Down
# Release when bootloader screen appears

# Step 3: Try again
fastboot devices

# Step 4: If still not found, check drivers (macOS only)
# Usually not needed on Linux; on macOS:
brew install libusb

# Step 5: From running system, reboot to bootloader
adb reboot bootloader
```

---

### Issue: `fastboot: error: device unauthorized`

**Cause**: Device USB debugging not enabled or authorizing required.

**Fix**:
```bash
# Reconnect device and click "Allow" on the authorization prompt

# Or:
adb devices  # Should show device with "device" status, not "unauthorized"

# If repeatedly unauthorized:
adb kill-server
adb start-server
adb devices  # Authorize again
```

---

### Issue: `fastboot: error: Bootloader locked`

**Cause**: Device bootloader is locked (security feature).

**Fix**:
```bash
# Unlock bootloader (WARNING: Wipes /data)
fastboot oem unlock

# Confirm on device screen

# Verify unlock
fastboot getvar unlocked
# Should output: unlocked: yes

# Re-lock after testing (optional)
fastboot oem lock
```

---

### Issue: Device Silent Reboot Loop (No Console)

**Cause**: Early kernel crash, boot header mismatch, or wrong DTB.

**Symptoms**:
- Device reboots every 25 seconds
- No UART output
- Bootloader splash visible briefly, then reboot

**Fix - Check Boot Image Validity**:
```bash
# Verify boot image header matches stock
python3 tools/parse_bootimg.py build/out/boot.img
python3 tools/parse_bootimg.py /path/to/stock_boot.img
# Compare outputs

# Key fields to check:
# - kernel_offset: should be 0x8000
# - ramdisk_offset: should be 0x01000000
# - page_size: should be 4096
# - header_version: should be 0

# If mismatch, re-pack
bash build/scripts/pack_boot_image.sh
```

**Fix - Check DTB Selection**:
```bash
# Verify DTB is correct
file build/out/sdm636-nokia-frt.dtb
# Should show: Device Tree Blob

# Check board IDs (from RCA, should be MSM ID 345, board ID 8)
strings build/out/sdm636-nokia-frt.dtb | grep -E "(board|msm)" | head -5
```

**Fix - Check Kernel Config**:
```bash
# Verify critical configs are enabled
grep "CONFIG_SERIAL_EARLYCON\|CONFIG_SERIAL_MSM\|CONFIG_BLK_DEV_INITRD" linux-6.9/.config

# Should all show: =y

# If not, rebuild with fixed defconfig
cat build/out/.kernel-build-manifest.txt
# Check defconfig_sha256 matches latest kernel/zethra_defconfig
```

**Fix - Check for Early Console**:
```bash
# Connect serial adapter and monitor UART at 115200 baud
# If no output within 2 seconds of boot, kernel didn't start

# Last resort: Check if stock TWRP still boots (control test)
# Copy TWRP image to device and boot:
fastboot boot twrp.img
# If TWRP boots, bootloader is fine; issue is custom kernel/DTB/config
```

---

### Issue: Kernel Panics (Visible in Ramoops)

**Symptoms**:
```bash
adb shell cat /proc/last_kmsg | head -50
# Shows panic log from previous crash
```

**Fix**:
```bash
# Read full panic log
adb shell cat /proc/last_kmsg > panic_log.txt

# Look for the panic message
grep -A 20 "Kernel panic" panic_log.txt

# Common causes:
# 1. NULL pointer dereference → check for uninitialized structures
# 2. Out of memory → check initramfs size
# 3. Missing driver → check CONFIG_* options in defconfig
# 4. Stack overflow → kernel stack might be too small

# For detailed analysis:
# Export /proc/last_kmsg and analyze with kernel debug symbols:
adb shell cat /proc/last_kmsg | decode_stacktrace.sh
```

---

## Checking Build Reproducibility

### Automated Check Script

**Location**: `build/scripts/verify_reproducibility.sh` (see next section)

**Quick Manual Check**:
```bash
# Step 1: Build once
bash build/scripts/build_kernel.sh --pin-check
MANIFEST1="build/out/.kernel-build-manifest.txt"

# Step 2: Move manifest aside
cp "$MANIFEST1" "${MANIFEST1}.run1"

# Step 3: Clean and rebuild
rm -rf linux-6.9 build/out/*
bash build/scripts/build_kernel.sh --pin-check

# Step 4: Compare
diff "${MANIFEST1}.run1" "$MANIFEST1"

# If identical, build is reproducible ✓
# If differs, investigate why (see: Reproducibility Issues)
```

---

## Getting Help

### Collecting Debug Info for Support

Before reaching out, collect:

```bash
# 1. Full build log
bash build/scripts/build_kernel.sh 2>&1 | tee full_build.log

# 2. Build manifest
cat build/out/.kernel-build-manifest.txt

# 3. Boot image analysis
python3 tools/parse_bootimg.py build/out/boot.img > boot_analysis.txt

# 4. Device info (if booted successfully)
adb shell cat /proc/cpuinfo > device_cpuinfo.txt
adb shell cat /proc/meminfo > device_meminfo.txt
adb shell dmesg > device_dmesg.txt

# 5. Environment info
uname -a
gcc --version
aarch64-linux-gnu-gcc --version
docker --version
rustc --version
cargo --version

# 6. Zip all for upload
tar -czf zethraos_build_debug.tar.gz full_build.log boot_analysis.txt build/out/.*.txt \
  device_*.txt 2>/dev/null || true
```

---

## Reference: Expected Artifact Sizes

For sanity checking during builds:

| Artifact | Typical Size | Min | Max |
|----------|--------------|-----|-----|
| `Image.gz-dtb` | 6-8 MB | 5 MB | 15 MB |
| `sdm636-nokia-frt.dtb` | 50-100 KB | 30 KB | 200 KB |
| `initramfs.cpio.gz` | 1-2 MB | 500 KB | 5 MB |
| `boot.img` | 8-12 MB | 7 MB | 20 MB |
| `vbmeta.img` | 1-5 KB | 1 KB | 10 KB |

If artifacts fall outside these ranges, investigate the build log.

---

## Useful Commands Reference

```bash
# Monitor build in real-time
watch -n 1 'du -sh build/out/* 2>/dev/null | tail -5'

# Find build errors quickly
make ... 2>&1 | grep -E "error:|warning:" | sort | uniq -c

# Clean everything and start fresh
rm -rf linux-6.9 build/out/* cargo build target/

# Check disk space (builds are large)
df -h | grep -E "Filesystem|home|workspace"

# Kill hung build processes
killall make gcc g++ rustc cargo 2>/dev/null || true

# Compare two boot images
cmp -l build/out/boot.img /path/to/other/boot.img | head -20

# Extract and inspect initramfs contents
cd /tmp && \
mkdir -p initramfs_inspect && \
cd initramfs_inspect && \
zcat /path/to/initramfs.cpio.gz | cpio -idmv && \
find . -type f -executable | head -20
```

---

## When All Else Fails

1. **Check the RCA**: Most boot issues are documented in [NOKIA61PLUS_BOOT_RCA.md](../NOKIA61PLUS_BOOT_RCA.md)

2. **Review recent commits**: See what changed last
   ```bash
   git log --oneline -20
   git diff HEAD~1
   ```

3. **Rollback to last known-good state**:
   ```bash
   git checkout <last-good-commit>
   rm -rf linux-6.9 build/out/*
   bash build/scripts/build_kernel.sh --clean
   ```

4. **Verify source integrity**:
   ```bash
   git status
   git diff  # Should be empty for reproducible build
   ```

5. **Check for environment-specific issues**:
   ```bash
   # On macOS
   uname -s  # Should output: Darwin
   # May need Docker for cross-compilation
   
   # On Linux
   uname -s  # Should output: Linux
   # Can compile directly with native cross-compiler
   ```

---

**Last Updated**: 2026-06-11  
**Maintainer**: ZethraOS Team  
**Related**: [Build Scripts](../build/scripts/), [RCA](NOKIA61PLUS_BOOT_RCA.md), [Boot Attempt Guide](NOKIA61PLUS_BOOT_ATTEMPT_N1.md)
