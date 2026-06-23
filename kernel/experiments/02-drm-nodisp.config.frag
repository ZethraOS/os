# Experiment 02: drm-nodisp
# Purpose: DRM core + Adreno GPU active, but DSI/panel/framebuffer disabled.
#           Confirms the MSM DRM core and GPU bind without crashing.
#           If this fails, the crash is in DRM core/GPU, not the display path.
# Expected result: 120s timeout, ACM console appears, dmesg shows msm probe OK.

# DRM core ON (GPU + KMS framework active)
CONFIG_DRM=y
CONFIG_DRM_MSM=y

# DSI controller and PHY OFF — no display hardware probed
CONFIG_DRM_MSM_DSI=n
CONFIG_DRM_MSM_DSI_14NM_PHY=n

# Framebuffer emulation OFF — no fbdev device created
CONFIG_DRM_FBDEV_EMULATION=n

# Framebuffer console OFF
CONFIG_FRAMEBUFFER_CONSOLE=n
CONFIG_FRAMEBUFFER_CONSOLE_DETECT_PRIMARY=n
CONFIG_FB_SIMPLE=n
CONFIG_LOGO=n
