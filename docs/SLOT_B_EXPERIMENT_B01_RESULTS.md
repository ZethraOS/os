# Slot B Experiment B-01 — Results

**Date executed:** 2026-07-15  
**Operator:** ZethraOS bring-up session  
**Status:** COMPLETE — PARTIAL FAILURE (diagnosis obtained)  
**Rollback:** ✅ CONFIRMED — Slot A active, `slot_suffix=_a`, `zethrad` PID 1

---

## Execution Log

| Step | Action | Result |
|---|---|---|
| Pre-flight | `fastboot getvar` slot state | ✅ Matches v0.4.0 baseline exactly |
| Step 1 | `fastboot flash boot_b` | ✅ 30940 KB in 0.988s |
| Step 2 | `fastboot set_active b` | ✅ |
| Step 3 | `fastboot reboot` | ✅ |
| Wait | ACM poll (18 × 5s) | ✅ ACM appeared at ~85s (attempt 17) |
| Obs 1 | `/proc/cmdline` slot check | ⚠️ Reports `_a` — ABL hardcodes slot in cmdline; **`slot_suffix` in cmdline is NOT the running slot** (ABL artefact) |
| Obs 2 | `ps` | ✅ `/sbin/zethrad` is PID 1, full ZethraOS userspace running |
| Obs 3 | `dmesg` panel probe | ❌ No `msm_drm` driver registered. No panel probe. |
| Rollback | `fastboot set_active a && reboot` | ✅ ACM returned immediately (~5s), `slot_suffix=_a` confirmed |

---

## Experiment Outcome vs. Success Criterion

| Criterion | Expected | Observed | Met? |
|---|---|---|---|
| `orisetech,otm1911a` probe trace in dmesg | Any probe attempt (even error) | **Not present** | ❌ |
| `slot_suffix=_b` in cmdline | Present | `_a` (ABL artefact) | N/A |
| `zethrad` PID 1, userspace alive | Present | ✅ Present | ✅ |

**Panel probe success criterion: NOT MET.**  
**Experiment did not cause any harm. Rollback was clean.**

---

## Root Cause Diagnosis

`msm_drm` is **absent from the running kernel's platform driver table** (`/sys/bus/platform/drivers/`) despite `CONFIG_DRM_MSM=y` being set in `zethra_defconfig`.

Evidence:

```
/sys/bus/platform/drivers/  → no msm_drm entry
/sys/module/                → no msm_drm, drm, dpu, or dsi entries
/sys/kernel/debug/device_component/ → does not exist
dmesg                       → no msm_drm/DPU probe line
```

The `c900000.display-subsystem` node appears in dmesg **only** as a target of clock/dependency resolution messages (during boot's OF dependency cycle resolution pass), but no driver ever binds to it.

### Why msm_drm Is Silently Absent

`CONFIG_DRM_MSM=y` compiles the driver into the kernel image. However, `msm_drm` uses the **component aggregator** framework — the driver's `.probe()` only fires after **all** sub-components (DPU, DSI, PHY) have individually probed and registered with the aggregator. If any one sub-component fails to probe (or is never reached due to a missing dependency), the aggregator never fires and `msm_drm` never appears in the driver list.

The dmesg contains:

```
mmcc-sdm660 c8c0000.clock-controller: sync_state() pending due to c900000.display-subsystem
```

This line means the MMCC clock controller is **still waiting** to mark its sync_state done because `c900000.display-subsystem` (the DRM master) has never completed its probe. The display subsystem never resolved its clock dependencies — the chain broke before DPU/DSI/PHY ever tried to probe.

### Most Likely Root Cause

The `adreno_gpu` node is **disabled** in our DTS (`status = "disabled"`) and `msm.separate_gpu_kms=1` is in the kernel command line. However, `DRM_MSM` with the component framework still requires the **MDP/DPU** path to work independently. The most likely missing piece is that `c901000.display-controller` (the DPU/MDP5) sub-component is silently failing to probe because of a missing or mis-configured intermediate dependency — most likely **iommu/smmu** or **power domain** — before the component framework can fire.

---

## Hypothesis for Next Experiment (B-02)

**Single variable to change:** Add verbose component framework and deferred probe debugging to the kernel command line — specifically `drm.debug=0x1ff dyndbg="module msm_drm +p"` — and re-flash Slot B with the same image plus this cmdline change, to get a complete deferred-probe trace showing exactly which sub-component is blocking the aggregator.

**Alternative (preferred):** Check if the DPU `c901000.display-controller` sub-component is registering at all using `/sys/kernel/debug/devices_deferred` on a boot with `CONFIG_PM_DEBUG=y` or `CONFIG_OF_DYNAMIC=y`.

---

## Files

| File | Description |
|---|---|
| `build/out/acm_logs/slot-b-exp-b01-dmesg-20260715T194113.log` | Full 367-line dmesg from Slot B run |
| `docs/NOKIA61PLUS_STOCK_DTB_HARDWARE_REFERENCE.md` | Hardware reference (frozen) |
| `docs/SLOT_B_EXPERIMENT_B01.md` | Original experiment plan |

---

*Slot A is confirmed restored to v0.4.0 baseline. Do not tag this result.*
