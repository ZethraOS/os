# Quick Reference Card

**ZethraOS Nokia 6.1 Plus Build & Reproducibility Tools**  
**Last Updated**: 2026-06-15

**Phase 1 status**: Completed on-device on 2026-06-13. See the
[Phase 1 verification matrix](task-phases/phase_1_verification_matrix.md) for
evidence strength and re-test requirements.

---

## Build Sequence

```bash
bash build/scripts/quick_reproducibility_check.sh && \
bash build/scripts/build_kernel.sh && \
bash build/scripts/build_initramfs.sh && \
bash build/scripts/pack_boot_image.sh
```

Flashing is intentionally separate from building. Confirm the commit, dirty
state, artifact hashes, active slot, and rollback slot before any device write.

---

## Tool Matrix

| Tool | What It Does | Time | Command |
|------|-------------|------|---------|
| `quick_reproducibility_check.sh` | Pre-build validation | <1m | `bash build/scripts/quick_reproducibility_check.sh` |
| `check_source_integrity.sh` | Verify source unchanged | <1m | `bash build/scripts/check_source_integrity.sh` |
| `build_kernel.sh` | Compile kernel + DTB | 15-30m | `bash build/scripts/build_kernel.sh --pin-check` |
| `build_initramfs.sh` | Build root + ADB | 5-10m | `bash build/scripts/build_initramfs.sh` |
| `pack_boot_image.sh` | Create boot.img | 1-2m | `bash build/scripts/pack_boot_image.sh` |
| `flash_nokia61plus.sh` | Flash to device | 1m | `bash build/scripts/flash_nokia61plus.sh` |
| `verify_reproducibility.sh` | 2x build comparison | 30-45m | `bash build/scripts/verify_reproducibility.sh` |
| `parse_bootimg.py` | Validate boot image | <1s | `python3 tools/parse_bootimg.py build/out/boot.img` |

---

## Documentation Map

| Document | Situation |
|----------|-----------|
| [task-phases/phase_1_verification_matrix.md](task-phases/phase_1_verification_matrix.md) | Need the current Phase 1 status and evidence |
| [task-phases/phase_1_completion_report.md](task-phases/phase_1_completion_report.md) | Need the detailed Phase 1 completion record |
| [NOKIA61PLUS_BOOT_RCA.md](NOKIA61PLUS_BOOT_RCA.md) | Need failure history and root causes |
| [NOKIA61PLUS_BOOT_ATTEMPT_N1.md](NOKIA61PLUS_BOOT_ATTEMPT_N1.md) | Need the historical N+1 procedure |
| [BUILD_TROUBLESHOOTING.md](BUILD_TROUBLESHOOTING.md) | Build fails, need a diagnostic path |
| [REPRODUCIBILITY_TOOLS.md](REPRODUCIBILITY_TOOLS.md) | Need to verify build reproducibility |

---

## Defconfig Fixes at a Glance

```diff
- CONFIG_QCOM_PM8953 (wrong PMIC)
+ CONFIG_QCOM_PM660 + CONFIG_QCOM_PM660L (correct)

- CONFIG_DRM_PANEL_TRULY_NT35597_WQXGA (wrong panel)
+ OTM1911A identified from vendor metadata; upstream Linux 6.9 driver absent

- # CONFIG_USB is not set (disabled)
+ CONFIG_USB=y (re-enabled)

- # CONFIG_USB_DWC3 is not set (disabled)
+ CONFIG_USB_DWC3=y (re-enabled)

- (missing)
+ CONFIG_SERIAL_EARLYCON=y (early console)
```

---

## Expected Boot Success Output

**UART Console** (within 2 sec):
```
earlycon: msm_serial_dm at 0xc170000 (options: '')
printk: console [ttyMSM0] enabled
... Linux 6.9 kernel boot messages ...
```

**PID 1** (within 10 sec):
```
[init] Kernel boot initiated
[init] Launching PID 1: zethrad...
```

**ADB** (check anytime):
```bash
$ adb shell dmesg | tail -20
```

---

## Troubleshooting Flowchart

```
Build fails?
  ├─ "cross-compiler not found" 
  │  └─ brew install arm64-elf-gcc
  ├─ "missing symbols"
  │  └─ Normal (51 Zethra symbols expected)
  └─ See: BUILD_TROUBLESHOOTING.md

Device doesn't boot?
  ├─ No UART output
  │  └─ Check fastboot lock: fastboot oem unlock
  ├─ Kernel panic
  │  └─ adb shell cat /proc/last_kmsg
  └─ See: NOKIA61PLUS_BOOT_ATTEMPT_N1.md #failure-diagnostics

Build not reproducible?
  └─ bash build/scripts/verify_reproducibility.sh --verbose
```

---

## Essential Files

| File | Purpose |
|------|---------|
| `kernel/zethra_defconfig` | Kernel configuration (FIXED) |
| `build/scripts/build_kernel.sh` | Build kernel (REPRODUCIBLE) |
| `build/scripts/build_initramfs.sh` | Build root FS (ADB SUPPORT) |
| `build/scripts/pack_boot_image.sh` | Pack boot image (NEW) |
| `build/out/.kernel-build-manifest.txt` | Build checksums |
| `build/out/boot.img` | Final image to flash |

---

## Key Metrics

| Metric | Before | After |
|--------|--------|-------|
| Early UART console | ❌ Disabled | ✅ Enabled |
| USB/ADB debugging | ❌ Disabled | ✅ Enabled |
| PMIC config | ❌ Wrong (PM8953) | ✅ Correct (PM660) |
| Panel identity | Wrong (NT35597) | OTM1911A identified; driver still required |
| Reproducibility tracking | ❌ None | ✅ Full manifests |
| Diagnostics on boot fail | ❌ Silent reboot | ✅ Logs via UART/ADB/ramoops |

---

## Re-verification Path

1. Read the
   [Phase 1 verification matrix](task-phases/phase_1_verification_matrix.md).

2. Rebuild from the clean `v0.3.0` baseline in Docker or on Linux.

3. Record the commit, manifests, and artifact hashes.

4. Connect the Nokia only when on-device re-validation is scheduled.

5. Verify the active and rollback slots before using the flash script.

Build-only commands:
```bash
bash build/scripts/quick_reproducibility_check.sh
bash build/scripts/build_kernel.sh
bash build/scripts/build_initramfs.sh
bash build/scripts/pack_boot_image.sh
```

---

## Status: Phase 1 Complete

- [x] Deterministic build recorded
- [x] Linux 6.9 booted on Nokia 6.1 Plus
- [x] eMMC detected and persist partition mounted
- [x] Initramfs `/init` reached
- [x] USB CDC ACM root shell recorded
- [x] `zethrad` recorded as PID 1 with core daemons
- [ ] Repeat-boot stability evidence still needs durable capture

---

**Total effort**: 
- 4 build scripts enhanced/created
- 5 documentation files
- ~2,500 lines of documentation
- ~1,500 lines of reproducible build code
- All in support of **ONE goal**: Get early console output on boot ✅

Do not use the historical one-command flash path without first confirming the
active slot, rollback slot, artifact hashes, and clean Git state.
