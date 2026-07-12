# Nokia 6.1 Plus Slot A Control Baseline

**Date frozen:** 2026-07-12  
**Device:** Nokia 6.1 Plus / DRG / TA-1103 / SDM636  
**Purpose:** Recovery/control baseline before any further graphical bring-up  
**Policy:** Slot A is frozen. Slot B is the only future test target.

## Executive Summary

Slot A is now the known-good recovery/control baseline for Nokia 6.1 Plus bring-up. It has been verified to reach ZethraOS initramfs/userspace over USB CDC ACM with `/sbin/zethrad` as PID 1 and core Zethra daemons running. This is a userspace/control baseline only; it does not prove display, panel, GPU, compositor, or graphical health. No display or panel work may continue until this baseline remains documented, protected, and explicitly referenced by the next Slot B experiment.

## Hard Rules

- Do not flash Slot A.
- Do not set Slot A as an experimental target.
- Do not modify Slot A contents.
- Do not use `build/scripts/flash_nokia61plus.sh` defaults without explicitly overriding the slot, because that script defaults to Slot A.
- Slot B is the only permitted test target after explicit approval.
- Any future experiment must have one variable, one success criterion, and one rollback criterion.
- Display/panel work is paused until an approved Slot B experiment is selected.

## Verified Live Slot State

Captured from fastboot on 2026-07-12 after Slot A recovery:

```text
current-slot:a
slot-retry-count:a:5
slot-unbootable:a:no
slot-successful:a:yes
slot-retry-count:b:0
slot-unbootable:b:yes
slot-successful:b:no
serialno:DRGID18100509899
product:fih_sdm660_64
```

Interpretation:

- Slot A is current, bootable, and marked successful.
- Slot B is currently unbootable and has zero retries.
- Slot B must be reset/re-prepared only after an explicit experiment approval.

## Verified Slot A Userspace Evidence

Evidence log:

```text
build/out/acm_logs/slot-a-requested-three-commands-cu-rw-20260712T181244+0530.log
```

ACM command capture used only:

```sh
cat /proc/cmdline
ps
dmesg | tail
```

### `/proc/cmdline` Evidence

Key verified fields:

```text
androidboot.slot_suffix=_a
root=/dev/dm-0
/dev/mmcblk0p81
buildvariant=userdebug
panic=10
msm.separate_gpu_kms=1
download_mode=0
```

Interpretation:

- The booted userspace was Slot A.
- The system dm target points at the Slot A side.
- This boot used the current ZethraOS/userdebug bring-up command-line path.

### Process Evidence

Key verified processes:

```text
1 0 /sbin/zethrad
710 0 {init} /bin/sh /init
725 0 {init} /bin/sh /init
731 0 /bin/sh
751 0 /usr/lib/zethra/sensord/zethra-sensord
754 0 /usr/lib/zethra/networkd/zethra-networkd
755 0 /usr/lib/zethra/otad/zethra-otad
```

Interpretation:

- ZethraOS userspace is reached.
- `zethrad` is PID 1.
- `sensord`, `networkd`, and `otad` are running.
- ACM shell is available.

### `dmesg | tail` Evidence

Captured tail:

```text
[    2.034491]     earlycon=msm_serial_dm,0xc170000
[    2.034502]     buildvariant=userdebug
[    2.034510]     dm=system none ro,0 1 android-verity /dev/mmcblk0p81
[    2.034520]     download_mode=0
[    4.599268] EXT4-fs (mmcblk1p73): warning: maximal mount count reached, running e2fsck is recommended
[    4.605248] EXT4-fs (mmcblk1p73): recovery complete
[    4.608643] EXT4-fs (mmcblk1p73): mounted filesystem 57f8f4bc-abf4-655f-bf67-946fc0f9f25b r/w with ordered data mode. Quota mode: disabled.
[    9.158947] random: crng init done
```

Interpretation:

- Persist partition mounted read/write.
- Kernel reached normal userspace late enough for entropy initialization.
- The ext4 mount-count warning is an operations hygiene item, not a display bring-up finding.

## What This Baseline Does Not Prove

- It does not prove display or panel initialization.
- It does not prove GPU/KMS health.
- It does not prove compositor health.
- It does not prove Android framework boot.
- It does not prove Slot B is recoverable without re-preparation.
- It does not prove any prior `mock_attach` or checkpoint hypothesis.

## Stale Claims Superseded By This Baseline

- Any statement that Slot B is the known-good boot baseline is stale.
- Any statement that current graphical failures should be debugged before protecting Slot A is stale.
- Any statement that a boot is successful based only on USB enumeration is insufficient.
- Any claim about the currently flashed test image must be re-proven before use.

## Next Slot B Experiment Draft

This is a draft only. Do not execute without explicit approval.

### Objective

Re-enable Slot B as the test slot without changing display, panel, kernel source, or boot image contents.

### Single Variable

Only the A/B boot metadata for Slot B is changed from unbootable/zero-retry to active-testable state.

### Explicitly Not Included

- No flashing.
- No kernel rebuild.
- No DTS change.
- No panel driver change.
- No display/panel experiment.
- No Slot A write.

### Proposed Command Sequence

Read-only pre-check:

```sh
fastboot getvar all
```

Only after approval, reset the Slot B boot attempt state by selecting Slot B:

```sh
fastboot set_active b
fastboot reboot
```

Observation:

- Watch for fastboot return.
- Watch for ACM enumeration.
- If ACM appears, capture only `/proc/cmdline`, `ps`, and `dmesg | tail`.

### Success Criterion

The experiment is successful only if Slot B reaches ACM userspace and reports:

```text
androidboot.slot_suffix=_b
/sbin/zethrad as PID 1
```

### Failure Criterion

The experiment fails if the device returns to fastboot before ACM userspace is captured, or if ACM never appears within the agreed timeout.

### Rollback Criterion

Rollback is immediate and limited to:

```sh
fastboot set_active a
fastboot reboot
```

Rollback is considered successful only after Slot A again reaches ACM userspace with `/sbin/zethrad` as PID 1.

## Required Approval Gate

Before executing the Slot B experiment, the approver must explicitly confirm:

```text
Approved: run Slot B metadata-only boot test. Do not flash. Do not patch. Keep Slot A contents untouched.
```
