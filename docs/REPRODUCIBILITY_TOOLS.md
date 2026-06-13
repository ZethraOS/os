# Reproducibility Testing & Verification Guide

**Purpose**: Ensure ZethraOS builds are deterministic and auditable  
**Created**: 2026-06-11  
**Scope**: Kernel, initramfs, boot image builds

---

## Overview: Four Reproducibility Tools

| Tool | Purpose | Time | When to Use |
|------|---------|------|------------|
| **quick_reproducibility_check.sh** | Pre-flight check (30 sec) | <1 min | Before every build |
| **check_source_integrity.sh** | Validate source code (commit/checksum) | <1 min | After git changes |
| **verify_reproducibility.sh** | Full 2x build comparison (30+ min) | 30-45 min | Before releases, after major changes |
| **parse_bootimg.py** | Boot image validation | <1 sec | When boot image won't flash |

---

## Quick Reference Workflow

### Pre-Build (Always Do This First)
```bash
# ✓ 1. Quick reproducibility check
bash build/scripts/quick_reproducibility_check.sh

# If "READY FOR REPRODUCIBLE BUILD ✓", proceed to builds
# If issues, fix them (see troubleshooting below)
```

### Build Workflow
```bash
# ✓ 2. Build kernel (produces .kernel-build-manifest.txt)
bash build/scripts/build_kernel.sh --pin-check

# ✓ 3. Build initramfs
bash build/scripts/build_initramfs.sh

# ✓ 4. Pack boot image (produces .boot-pack-manifest.txt)
bash build/scripts/pack_boot_image.sh

# ✓ 5. Flash to device
bash build/scripts/flash_nokia61plus.sh
```

### Post-Release (Full Verification)
```bash
# ✓ For production release, verify reproducibility
bash build/scripts/verify_reproducibility.sh

# Expected: "REPRODUCIBLE ✓"
# This compares 2 consecutive builds for byte-identical artifacts
```

---

## Tool Descriptions & Usage

### 1. Quick Reproducibility Check (30 seconds)

**What it does**:
- Checks if repository is clean
- Verifies build artifacts exist
- Spot-checks artifact checksums in manifests
- Confirms build infrastructure is present

**When to use**:
- Before starting any build
- After git commits
- In CI/CD pre-build step

**Usage**:
```bash
bash build/scripts/quick_reproducibility_check.sh

# Optional: verbose output
bash build/scripts/quick_reproducibility_check.sh --verbose
```

**Exit codes**:
- `0`: Ready to build ✓
- `1`: Issues detected, fix before building

**Example Output**:
```
✓ Repository is clean
✓ All expected artifacts present
✓ Manifest valid: .kernel-build-manifest.txt
✓ All build scripts present
✓ Kernel source tree is clean

✓ Quick check: READY FOR REPRODUCIBLE BUILD ✓
```

**When to Fix Issues**:
```bash
# If check says "ISSUES DETECTED":

# Issue 1: Uncommitted changes
git add -A
git commit -m "Build fixes"

# Issue 2: Dirty repo
git clean -fdx  # Removes untracked files

# Issue 3: Missing artifacts
bash build/scripts/build_kernel.sh
bash build/scripts/build_initramfs.sh
bash build/scripts/pack_boot_image.sh
```

---

### 2. Check Source Integrity (< 1 minute)

**What it does**:
- Verifies kernel source tree is clean (no modifications)
- Checks defconfig hasn't changed
- Validates all build scripts are present
- Optionally generates/compares checksums

**When to use**:
- After downloading kernel source
- After major refactoring
- To generate baseline checksums for CI/CD

**Usage**:
```bash
# Normal: Check integrity
bash build/scripts/check_source_integrity.sh

# Generate baseline checksums (first run or after major changes)
bash build/scripts/check_source_integrity.sh --generate-checksums

# Verbose: Show all details
bash build/scripts/check_source_integrity.sh --verbose

# Auto-fix: Clean up detected issues
bash build/scripts/check_source_integrity.sh --fix
```

**Output Example**:
```
✓ Kernel source tree is clean
✓ Defconfig matches expected checksum
✓ All required build scripts present
✓ Main repository is clean
✓ Source integrity check: All clear ✓
```

**Checksum File**:
- Location: `build/.source-checksums.txt`
- Format: One per line: `<type> <sha256_hash>`
- Example:
  ```
  defconfig 5d4c3b2a1f0e9d8c7b6a5f4e3d2c1b0a
  dts abc123...
  script_build_kernel.sh def456...
  ```

---

### 3. Verify Full Reproducibility (30-45 minutes)

**What it does**:
- Performs **two consecutive builds** from identical sources
- Compares all artifacts byte-by-byte
- Generates detailed report of any differences
- Identifies sources of non-determinism

**When to use**:
- Before production release
- After making kernel changes that could affect reproducibility
- When debugging non-determinism issues
- In CI/CD release pipeline

