# Slot B Experiment B-01 — Panel Driver Probe on Slot B

**Date drafted:** 2026-07-15  
**Branch:** `feature/phase2-track-b-panel-bringup`  
**Status:** AWAITING APPROVAL — do not execute until the user explicitly confirms.  
**Control baseline:** v0.4.0 (`4293bc4`) — Slot A frozen, untouchable.  
**Reference:** [NOKIA61PLUS_STOCK_DTB_HARDWARE_REFERENCE.md](NOKIA61PLUS_STOCK_DTB_HARDWARE_REFERENCE.md)

---

## Objective

Boot the current ZethraOS image (already built with the correct panel DTS and OTM1911A driver enabled) on **Slot B** and observe whether the panel driver probes successfully — or fails with a specific, actionable error — without touching Slot A.

---

## Single Variable

**One and only one change from the current Slot A baseline:**

> Flash `build/out/boot.img` (SHA-256 `5a5d2a2f696a2e137f243f781ffb2b03cbf700354c6c8da2128912bc32eb3fcf`) to the Slot B boot partition, then switch the active slot to B.

Everything else remains identical to the Slot A baseline:
- Same kernel source tree (built from `b4ac9a9`)
- Same DTS (`sdm636-nokia-frt.dts`) — hardware-verified against stock DTB
- Same initramfs and userspace
- Same `zethra_defconfig`
- No Slot A writes of any kind

---

## Explicitly Not Included in This Experiment

- No kernel source changes
- No DTS changes
- No defconfig changes
- No panel driver code changes
- No Slot A read or write operations
- No display output expectation — the panel driver producing **any dmesg probe trace** is sufficient

---

## Pre-Experiment Read-Only Check

Before flashing, verify the device slot state matches the v0.4.0 baseline exactly:

```sh
fastboot getvar current-slot
fastboot getvar slot-unbootable:a
fastboot getvar slot-successful:a
fastboot getvar slot-unbootable:b
```

Expected output:
```
current-slot: a
slot-unbootable:a: no
slot-successful:a: yes
slot-unbootable:b: yes
```

If any of the above differ, **stop and report** before proceeding.

---

## Execution Sequence

Execute these commands in order, one at a time:

### Step 1 — Flash boot image to Slot B

```sh
fastboot flash boot_b build/out/boot.img
```

### Step 2 — Set Slot B active

```sh
fastboot set_active b
```

### Step 3 — Reboot

```sh
fastboot reboot
```

### Step 4 — Wait and observe

- Wait up to **90 seconds** for the USB ACM interface to appear (`/dev/cu.usbmodemZETHRA0000011`).
- If ACM appears, capture the following three commands only:

```sh
cat /proc/cmdline
ps
dmesg | tail -n 80
```

---

## Success Criterion

The experiment is **successful** if dmesg contains **any** of the following:

1. `orisetech,otm1911a: panel probed` (clean probe)
2. `orisetech,otm1911a` followed by a specific error (e.g., `failed to enable supply`, `gpio request failed`, `DSI read failed`) — this is still a **success for this experiment** because it shows the driver was reached and produced a specific actionable error
3. `/proc/cmdline` reports `androidboot.slot_suffix=_b` and `zethrad` is PID 1

Any of the above means we have a working Slot B userspace and a driver probe attempt. The next experiment would then address the specific error returned.

---

## Failure Criterion

The experiment **fails** if:

- No ACM interface appears within 90 seconds of reboot
- The device returns to fastboot mode without ACM appearing
- `dmesg` contains a kernel panic before userspace is reached

---

## Rollback Criterion

Rollback is **mandatory** immediately upon failure:

```sh
fastboot set_active a
fastboot reboot
```

Rollback is **confirmed successful only** after:
- ACM appears
- `/proc/cmdline` reports `androidboot.slot_suffix=_a`
- `ps` shows `/sbin/zethrad` as PID 1

This is the identical state to the v0.4.0 control baseline.

---

## Post-Experiment Logging

Regardless of outcome, capture the full ACM log using:

```sh
python3 brain/scratch/send_command.py "dmesg" > build/out/acm_logs/slot-b-exp-b01-dmesg-$(date +%Y%m%dT%H%M%S%z).log
```

And document findings as a section appended to `docs/NOKIA61PLUS_STOCK_DTB_HARDWARE_REFERENCE.md`.

---

## Required Approval Gate

Before executing Step 1, the approver must state:

```
Approved: run Slot B Experiment B-01. Flash boot.img to Slot B only. Do not modify Slot A.
```
