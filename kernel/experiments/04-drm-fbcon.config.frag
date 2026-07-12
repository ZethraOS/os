# Experiment 04: drm-fbcon  (FULL STACK — known-broken baseline)
# Purpose: Re-confirm the full DRM + DSI + fbdev + fbcon stack still crashes.
#           This is the configuration that previously caused the reboot loop.
#           Must be run AFTER Image 01 confirms Linux 7.1 itself is stable.
# Expected result: < 20s fastboot return (crash-reboot loop re-confirmed).
#                  If this BOOTS, we have already fixed the root cause elsewhere.
#
# Note: This is the SAME as zethra_defconfig base — no overrides needed.
# The fragment exists to be explicit and to generate a distinct EXPERIMENT_ID.

# Full DRM + DSI stack
CONFIG_DRM=y
CONFIG_DRM_MSM=y
CONFIG_DRM_MSM_DSI=y
CONFIG_DRM_MSM_DSI_14NM_PHY=y
CONFIG_DRM_FBDEV_EMULATION=y

# Framebuffer console ON — this is the configuration that crashes
CONFIG_FRAMEBUFFER_CONSOLE=y
CONFIG_FRAMEBUFFER_CONSOLE_DETECT_PRIMARY=y
CONFIG_FB_SIMPLE=y
CONFIG_LOGO=y
