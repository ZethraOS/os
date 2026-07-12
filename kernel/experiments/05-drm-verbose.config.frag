# Experiment 05: drm-verbose
# Purpose: Same as Image 04 (full stack) but with DRM verbose logging active.
#           Captures a pstore/ramoops console dump of the crash trace for analysis.
#           Run ONLY after Image 04 confirms the crash still occurs.
#
# HOW IT WORKS (F-21 correction):
#   drm.debug=0x3f is passed via BOOT_EXTRA_CMDLINE (not Kconfig).
#   This sets the DRM debug bitmask: KMS|PRIME|RM|DRIVER|ATOMIC|VBL = all planes.
#   pr_debug() calls in the DRM core emit to the kernel ring buffer.
#   CONFIG_PSTORE_CONSOLE=y (already in base defconfig) captures the ring buffer
#   to ramoops on panic. After the crash, boot into the known-good slot A image
#   and read /sys/fs/pstore/console-ramoops-0 for the pre-crash DRM trace.
#
#   WARNING: drm.debug=0x3f is extremely verbose. The 256KB ramoops console buffer
#   may fill before the crash. If the trace is truncated, reduce to drm.debug=0x04
#   (KMS only) on the next run.
#
# BOOT_EXTRA_CMDLINE must be set when calling run_experiment.sh:
#   EXPERIMENT_NAME=drm-verbose BOOT_EXTRA_CMDLINE="drm.debug=0x3f" bash run_experiment.sh

# Full DRM + DSI stack (same as Image 04)
CONFIG_DRM=y
CONFIG_DRM_MSM=y
CONFIG_DRM_MSM_DSI=y
CONFIG_DRM_MSM_DSI_14NM_PHY=y
CONFIG_DRM_FBDEV_EMULATION=y
CONFIG_FRAMEBUFFER_CONSOLE=y
CONFIG_FRAMEBUFFER_CONSOLE_DETECT_PRIMARY=y
CONFIG_FB_SIMPLE=y
CONFIG_LOGO=y

# Enable GPU state capture for coredump analysis
CONFIG_DRM_MSM_GPU_STATE=y
