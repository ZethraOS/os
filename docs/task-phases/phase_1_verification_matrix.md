# Phase 1 Verification Matrix

**Target**: Nokia 6.1 Plus (DRG / TA-1103) / Qualcomm SDM636  
**Baseline release**: `v0.3.0` at commit `c911358`  
**On-device verification date**: 2026-06-13  
**Documentation review date**: 2026-06-15  
**Overall status**: Completed, with evidence-retention gaps documented below

This file is the canonical status record for Phase 1. The detailed implementation
summary is in [phase_1_completion_report.md](phase_1_completion_report.md), and
the investigation history is in
[NOKIA61PLUS_BOOT_RCA.md](../NOKIA61PLUS_BOOT_RCA.md).

## Verification Levels

| Level | Meaning |
| --- | --- |
| **Repository-proven** | The implementation and/or result is present in tracked Git history. |
| **Local-artifact-proven** | A generated log or manifest exists in `build/out/`, which is Git-ignored and therefore not durable project evidence. |
| **Recorded on-device observation** | The completion report records terminal output observed on the phone, but the raw transcript is not committed separately. |
| **Needs re-validation** | The claim cannot be reproduced without rebuilding or reconnecting the physical device. |

## Phase 1 Gates

| Gate | Status | Evidence | Verification level | Re-check requirement |
| --- | --- | --- | --- | --- |
| Deterministic build | Passed on 2026-06-13 | `build/out/.reproducibility-report.txt` records byte-identical kernel, initramfs, and boot image outputs from two builds at commit `eeb034e` | Local-artifact-proven | Docker or Linux host; no phone required |
| Linux kernel boot | Passed on 2026-06-13 | `build/out/zethra_boot.log` identifies Linux 6.9.0 and machine model `Nokia 6.1 Plus` | Local-artifact-proven | Phone required to reproduce |
| Nokia DT selected | Passed on 2026-06-13 | Same boot log reports the Nokia machine model and SDM636 platform devices | Local-artifact-proven | Phone required to reproduce |
| eMMC discovery | Passed on 2026-06-13 | Same boot log reports `mmcblk1`, 58.2 GiB, and partitions `p1` through `p85` | Local-artifact-proven | Phone required to reproduce |
| Initramfs `/init` reached | Passed on 2026-06-13 | Same boot log reports `Run /init as init process` | Local-artifact-proven | Phone required to reproduce |
| Persist partition mounted | Passed on 2026-06-13 | Same boot log reports successful read/write EXT4 mount of `mmcblk1p73` | Local-artifact-proven | Phone required to reproduce |
| USB CDC ACM root shell | Passed on 2026-06-13 | Completion report records a responsive root shell at `/dev/tty.usbmodemZETHRA0000011` | Recorded on-device observation | Phone required for independent re-validation |
| `zethrad` as PID 1 | Passed on 2026-06-13 | Completion report records `zethrad: PID 1` and an on-device `ps` process list | Recorded on-device observation | Phone required for independent re-validation |
| Core daemon supervision | Passed on 2026-06-13 | Completion report records `sensord`, `networkd`, and `otad` running under `zethrad` | Recorded on-device observation | Phone required for independent re-validation |
| Repeat-boot stability | Not established | No durable evidence of the earlier proposed ten consecutive successful boots | Needs re-validation | Phone required |

## Evidence Integrity Notes

- Files under `build/out/` are ignored by Git. They are useful local evidence,
  but a clean clone cannot audit them.
- The current local kernel and boot manifests were regenerated after the Phase 1
  branch, while uncommitted Phase 2 changes exist. They must not be presented as
  hashes of the exact Phase 1 image.
- The reproducibility report references commit `eeb034e`; the Phase 1 work was
  later merged through pull request `#26` and tagged as `v0.3.0` at `c911358`.
- The raw ACM terminal session, `/mnt/persist/zethrad.log`, and `ps` output quoted
  in the completion report are not stored as separate committed evidence files.
- The user recalls completing this work with the Antigravity Gemini 3.5 High
  model. Git history does not preserve model/session metadata, so this
  attribution is historical context rather than independently verifiable
  technical evidence.

## What Can Be Re-verified Without the Phone

The following checks can run on this Mac using the repository's Docker-backed
build scripts or on a Linux build host:

- Rust lint, unit tests, and ARM64 cross-compilation.
- Linux 6.9 kernel and Nokia DTB compilation.
- Initramfs construction and content inspection.
- Android boot image header, size, AVB footer, and manifest inspection.
- Two-build reproducibility comparison from a clean, pinned commit.
- Generic QEMU boot of the userspace/init path.

QEMU uses the generic `virt` machine. It does not emulate the Nokia bootloader,
SDM636 board, PM660/PM660L power tree, Nokia partition map, DWC3 gadget path,
OTM1911A display, or Adreno 509 GPU.

## What Requires the Nokia 6.1 Plus

A connected physical device is required to independently re-validate:

- ABL/AVB acceptance and A/B slot behavior.
- Boot of the Nokia-specific DTB on SDM636.
- eMMC and persist partition behavior.
- USB CDC ACM enumeration and interactive shell stability.
- `zethrad` PID 1 and daemon supervision on the real hardware.
- Reboot, power-cycle, and repeat-boot stability.
- Every Phase 2 display, backlight, DSI, DRM, and Adreno GPU gate.

## Current Re-verification Decision

Phase 1 does not need to be re-flashed merely to continue documentation cleanup.
Before Phase 2 code is flashed or Phase 1 is advertised as a reproducible
community release, perform a controlled device evidence capture from the tagged
`v0.3.0` baseline and archive:

- Git commit and dirty-state output.
- Artifact SHA-256 values.
- Fastboot slot and boot-chain state.
- Complete boot log.
- ACM enumeration and `uname`, `id`, `ps`, and mount output.
- `/mnt/persist/zethrad.log`.
- Results of at least three consecutive cold boots.

