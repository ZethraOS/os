# Slot B Experiment B-04 — Align Display Controller Architecture with MDP5 Hardware

**Date drafted:** 2026-07-24  
**Branch:** `feature/phase2-track-b-panel-bringup`  
**Status:** EXECUTING  
**Builds on:** B-01, B-02, B-03 diagnostics  
**Control baseline:** v0.4.0 (`4293bc4`) — Slot A frozen, untouchable.

---

## Technical Discovery & Root Cause

1. **Hardware Architecture Alignment**:
   - SDM630/SDM636/SDM660 SoCs in Linux 7.1 use the **MDP5** display controller architecture (`"qcom,sdm630-mdp5"`, `"qcom,mdp5"` in `sdm630.dtsi`), **NOT** DPU 3.2.
   - Attempting to probe `dpu1` (`dpu_kms.c`) on MDP5 hardware resulted in register layout mismatches and bus faults during early boot.

2. **Kconfig Resolution**:
   - `CONFIG_DRM_MSM_MDP5=y` is the accurate hardware driver flag for SDM636.
   - `CONFIG_DRM_MSM_MDP5=y` auto-selects `CONFIG_DRM_MSM_MDSS=y`, which enables `msm_mdss_register()` and registers the `"msm_mdp"` platform driver for `"qcom,mdp5"`.

3. **AVB / Slot Management**:
   - Both `vbmeta_b` (`build/out/vbmeta.img`) and `boot_b` (`build/out/boot.img`) must be flashed together to prevent ABL AVB verification fallback to Slot A.

---

## Single Variable Changes

1. **Defconfig**:
   Set `CONFIG_DRM_MSM_MDP5=y` in `kernel/zethra_defconfig`.
2. **Flash Command**:
   Flash both `vbmeta_b` and `boot_b` to ensure Slot B executes.

---

## Success Criterion

- `msm_drm` / `msm_mdp` platform driver registers and binds to `display-controller@c901000`.
- System boots into Slot B with ACM serial interface available (`/dev/cu.usbmodemZETHRA0000011`).

---

## Rollback Criterion

```sh
fastboot set_active a && fastboot reboot
```
Confirmed when: ACM appears, `slot_suffix=_a`, `zethrad` is PID 1.
