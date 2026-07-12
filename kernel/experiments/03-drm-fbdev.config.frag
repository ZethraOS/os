# Experiment 03: drm-fbdev
# Purpose: Full DRM + DSI + fbdev emulation active, but fbcon does NOT take over
#           the console. Tests whether fbdev emulation itself causes the crash,
#           or whether the crash only happens when fbcon grabs the display.
# Expected result: If this boots (120s timeout) but Image 04 crashes →
#                  fbcon console_lock contention is the crash culprit.

# Full DRM + DSI stack
CONFIG_DRM=y
CONFIG_DRM_MSM=y
CONFIG_DRM_MSM_DSI=y
CONFIG_DRM_MSM_DSI_14NM_PHY=y
CONFIG_DRM_FBDEV_EMULATION=y

# Framebuffer console OFF — fbdev device /dev/fb0 exists but fbcon does not attach
CONFIG_FRAMEBUFFER_CONSOLE=n
CONFIG_FRAMEBUFFER_CONSOLE_DETECT_PRIMARY=n

# Keep LOGO off to prevent early console takeover attempt
CONFIG_LOGO=n
CONFIG_FB_SIMPLE=n
