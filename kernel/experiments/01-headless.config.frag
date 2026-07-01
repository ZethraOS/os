# Experiment 01: headless
# Purpose: Verify Linux 7.1 boots cleanly with zero display stack.
#           This is the control image. If this fails, the problem is in Linux 7.1
#           itself (unrelated to DRM/display) and all other experiments are invalid.
# Expected result: 120s timeout (device stays up), ACM console appears.
#
# F-06 FIX: Disable FB_SIMPLE so the simple-framebuffer DTS node does not probe
#           and attempt to mmap an uninitialized framebuffer region.
# F-17 FIX: Disable QCOM_LLCC to avoid deferred-probe warnings polluting dmesg.

# Disable entire DRM/GPU/display subsystem
CONFIG_DRM=n
CONFIG_DRM_MSM=n
CONFIG_DRM_MSM_DSI=n
CONFIG_DRM_MSM_DSI_14NM_PHY=n
CONFIG_DRM_MSM_DSI_10NM_PHY=n
CONFIG_DRM_FBDEV_EMULATION=n
CONFIG_DRM_MSM_GPU_STATE=n

# Disable framebuffer console and all FB drivers
CONFIG_FRAMEBUFFER_CONSOLE=n
CONFIG_FRAMEBUFFER_CONSOLE_DETECT_PRIMARY=n
CONFIG_FB=n
CONFIG_FB_SIMPLE=n
CONFIG_FB_DEVICE=n
CONFIG_LOGO=n

# Disable backlight (requires display hardware)
CONFIG_BACKLIGHT_QCOM_WLED=n

# Disable LLCC — avoid deferred-probe warnings in headless dmesg
CONFIG_QCOM_LLCC=n
