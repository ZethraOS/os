# Nokia 6.1 Plus Boot Attempt N+1 — Quick Reference Guide

**Date**: 2026-06-11  
**Target**: Nokia 6.1 Plus (TA-1103) / SDM636  
**Goal**: Establish reproducible build + early UART/ADB console  

---

## Pre-Boot Checklist

### Device Setup (First Time Only)
- [ ] Enable **Developer Options**: Settings → About Phone → tap Build Number 7×
- [ ] Enable **OEM Unlocking**: Developer Options → toggle "OEM unlocking"
- [ ] Enable **USB Debugging**: Developer Options → toggle "USB Debugging"
- [ ] Connect device to computer via USB-C cable

### Tools Required
```bash
# macOS
brew install android-platform-tools  # Installs fastboot, adb

# Ubuntu/Debian
sudo apt-get install android-tools-adb android-tools-fastboot
```

### Hardware Setup (Optional but Recommended)
- Serial adapter for UART debugging (early console before USB drivers load):
  - Device: 3.3V USB-to-UART adapter
  - Pins: TX (GPIO to adapter RX), RX (adapter TX to GPIO), GND
  - Baud: 115200 bps
  - Speed: Should see logs within 2 seconds of boot

---

## Build Workflow

### Step 1: Prepare Kernel
```bash
cd /Users/nomad/workstation/work/code/OS/Mobile/zethraos

# Clean previous build (optional)
rm -rf linux-6.9 build/out/*

# Build kernel with reproducibility manifest
bash build/scripts/build_kernel.sh

# Check manifest for input checksums
cat build/out/.kernel-build-manifest.txt
```

**Expected output**:
```
✓ Kernel build complete
Artifacts:
  build/out/Image.gz-dtb
  build/out/sdm636-nokia-frt.dtb
```

### Step 2: Build Initramfs
```bash
bash build/scripts/build_initramfs.sh

# Verify ADB was included
ls -lh build/out/initramfs.cpio.gz
```

**Expected output**:
```
✓ initramfs.cpio.gz assembled at: build/out/initramfs.cpio.gz (1.2M)
```

### Step 3: Pack Boot Image
```bash
bash build/scripts/pack_boot_image.sh

# Verify boot image
ls -lh build/out/boot.img build/out/vbmeta.img

# Check parameters match stock
cat build/out/.boot-image-params.txt
```

**Expected output**:
```
✓ Boot image: build/out/boot.img (7.8M)
✓ Manifest: build/out/.boot-pack-manifest.txt
```

---

## Flash & Boot (Critical Test)

### Step 4: Boot into Fastboot Mode

**Option A**: USB cable method (recommended for first attempt)
```bash
# With device powered on, connect USB and run:
fastboot devices

# Should output something like: TA1103A1R3XXX fastboot
```

**Option B**: Manual fastboot boot (no flash)
```bash
# Flash boot partition:
fastboot flash boot build/out/boot.img

# OR boot transiently without flashing (can reboot stock):
fastboot boot build/out/boot.img
```

### Step 5: Monitor Early Console (CRITICAL DATA)

**via UART** (if serial adapter connected):
```bash
# From computer's serial terminal (e.g., minicom, screen):
screen /dev/ttyUSB0 115200

# Expect within ~2 seconds:
# earlycon: msm_serial_dm at 0xc170000 (options: '')
# printk: console [ttyMSM0] enabled
# ... kernel boot messages ...
```

**via ADB** (after ~10 seconds, if USB driver loads):
```bash
adb wait-for-device
adb shell dmesg | head -50

# Or monitor continuously:
adb shell dmesg -w
```

### Step 6: Record Observations

**Reboot**: Press power button 10 seconds (hard reset)

**Capture recovery log**:
```bash
adb shell cat /proc/last_kmsg > attempt_n1_kmsg.txt

# Or if device won't boot:
# Manually note UART output or take photos of serial terminal
```

---

## Expected Success Scenarios

### Scenario 1: Boots to PID 1 ✅
```
[UART output within 5 seconds]
earlycon: msm_serial_dm at 0xc170000 (options: '')
printk: console [ttyMSM0] enabled
Linux version 6.9 ... (see full kernel log)
...
[init] Kernel boot initiated — starting early diagnostics...
[init] Launching PID 1: zethrad...
[zethrad] System initialization starting...
```
**Action**: Celebrate! Move to Step 3: Mount rootfs. Take dmesg logs.

### Scenario 2: Boot Hangs ~25 seconds ⏳
```
[Early kernel output visible]
... normal kernel messages ...
[At ~25 seconds: no more output, device reboots]
```
**Action**: Investigate what subsystem loaded just before hang. Collect UART log.

