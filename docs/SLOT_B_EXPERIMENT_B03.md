# Slot B Experiment B-03 — Enable DPU backend + fix MDSS/DPU compatible strings

**Date drafted:** 2026-07-24  
**Branch:** `feature/phase2-track-b-panel-bringup`  
**Status:** EXECUTING  
**Builds on:** B-01, B-02 diagnostics  
**Control baseline:** v0.4.0 (`4293bc4`) — Slot A frozen, untouchable.

---

## Root Cause Chain (fully resolved)

### Why msm_drm_register() did nothing:

1. `msm_drm_register()` calls `msm_mdss_register()` ✓
2. But `msm_mdss_register()` compiles to an **empty stub** when `CONFIG_DRM_MSM_MDSS=n`
3. `DRM_MSM_MDSS` has `default n` in Kconfig — it's NOT auto-selected by `DRM_MSM`
4. It IS auto-selected by `DRM_MSM_DPU` or `DRM_MSM_MDP5`
5. Our defconfig had NEITHER `CONFIG_DRM_MSM_DPU=y` NOR `CONFIG_DRM_MSM_MDP5=y`

### Why the correct backend is DPU (not MDP5):

- SDM636 = SDM660 lite = MSM8998 lineage → uses **DPU 3.2** hardware
- Confirmed: `linux-7.1/drivers/gpu/drm/msm/disp/dpu1/dpu_kms.c` has `qcom,sdm660-mdp5` → `dpu_sdm660_cfg`
- Confirmed: `linux-7.1/arch/arm64/boot/dts/qcom/sdm660.dtsi` uses `&mdp` with `compatible = "qcom,sdm660-mdp5", "qcom,mdp5"` — DPU driver, not MDP5 driver

### Wrong compatible strings in our DTS:

| Node | Our DTS | Correct |
|------|---------|---------|
| display-subsystem | `qcom,mdss` | `qcom,msm8998-mdss` |
| display-controller | (correct name, wrong compatible TBD) | `qcom,sdm660-mdp5` |
| dsi-ctrl | `qcom,sdm660-dsi-ctrl` | `qcom,sdm660-dsi-ctrl`, `qcom,mdss-dsi-ctrl` |

---

## Two Changes (this experiment)

### Change 1 — defconfig: add CONFIG_DRM_MSM_DPU=y

```diff
+CONFIG_DRM_MSM_DPU=y
```

This auto-selects `CONFIG_DRM_MSM_MDSS=y`, which is the missing link.

### Change 2 — DTS: fix display-subsystem compatible

In `kernel/dts/sdm636-nokia-frt.dts`, the `&mdss` node override must set:
```dts
compatible = "qcom,msm8998-mdss";
```

The upstream `sdm660.dtsi` inherits from `msm8998.dtsi` where `mdss` is defined with `compatible = "qcom,msm8998-mdss"`. Our DTS was leaving it as `qcom,mdss` (inherited from somewhere else or explicitly set wrong).

---

## Success Criterion

Any of the following in dmesg after boot:
1. `msm-mdss` appears in `/sys/bus/platform/drivers/`  ← primary
2. `c900000.display-subsystem` has a `driver` symlink in sysfs
3. `msm_drm` or `dpu_kms` appears in dmesg

---

## Failure Criterion

- `msm-mdss` still absent from `/sys/bus/platform/drivers/`
- Kernel panic (no ACM within 90s)

---

## Rollback

```sh
fastboot set_active a && fastboot reboot
```

---

## Verification after rollback

Confirmed when: `slot_suffix=_a` in cmdline, `zethrad` is PID 1.
