# Phase 2 Implementation Plan: Display, GPU, and Graphics Shell Bring-Up

**Target Hardware**: Nokia 6.1 Plus (DRG / TA-1103) / SDM636  
**Status**: Draft / Proposed  
**Output Path**: `docs/task-phases/phase_2_implementation_plan.md`

---

## 1. Goal Description

The goal of Phase 2 is to bring up the graphical display system and GPU hardware acceleration on the Nokia 6.1 Plus. This will allow ZethraOS to transition from a headless, text-only serial debug mode to rendering a fully hardware-accelerated user interface shell directly on the device's physical LCD screen.

### Success Criteria:
1. **MSM DRM Driver**: Probes successfully and exposes `/dev/dri/card0` and `/dev/dri/renderD128`.
2. **Display Panel Probe**: Probes and initializes the `OTM1911A` display controller, establishing DSI panel communications.
3. **LCD Backlight Controls**: Exposes backlight brightness controls via sysfs `/sys/class/backlight/`.
4. **Framebuffer Console (`fbcon`)**: Redirects kernel console boot logging directly onto the physical screen.
5. **GPU Hardware Acceleration**: Probes the Adreno 509 GPU, initializes graphics clock controllers (GPUCC), and binds power domains.
6. **Compositor Rendering**: Spawns `zethra-compositor` utilizing the DRM/KMS device node to render Wayland surfaces on the display.

---

## 2. Technical Breakdown

### A. Kernel Driver Enablements (`zethra_defconfig`)
We will configure the Qualcomm MSM DRM graphics subsystem and framebuffers by enabling the following driver trees:

```ini
# Direct Rendering Manager (DRM)
CONFIG_DRM=y
CONFIG_DRM_MSM=y
CONFIG_DRM_MSM_DSI=y
CONFIG_DRM_MSM_ADRENO=y
CONFIG_DRM_MSM_REGISTER_LOGGING=y

# Panel Selection
CONFIG_DRM_PANEL_ORISETECH_OTM1911A=y

# Framebuffer & Console Support
CONFIG_FRAMEBUFFER_CONSOLE=y
CONFIG_FRAMEBUFFER_CONSOLE_DETECT_PRIMARY=y
CONFIG_FB_SIMPLE=y
CONFIG_LOGO=y
CONFIG_FB=y

# GPU & Clock controllers
CONFIG_DRM_MSM_GPU_STATE=y
CONFIG_QCOM_CLK_RPM=y
CONFIG_QCOM_CLK_RPMH=y
```

### B. Device Tree (DTS) Mapping
We must specify the display panel routing, regulator dependencies, and clock constraints in `sdm636-nokia-frt.dts`:

> [!IMPORTANT]
> Downstream CAF/vendor DTS trees for Nokia DRG must be cross-referenced to verify exact regulator labels and GPIO polarities:
> - **Regulator Labels**: `pm660_l1` / `pm660_l2` names must exactly match PM660 PMIC node regulator labels to prevent deferred probe loops.
> - **Reset GPIO Polarity**: `GPIO_ACTIVE_LOW` vs `GPIO_ACTIVE_HIGH` on `tlmm 82` must be verified against vendor DTS configuration.

1. **Backlight Node**: Link the PWM/WLED driver on the PM660 PMIC to the display panel:
   ```dts
   backlight: backlight {
       compatible = "pwm-backlight";
       pwms = <&pm660_pwm 0 5000000>;
       brightness-levels = <0 4 8 16 32 64 128 255>;
       default-brightness-level = <6>;
   };
   ```
2. **Display Panel Node (DSI)**: Declare the Orisetech OTM1911A DSI panel inside the Mobile Display Subsystem (MDSS) node:
   ```dts
   &dsi0 {
       status = "okay";
       panel@0 {
           compatible = "orisetech,otm1911a";
           reg = <0>;
           vdda-supply = <&pm660_l1>;
           vddi-supply = <&pm660_l2>;
           reset-gpios = <&tlmm 82 GPIO_ACTIVE_LOW>;
           backlight = <&backlight>;
       };
   };
   ```

### C. GPU, IOMMU, & Power Domain Binding
- Ensure the graphics clock controller (`CONFIG_COMMON_CLK_QCOM`) is correctly mapped.
- **IOMMU Binding**: GPU memory management requires `CONFIG_QCOM_IOMMU` or `CONFIG_ARM_SMMU` on the SDM636 platform. Without valid IOMMU mappings, command submissions will fault, triggering a watchdog reset.
- Bind the GPU power domain controls (`PM_OPP` and `PM_GENERIC_DOMAINS`) to prevent device hangs or watchdog resets when the GPU powers up.

---

## 3. Phased Rollout Gates

To manage risk and enforce reproducibility, Phase 2 is structured into four sequential verification gates:

### Gate 2.0: DRM & GPU Compilation
* **Target**: Compile-time check.
* **Verification**: Verify that the Linux kernel compiles cleanly with `CONFIG_DRM_MSM` and `CONFIG_DRM_MSM_DSI` enabled. Watch for header conflicts in `msm_drv.h` on linux-6.9.

### Gate 2.1: Panel Discovery & Probing
* **Target**: Panel driver initialization check.
* **Verification**: Boot the kernel on the device and monitor logs (`dmesg` via the ACM serial console) to ensure the DSI controller detects and registers the display panel:
  ```
  [drm] Panel orisetech,otm1911a registered successfully
  ```
  *Risks*: Watch for regulator name mismatches and reset GPIO polarity deferred probe hangs.

### Gate 2.2: LCD Console Output (`fbcon` / `simplefb`)
* **Target**: Graphics console validation.
* **Verification**: Verify that kernel logs and boot splash redirect directly onto the phone's LCD screen. Ensure `CONFIG_FB_SIMPLE` is in place as a fallback if DRM fbdev emulation is not initialized early enough. Verify sysfs backlight nodes (`/sys/class/backlight/`) are responsive to brightness writes.

### Gate 2.3: Compositor Wayland Rendering
* **Target**: Wayland GUI Shell rendering.
* **Verification**: Launch `zethra-compositor`. Confirm that the compositor opens `/dev/dri/card0`, initiates Wayland protocols, and renders the user interface launcher onto the display.
  *Risks*: Handle `ENODEV` errors gracefully if `renderD128` isn't ready. Diagnose DMA-BUF import or format modifier mismatches.

### Gate 2.4: Stress & Stability Verification
* **Target**: Sustained GPU load test.
* **Verification**: Execute a 10-minute GPU benchmark burn-in test (e.g. `glmark2-es2-drm`) to confirm the watchdog and power domain bindings hold under sustained load.

---

## 4. Resource & Budget Estimates

### A. Timeline Estimates
* **Gate 2.0 (Compilation)**: 1.0 – 1.5 Hours
* **Gate 2.1 (Panel Discovery)**: 2.5 – 3.5 Hours
* **Gate 2.2 (Console Output)**: 1.0 – 1.5 Hours
* **Gate 2.3 (Compositor Rendering)**: 2.5 – 3.5 Hours (adjusted for Wayland/DMA-BUF debugging)
* **Gate 2.4 (Stress / Stability)**: 1.0 Hour (benchmarking and configuration)
* **Total Timeline**: **8.0 – 11.0 Hours**

### B. AI Token Estimates
* **Estimated Conversation Turns**: 15 – 25 turns.
* **Estimated Input Tokens**: 2.0M – 3.2M tokens.
* **Estimated Output Tokens**: 60k – 90k tokens.
* **Token Saving Strategy**: Offload deep file searches of the kernel source tree (`linux-6.9/`) to the isolated `research` subagent to keep the main developer context light.
