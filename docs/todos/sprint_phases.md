# ZethraOS Nokia 6.1 Plus Bring-Up Sprint Phases

This document details the tasks and roadmap for bringing up ZethraOS on the Nokia 6.1 Plus (SDM636) device, divided into structured sprint phases.

---

## Phase 1: Core Kernel Boot & Storage (Completed)
- [x] **Secure TLMM Bypass**: Resolved early boot hang at GPIO 8 by falling back to parent-subsystem ranges in gpiolib.
- [x] **Interconnect Drivers**: Enabled SDM660 interconnects and MMCC/GPUCC clock controllers in defconfig.
- [x] **Power Domain Framework**: Enabled `CONFIG_PM_GENERIC_DOMAINS` and matched dependencies to select `PM_OPP`, allowing the power domains to probe successfully.
- [x] **eMMC Storage Detection**: Configured and successfully probed `sdhci` storage to expose `/dev/mmcblk1`.
- [x] **Persistence Logging**: Mounted persist partition `/dev/mmcblk1p73` to save kernel boot dmesg directly on physical storage.
- [x] **Initramfs Hand-off**: Reached `/init` and launched `zethrad` as PID 1.
- [x] **Recovery Console**: Verified an interactive USB CDC ACM root shell.

Canonical evidence status:
[Phase 1 Verification Matrix](../task-phases/phase_1_verification_matrix.md).

---

## Phase 2: Display & Graphics (Next Sprint)
Configure graphics hardware to render the user interface.
- [ ] **MSM DRM Driver**: Validate the enabled `CONFIG_DRM_MSM` stack on Nokia hardware.
- [ ] **Display Panel Driver**: Implement and review OTM1911A support; Linux 6.9 has no matching upstream panel driver.
- [ ] **Adreno 509 GPU**: Initialize graphics clock controller and power domains.
- [ ] **Console Mapping**: Enable DRM/KMS framebuffer console (`fbcon`) to print boot logs directly on the LCD panel.
- [ ] **Surface Rendering**: Verify GUI shell or raw framebuffer test rendering.

---

## Phase 3: Root Filesystem & OS Boot
Move from minimal initramfs to booting the full operating system.
- [ ] **Boot CMDLINE Configuration**: Update boot parameters to mount ZethraOS rootfs from eMMC partition (typically `/dev/mmcblk1p82`).
- [x] **Init Manager**: Integrated and successfully booted `zethrad` as PID 1 supervising core daemons (sensord, networkd, otad).
- [ ] **Persistent Storage**: Mount user-space partitions (`/data`) using EXT4/F2FS.
- [ ] **Per-Service Sandboxing**: Enable cgroups v2 and custom seccomp profiles for process isolation.

---

## Phase 4: USB Gadget & Power Management
- [x] **USB ConfigFS**: Configured USB CDC ACM serial gadget console interface (`/dev/ttyGS0`) for emergency host debug connectivity.
- [ ] **Power Supply**: Enable battery charging controller driver (`smb1351` / `pm660` charger) and verify power reporting in `/sys/class/power_supply`.
- [ ] **USB OTG Host**: Enable USB role switching for external peripherals.

---

## Phase 5: Peripherals & Connectivity
- [ ] **Touchscreen**: Enable I2C input driver for the touchscreen and verify `/dev/input/event*`.
- [ ] **Wi-Fi**: Enable `wcn36xx` driver and configure wireless interface.
- [ ] **Bluetooth**: Configure Bluetooth UART interfaces.
- [ ] **Audio**: Set up the WCD9335 audio codec and verify ALSA outputs.
