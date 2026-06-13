# ZethraOS Debugging & Reproducibility Walkthrough

This document summarizes the changes, diagnoses, and verification steps that enabled a successful boot of the `linux-6.9` kernel on the **Nokia 6.1 Plus (SDM636)** and achieved 100% reproducible builds for ZethraOS.

---

## 1. Summary of Changes

To achieve a clean boot and determinism, we resolved boot hang issues and isolated all build inputs to guarantee identical binary outputs.

### 1.1 Early Boot Hang (TLMM / GPIO Subsystem)
* **Problem**: The Qualcomm TLMM pinctrl driver attempted to query the direction of reserved GPIO 8 (restricted by security firmware), triggering a secure bus hang.
* **Fix**: Patched [gpiolib.c](file:///Users/nomad/workstation/work/code/OS/Mobile/zethraos/linux-6.9/drivers/gpio/gpiolib.c) to fallback to `gc->parent` when parsing the device-tree for `gpio-reserved-ranges`, marking GPIOs 8-11 as invalid/reserved.

### 1.2 Storage Controller Deferral (Interconnects & Clock Controllers)
* **Problem**: The eMMC controller `c0c4000.sdhci` deferred probe because it lacked MMCC clocks.
* **Fix**: Enabled MMCC/GPUCC clock controllers and interconnect drivers in `kernel/zethra_defconfig`.

### 1.3 Power Domain Probe Failure (OPP Framework)
* **Problem**: Power domain driver `qcom-rpmpd` failed to probe with error `-95` (`-EOPNOTSUPP`) because the OPP framework was disabled.
* **Fix**: Enabled generic power domains and patched `drivers/pmdomain/qcom/Kconfig` to force-select the OPP framework.

### 1.4 Reproducible Builds Implementation (Gate 0)
* **Problem**: Pristine kernel source directory size exceeded 10,000+ files and had non-deterministic build artifacts due to dynamic timestamps, filesystem metadata, cpio archive generation, and AVB signing salts.
* **Fix**:
  * **Patch Isolation**: All kernel source changes were extracted to a single clean patch: [0001-zethra-nokia-sdm636-bringup.patch](file:///Users/nomad/workstation/work/code/OS/Mobile/zethraos/kernel/patches/0001-zethra-nokia-sdm636-bringup.patch).
  * **Defconfig & DTS Isolation**: Defconfig was tracked at `kernel/zethra_defconfig`, and device tree source was tracked at [sdm636-nokia-frt.dts](file:///Users/nomad/workstation/work/code/OS/Mobile/zethraos/kernel/dts/sdm636-nokia-frt.dts).
  * **Build Determinism**:
    * Frozen build timestamp, host, and user metadata via Kbuild env variables (`KBUILD_BUILD_USER=zethra KBUILD_BUILD_HOST=zethra-build KBUILD_BUILD_TIMESTAMP="Fri Jun 12 17:00:00 UTC 2026"`).
    * Created `build_initramfs.sh` step to package `initramfs` inside a Docker container using GNU `cpio --reproducible --owner=0:0` and `gzip -n` to strip local file system metadata, UID/GID, and timestamps.
    * Extracted public AVB key [test_key.pem.pub](file:///Users/nomad/workstation/work/code/OS/Mobile/zethraos/tools/test_key.pem.pub) to fix signing failures.
    * Pinned a static cryptographic salt (`c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00`) for `avbtool add_hash_footer` in `build_kernel.sh` and `pack_boot_image.sh` to remove non-determinism from Android Verified Boot signing.

---

## 2. Verification & Validation Results

### 2.1 Reproducibility Check Results
The reproducibility script `verify_reproducibility.sh` ran two complete builds sequentially and verified that all produced output binaries are **bit-for-bit identical**:

* **Kernel Image (`Image.gz-dtb`)**: **MATCH**
  * SHA256: `1843a46431a78af0d05d4ce0804af9c08262ed291b0dcbdbb7b479d03e21de4c`
* **Initramfs (`initramfs.cpio.gz`)**: **MATCH**
  * SHA256: `d8d8666fbedef4e9899c4f67f6b7f4f51e5553a7ab1e23134ad7312b807c358f`
* **Signed Boot Image (`boot.img`)**: **MATCH**
  * SHA256: `4b8731e959c57e5db01d7fe80b4abed3956be6f37f07622543b0396611d2f158`

The reproducibility check successfully **PASSED** all criteria!

### 2.2 Boot Logs Analysis
With these changes in place, the kernel successfully completed driver probing and loaded our minimal `/init` script in the initramfs:
1. **Interconnects Probed successfully**
2. **eMMC Storage Controller initialized successfully**
3. **eMMC Card & Partitions populated (`mmcblk1`)**
4. **Persist partition successfully mounted** and boot log written to physical eMMC storage.

---

## 3. Retaining the Logs

The complete boot log was pulled via TWRP recovery from the device and is saved locally at:
* [zethra_boot.log](file:///Users/nomad/workstation/work/code/OS/Mobile/zethraos/build/out/zethra_boot.log)