### Scenario 3: Silent Reboot (No UART Output) 🔇
```
[Device just reboots silently, no console]
```
**Action**: 
- Check bootloader lock state: `fastboot getvar locked`
- Check AVB state: `fastboot getvar dm-verity`
- Try: `fastboot oem unlock` (clears /data)
- Check if test key is trusted by bootloader

### Scenario 4: Bootloader Splash, No Kernel Boot 📺
```
[Stock Android One splash remains, no kernel messages]
```
**Action**: Kernel likely didn't load. Check:
- `fastboot oem unlock` → might be rejected by bootloader
- Verify `boot.img` parameters: `python3 tools/parse_bootimg.py build/out/boot.img`
- Check defconfig matches: `grep CONFIG_SERIAL_EARLYCON build/out/.kernel-build-manifest.txt`

---

## Failure Diagnostics

### "Device Not Found" on `fastboot flash`
```bash
# Check device is in fastboot:
fastboot devices

# If empty:
adb reboot bootloader  # Reboot into fastboot from running system

# If device stuck:
# Manually boot into fastboot: Power + Volume Down for 10s
```

### "Invalid boot image" Error
```bash
# Verify boot image was packed correctly:
python3 tools/parse_bootimg.py build/out/boot.img

# Expected: header_version=0, page_size=4096, kernel_offset=0x8000
# If doesn't match stock, repack with pack_boot_image.sh
```

### Bootloader Rejects Image (AVB Signature Issue)
```bash
# Try flashing without AVB signature:
bash build/scripts/pack_boot_image.sh --no-sign

# Then flash:
fastboot flash boot build/out/boot.img
```

---

## After Boot (If Successful)

### Collect Diagnostic Data
```bash
# Kernel messages
adb shell dmesg > attempt_logs.txt

# CPU info
adb shell cat /proc/cpuinfo

# Memory status
adb shell cat /proc/meminfo

# Boot timing
adb shell cat /proc/uptime

# Check ramoops (logs from previous panic)
adb shell cat /proc/last_kmsg
```

### Test ADB Shell Commands
```bash
adb shell ls /           # List root filesystem
adb shell ps aux         # List running processes
adb shell cat /etc/hostname  # Check hostname
adb shell /system/bin/cmd package list packages  # List packages (if applicable)
```

---

## Reference: Boot Image Parameters

Used for this attempt (from `build/out/.boot-image-params.txt`):

```
Header version:      0
Page size:           4096 bytes
Base address:        0x0
Kernel offset:       0x8000
Ramdisk offset:      0x01000000
Second offset:       0x00f00000
Tags offset:         0x100
OS version:          10.0.0
OS patch level:      2021-08

Command line:
  earlycon=msm_serial_dm,0xc170000
  console=ttyMSM0,115200,n8
  panic=10
  buildvariant=userdebug
```

These match stock/TWRP parameters from RCA.

---

## Build Artifacts Checksums

After each build, verify reproducibility:

```bash
cat build/out/.kernel-build-manifest.txt     # Kernel inputs/outputs
cat build/out/.boot-image-params.txt         # Boot parameters
cat build/out/.boot-pack-manifest.txt        # Final boot image hash
```

Compare successive builds:
```bash
# If checksums differ between builds, investigate:
# (e.g., timestamps, compiler version, kernel source state)
```

---

## When to Escalate / Call for Help

**If the device**:
- Boots stock Android successfully (TWRP control still works) → Good, bootloader is fine
- Boots ZethraOS kernel → **SUCCESS**, document and move to next Gate
- Hangs in early kernel → **Collect UART log**, analyze with `dmesg`
- Reboots silently with no UART → **Likely AVB/unlock issue**, try `fastboot oem unlock`
- Never boots ZethraOS, only stock → **Likely boot.img issue**, re-verify parameters

**Before reporting**, ensure you have:
1. Full UART log (or screenshot) from `dmesg`
2. Output from `fastboot getvar all`
3. Build manifest checksums (`build/out/.*.txt` files)
4. `parse_bootimg.py` analysis of your `boot.img`

---

## Key Contacts & Resources

- **RCA Document**: `docs/NOKIA61PLUS_BOOT_RCA.md`
- **Build Scripts**: `build/scripts/{build_kernel,build_initramfs,pack_boot_image,flash_nokia61plus}.sh`
- **Kernel Config**: `kernel/zethra_defconfig`
- **DTS**: `linux-6.9/arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dts`

---

**Good luck! 🚀**