**Usage**:
```bash
# Standard: Full reproducibility test
bash build/scripts/verify_reproducibility.sh

# Verbose: Show build logs during test
bash build/scripts/verify_reproducibility.sh --verbose

# Preserve: Keep build directories for manual inspection
bash build/scripts/verify_reproducibility.sh --preserve

# Quick mode: Skip heavy cleanup steps (faster iteration)
bash build/scripts/verify_reproducibility.sh --quick
```

**What It Tests**:
```
Build 1:
  - Run build_kernel.sh
  - Run build_initramfs.sh
  - Run pack_boot_image.sh
  [Save all artifacts & manifests]

Clean phase:
  - Remove linux-6.9/ and build/out/*
  - Optional: Remove target/ and other heavy dirs

Build 2:
  - Run build_kernel.sh again
  - Run build_initramfs.sh again
  - Run pack_boot_image.sh again
  [Save all artifacts & manifests]

Comparison:
  - Hash each artifact from Build 1 and Build 2
  - Compare sha256 checksums
  - Report: REPRODUCIBLE ✓ or NON-REPRODUCIBLE ✗
```

**Output**: Detailed report at `build/out/.reproducibility-report.txt`

**Example Report (Success)**:
```
## ✓ REPRODUCIBLE

Consecutive builds produced **byte-identical** artifacts:

| Artifact | Hash |
|----------|------|
| Image.gz-dtb | abc123... |
| initramfs.cpio.gz | def456... |
| boot.img | ghi789... |

Implications: The build process is deterministic.
```

**Example Report (Non-Deterministic)**:
```
## ✗ NON-REPRODUCIBLE

Builds produced **different** artifacts despite identical inputs.

Differences Found:
- Hash mismatch: Image.gz-dtb

Common Causes:
1. Timestamps embedded in binaries
2. Non-deterministic file ordering
3. Compiler version differences
...
```

---

### 4. Boot Image Validation (< 1 second)

**What it does**:
- Parses Android boot image header
- Shows kernel/ramdisk offsets and sizes
- Compares with stock/reference boot images

**When to use**:
- When boot image won't flash
- Before flashing to verify header matches stock
- Debugging boot failures

**Usage**:
```bash
# Parse boot image header
python3 tools/parse_bootimg.py build/out/boot.img

# Compare with stock
python3 tools/parse_bootimg.py build/out/boot.img > custom.txt
python3 tools/parse_bootimg.py /path/to/stock_boot.img > stock.txt
diff custom.txt stock.txt
```

**Expected Output**:
```
Magic: ANDROID!
Kernel offset: 0x8000
Ramdisk offset: 0x01000000
Second offset: 0x00f00000
Tags offset: 0x100
Page size: 4096
Header version: 0
...
```

---

## Reproducibility Workflow Examples

### Example 1: Daily Development

```bash
# Start of day
cd /Users/nomad/workstation/work/code/OS/Mobile/zethraos

# 1. Quick check (30 sec)
bash build/scripts/quick_reproducibility_check.sh
# → "READY FOR REPRODUCIBLE BUILD ✓"

# 2. Build
bash build/scripts/build_kernel.sh
bash build/scripts/build_initramfs.sh
bash build/scripts/pack_boot_image.sh

# 3. Flash & test
bash build/scripts/flash_nokia61plus.sh
```

### Example 2: Making Kernel Changes

```bash
# Edit kernel config
vim kernel/zethra_defconfig

# Commit changes (for reproducibility tracking)
git add kernel/zethra_defconfig
git commit -m "Enable CONFIG_XYZ for feature ABC"

# Check integrity
bash build/scripts/check_source_integrity.sh

# Build with changes
bash build/scripts/build_kernel.sh

# Verify reproducibility (full test)
bash build/scripts/verify_reproducibility.sh
```

### Example 3: Pre-Release Verification

```bash
# Full reproducibility test
bash build/scripts/verify_reproducibility.sh

# Check report
cat build/out/.reproducibility-report.txt

# If "REPRODUCIBLE ✓":
echo "Ready for release!"
git tag -a v0.2.0-rc1 -m "Pre-release build verified reproducible"

# If "NON-REPRODUCIBLE ✗":
echo "Investigate non-determinism before release"
cat build/out/.reproducibility-errors.txt
```

### Example 4: CI/CD Pipeline

```bash
#!/bin/bash
set -e

# Pre-build check
bash build/scripts/quick_reproducibility_check.sh

# Build
bash build/scripts/build_kernel.sh --pin-check
bash build/scripts/build_initramfs.sh
bash build/scripts/pack_boot_image.sh

# Verify boot image header
python3 tools/parse_bootimg.py build/out/boot.img

# For release builds, verify full reproducibility
if [[ "$BUILD_TYPE" == "release" ]]; then
  bash build/scripts/verify_reproducibility.sh || {
    echo "FAILED: Build not reproducible"
    exit 1
  }
fi

# Success
echo "Build verified and ready for deployment"
```

---

## Common Issues & Fixes

