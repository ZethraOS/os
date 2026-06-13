# ZethraOS Boot Diagnostics Tasks — Complete

- [x] **Instrument drivers and trace early boot hang**: Added tracepoints in `pinctrl-msm` and `qcom_hwspinlock`.
- [x] **Secure TLMM Bypass**: Resolved early boot hang at GPIO 8 by falling back to parent-subsystem ranges in gpiolib.
- [x] **Diagnose SDHCI storage deferrals**: Traced to interconnect clock dependencies.
- [x] **Enable SDM660 interconnect and MMCC/GPUCC clock controllers**: Enabled them in defconfig.
- [x] **Diagnose power-controller (`rpmpd`) probe failure**: Traced to missing `CONFIG_PM_OPP`.
- [x] **Enable `CONFIG_PM_GENERIC_DOMAINS` and select `PM_OPP`**: Enabled both to allow RPM power domains to load.
- [x] **Verify eMMC block devices detection**: Confirmed `mmcblk1` is successfully detected and partitioned.
- [x] **Update diagnostic init script**: Configured the script to mount `/dev/mmcblk1p73` (persist partition).
- [x] **Verify boot log persistence**: Verified boot log is successfully written to `/persist/zethra_boot.log`.
- [x] **Extract full boot log**: Successfully retrieved log via TWRP recovery.
