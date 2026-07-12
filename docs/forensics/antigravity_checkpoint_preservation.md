# Antigravity Checkpoint Campaign Preservation Record

**Preserved on**: 2026-06-21 (Asia/Kolkata)
**Repository branch**: `feature/phase2-track-a-gpu`
**Repository HEAD**: `56cdd3f`
**Kernel baseline**: Linux 7.1.0 (`Baby Opossum Posse`)
**Status**: Forensic evidence only; not production-ready patches

## Purpose

This record preserves the ignored `linux-7.1/` source modifications used during
the Nokia 6.1 Plus blind DRM/checkpoint investigation. The extracted kernel tree
is excluded by `.gitignore` and is not a Git repository, so its state could not
be reconstructed from normal project history.

The preservation was completed before cleaning, rebuilding, or flashing.

## Reference Source

Official source archive:

- URL: `https://cdn.kernel.org/pub/linux/kernel/v7.x/linux-7.1.tar.xz`
- SHA-256: `691f44797fbe790dc8a321604c927087526ad27b6d649925d60f8eed0a2564a0`

The checkpoint patches are based directly on this official Linux 7.1 source.
The panel mock delta is based on official Linux 7.1 plus the tracked
`kernel/patches/0002-add-otm1911a-panel-driver.patch`.

## Preserved Patch Series

| Patch | Contents | Intended use |
| --- | --- | --- |
| `9001-antigravity-legacy-bringup-exact.patch` | Exact ignored changes in gpiolib, Qualcomm hwspinlock, and Qualcomm pinctrl | Historical preservation of earlier bring-up instrumentation |
| `9002-antigravity-msm-dpu-checkpoints-00-17.patch` | MSM bind/probe/KMS and DPU hardware-init checkpoints | Historical checkpoint reconstruction |
| `9003-antigravity-drm-registration-checkpoints-20-28.patch` | DRM device registration checkpoints | Historical checkpoint reconstruction |
| `9004-antigravity-drm-fbdev-checkpoints-30-43.patch` | DRM client, fbdev client, framebuffer helper checkpoints | Historical checkpoint reconstruction |
| `9005-antigravity-panel-mock-delta.patch` | `mock_attach` and additional panel debug behavior beyond the tracked panel patch | Historical panel experiment reconstruction |

These patches intentionally live under `kernel/patches/forensics/`. Normal build
scripts must not apply them automatically.

## Experiment Integrity Findings

1. The tracked repository patch set, ignored `linux-7.1/` tree, and generated
   `boot.img` had diverged.
2. The latest decoded `boot.img` command line contains neither
   `msm.debug_checkpoint=43` nor
   `panel_orisetech_otm1911a.mock_attach=1`.
3. The saved `.boot-image-params.txt` describes an older image with checkpoint
   14 and does not match the latest `boot.img` hash or command line.
4. No per-run checkpoint ledger or raw timing results were preserved.
5. The timing script classifies any cycle over 20 seconds as checkpoint reach,
   although the inserted stall is 10 seconds and total timing includes bootloader,
   panic/watchdog, and USB re-enumeration latency.
6. The tracked `0001-zethra-nokia-sdm636-bringup.patch` does not apply cleanly
   to official Linux 7.1. Multiple gpiolib and pinctrl hunks fail.

## What Checkpoint 43 Would Mean

If a correctly identified image with `msm.debug_checkpoint=43` produced the
expected deliberate delay, it would show that:

- `drm_fb_helper_single_fb_probe()` returned successfully;
- `drm_setup_crtcs_fb()` returned successfully; and
- `register_framebuffer()`, including initial fbcon registration, returned.

It would not prove successful panel modesetting, DSI communication, asynchronous
DRM stability, userspace boot, or watchdog stability.

## Evidence Classification

| Claim | Classification |
| --- | --- |
| Checkpoint source locations 0-43 | Preserved from ignored source tree |
| Exact ignored source hashes | Preserved in `antigravity_source_hashes.sha256` |
| Latest local artifact hashes | Preserved in `antigravity_artifact_hashes.sha256` |
| Latest boot-image checkpoint selection | Confirmed absent from decoded command line |
| Historical checkpoint 30-43 timing outcomes | Unverifiable without original run logs and exact image hashes |
| Root cause of the physical reboot | Unresolved |

## Required Next Step

Do not resume linear checkpoint testing. First construct immutable control images
with a build identity embedded in the artifact and a generated run manifest:

1. `v0.3.0` Phase 1 control.
2. Linux 7.1 headless control.
3. Linux 7.1 DRM with fbdev emulation disabled.
4. Linux 7.1 panel mock with fbdev disabled.
5. Linux 7.1 fbdev without fbcon takeover.
6. Linux 7.1 fbcon enabled.

The Nokia device is not required until those images and manifests are prepared.