### "Quick check: ISSUES DETECTED ✗"

**Cause**: Repository has uncommitted changes  
**Fix**:
```bash
# See what changed
git status

# Option 1: Commit changes
git add -A
git commit -m "My changes"

# Option 2: Discard changes
git checkout .

# Retry
bash build/scripts/quick_reproducibility_check.sh
```

### "Manifest: NEEDS INVESTIGATION ⚠️"

**Cause**: Two builds produced different artifacts  
**Fix**:
```bash
# Review the detailed report
cat build/out/.reproducibility-report.txt

# Common causes:
# 1. Check for embedded timestamps
strings build/out/Image.gz-dtb | grep "202[0-9]-"

# 2. Verify compiler versions match
gcc --version
aarch64-linux-gnu-gcc --version

# 3. Clean and retry
rm -rf linux-6.9 build/out/*
bash build/scripts/verify_reproducibility.sh --quick
```

### "Boot image header mismatch with stock"

**Cause**: Boot image not packed with correct parameters  
**Fix**:
```bash
# Compare headers
python3 tools/parse_bootimg.py build/out/boot.img > custom.txt
python3 tools/parse_bootimg.py /path/to/stock_boot.img > stock.txt
diff custom.txt stock.txt

# Re-pack with correct parameters
bash build/scripts/pack_boot_image.sh

# Verify
python3 tools/parse_bootimg.py build/out/boot.img
```

---

## Integration with CI/CD

### GitHub Actions Example

```yaml
name: Build Verification

on: [push, pull_request]

jobs:
  reproducibility:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      
      - name: Install dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y gcc-aarch64-linux-gnu build-essential

      - name: Quick check
        run: bash build/scripts/quick_reproducibility_check.sh

      - name: Build
        run: |
          bash build/scripts/build_kernel.sh --pin-check
          bash build/scripts/build_initramfs.sh
          bash build/scripts/pack_boot_image.sh

      - name: Upload artifacts
        uses: actions/upload-artifact@v2
        with:
          name: build-artifacts
          path: build/out/

      - name: Reproducibility test (on release only)
        if: startsWith(github.ref, 'refs/tags/v')
        run: bash build/scripts/verify_reproducibility.sh

      - name: Upload report
        if: always()
        uses: actions/upload-artifact@v2
        with:
          name: reproducibility-report
          path: build/out/.reproducibility-report.txt
```

---

## Reproducibility Metrics & Monitoring

### Track Over Time

```bash
# Generate build baseline
bash build/scripts/check_source_integrity.sh --generate-checksums

# Store baseline
cp build/.source-checksums.txt build/.source-checksums.v1.0.txt

# After changes, compare
diff build/.source-checksums.v1.0.txt build/.source-checksums.txt
```

### Monitor Build Times

```bash
# Log build time
time bash build/scripts/build_kernel.sh 2>&1 | tee build_timing.log

# Extract timing
grep "^real" build_timing.log

# Track improvements/regressions
echo "Kernel build: $(grep real build_timing.log)" >> build_metrics.txt
```

---

## Reference: Reproducibility Gates

From RCA: [NOKIA61PLUS_BOOT_RCA.md](../docs/NOKIA61PLUS_BOOT_RCA.md)

| Gate | Status | Check | Evidence |
|------|--------|-------|----------|
| **Gate 0** | IN PROGRESS | Build reproducible | `verify_reproducibility.sh` |
| **Gate 1** | Planned | Boot tooling w/ known-good control | `parse_bootimg.py` comparison |
| **Gate 2** | Planned | Early UART console | UART logs from device |
| **Gate 3** | Planned | PID 1 reach | Kernel boot messages |

---

## Support & Troubleshooting

**All troubleshooting**: See [BUILD_TROUBLESHOOTING.md](BUILD_TROUBLESHOOTING.md)

**Quick links**:
- [Kernel build errors](BUILD_TROUBLESHOOTING.md#kernel-build-failures)
- [Initramfs errors](BUILD_TROUBLESHOOTING.md#initramfs-build-failures)
- [Boot image errors](BUILD_TROUBLESHOOTING.md#boot-image-packing-failures)
- [Reproducibility issues](BUILD_TROUBLESHOOTING.md#reproducibility-issues)
- [Device boot failures](BUILD_TROUBLESHOOTING.md#boot-failures-device-side)

---

## Summary

| Goal | Use This | Time |
|------|----------|------|
| Pre-flight check | `quick_reproducibility_check.sh` | <1 min |
| Verify source code | `check_source_integrity.sh` | <1 min |
| Validate boot image | `parse_bootimg.py` | <1 sec |
| Full reproducibility test | `verify_reproducibility.sh` | 30-45 min |

**Best practice**: Run quick check before every build, full verification before releases.

---

**Document**: [REPRODUCIBILITY_TOOLS.md](REPRODUCIBILITY_TOOLS.md)  
**Created**: 2026-06-11  
**Updated**: 2026-06-11
