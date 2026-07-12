# Experiment 06: fbdev-lockless
# Purpose: Same as Image 04 (full stack) but passes fb.lockless_register_fb=1
#           via BOOT_EXTRA_CMDLINE. Tests whether the crash is caused by a
#           console_lock deadlock between fbcon and DRM's fb_helper_hotplug_event.
#
# HYPOTHESIS: The fbcon registration path holds console_lock while the DRM
#   component bind is still completing. If the GPU or KMS sub-driver also tries
#   to acquire console_lock (e.g., for a mode-set), a deadlock occurs and the
#   watchdog fires after ~10s causing the visible reboot loop.
#
# BOOT_EXTRA_CMDLINE must be set:
#   EXPERIMENT_NAME=fbdev-lockless BOOT_EXTRA_CMDLINE="fb.lockless_register_fb=1" bash run_experiment.sh
#
# INTERPRETATION:
#   Image 06 BOOTS + Image 04 CRASHES → console_lock deadlock confirmed as root cause.
#   Image 06 CRASHES → deadlock is not the issue; look elsewhere (PHY init, IOMMU).

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
