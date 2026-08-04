# Slot B Experiment B-02 — Remove msm.separate_gpu_kms=1 from Kernel Cmdline

**Date drafted:** 2026-07-16  
**Branch:** `feature/phase2-track-b-panel-bringup`  
**Status:** AWAITING APPROVAL  
**Builds on:** B-01 results + B-02 diagnostic session  
**Control baseline:** v0.4.0 (`4293bc4`) — Slot A frozen, untouchable.

---

## Root Cause (from B-01 / B-02 diagnostic)

**`msm.separate_gpu_kms=1` in the kernel command line is suppressing the KMS display driver.**

Evidence chain:
1. `CONFIG_DRM_MSM=y` correctly resolves in Kconfig (verified via Docker dry-run)
2. `msm_drm` is **not** registered in `/sys/bus/platform/drivers/` at runtime
3. `c900000.display-subsystem` exists in sysfs with `waiting_for_supplier=0` (all suppliers satisfied) but has no `driver` symlink — the driver was never offered
4. `adreno_gpu` is `status = "disabled"` in DTS
5. `msm.separate_gpu_kms=1` is present in the kernel cmdline (inherited from the v0.4.0 GPU Track A experiment)

**Why this breaks KMS:** In Linux 7.1's `msm_drm`, when `separate_gpu_kms=1` is set, the driver bifurcates: it registers the GPU `platform_driver` and the KMS `platform_driver` as two independent entries. With `adreno_gpu` disabled in DT, the GPU `platform_driver` never probes, but the flag *also* gates the KMS registration path — the KMS driver only registers **after** verifying the separate GPU path is active. Without a live GPU, neither registers.

**Fix:** Remove `msm.separate_gpu_kms=1` from the kernel cmdline. This restores unified KMS mode where `msm_drm` registers a single combined driver that handles display without requiring a separate GPU path.

---

## Single Variable

**One and only one change from the B-01 image:**

> Rebuild `boot.img` with the kernel cmdline parameter **`msm.separate_gpu_kms=1` removed**. All other build inputs are identical (same kernel source hash `57ce9a0`, same defconfig SHA `ecbbec31`, same DTS SHA).

New cmdline will be:
```
earlycon=msm_serial_dm,0xc170000 console=ttyMSM0,115200,n8 androidboot.hardware=qcom lpm_levels.sleep_disabled=1 loop.max_part=7 buildvariant=userdebug panic=10 root=/dev/dm-0 ...
```

(All other cmdline parameters preserved.)

---

## What Is Not Changed

- No kernel source changes
- No DTS changes
- No defconfig changes
- No driver code changes
- No Slot A writes

---

## Success Criterion

dmesg on Slot B shows **any** of the following:
1. `msm_drm` appears in `/sys/bus/platform/drivers/` ← primary criterion
2. `c900000.display-subsystem` has a `driver` symlink in sysfs
3. `orisetech,otm1911a` appears in dmesg (probe attempt, success or error)

Any one of the above = B-02 success. The next experiment then addresses the specific next error.

---

## Failure Criterion

- `msm_drm` is still absent from `/sys/bus/platform/drivers/` after boot
- No ACM within 90 seconds (kernel panic)

---

## Rollback Criterion

```sh
fastboot set_active a && fastboot reboot
```
Confirmed when: ACM appears, `slot_suffix=_a`, `zethrad` is PID 1.

---

## Build Step (pre-approval)

The only build change is the cmdline. This is set in `build/scripts/build_kernel.sh` or via a config fragment passed to `build_kernel.sh`. Verify the exact mechanism before building.

---

## Required Approval Gate

Before executing:
```
Approved: run Slot B Experiment B-02. Remove msm.separate_gpu_kms=1 from cmdline, rebuild boot.img, flash to Slot B only. Do not modify Slot A.
```
