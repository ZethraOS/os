# Adreno 509 GPU Firmware — Nokia 6.1 Plus (TA-1103 / SDM636)

## Files Required

| Filename        | Source                                        | Commit to repo? |
|-----------------|-----------------------------------------------|-----------------|
| a530_pm4.fw     | linux-firmware.git qcom/a530_pm4.fw           | ✅ Yes          |
| a530_pfp.fw     | linux-firmware.git qcom/a530_pfp.fw           | ✅ Yes          |
| a512_zap.mdt    | Stock Android /vendor/firmware/ (extract)     | ❌ No (gitignored) |
| a512_zap.b00    | Stock Android /vendor/firmware/ (extract)     | ❌ No (gitignored) |
| a512_zap.b01    | Stock Android /vendor/firmware/ (extract)     | ❌ No (gitignored) |
| a512_zap.b02    | Stock Android /vendor/firmware/ (extract)     | ❌ No (gitignored) |

## Why These Files

Linux 7.1 `adreno_device.c` (lines 188–204) defines the Adreno 509
(chip ID 0x05000900) entry. It requests:
- `a530_pm4.fw` / `a530_pfp.fw` — CP microcode, shared across all A5xx variants
  by design (A506/A508/A509/A510/A512/A530/A540 all use the same binaries).
- `a512_zap.mdt` — ZAP (Zero-Area Protection) shader, loaded via
  `qcom_mdt_load()` + `qcom_scm_pas_auth_and_reset()` to unlock the GPU from
  TrustZone secure mode. Without it, /dev/dri/renderD* will not work.

## Obtaining a530_pm4.fw and a530_pfp.fw (upstream, safe to distribute)

    git clone https://git.kernel.org/pub/scm/linux/kernel/git/firmware/linux-firmware.git
    cp linux-firmware/qcom/a530_pm4.fw kernel/firmware/qcom/
    cp linux-firmware/qcom/a530_pfp.fw kernel/firmware/qcom/

## Extracting a512_zap.mdt from Stock Android (one-time, from device)

    # 1. Boot device to stock Android (Slot A)
    fastboot set_active a && fastboot reboot
    # 2. Pull ZAP files
    adb root
    adb pull /vendor/firmware/a512_zap.mdt kernel/firmware/qcom/
    adb pull /vendor/firmware/a512_zap.b00  kernel/firmware/qcom/
    adb pull /vendor/firmware/a512_zap.b01  kernel/firmware/qcom/
    adb pull /vendor/firmware/a512_zap.b02  kernel/firmware/qcom/
    # 3. Record checksums
    sha256sum kernel/firmware/qcom/a512_zap.* > kernel/firmware/qcom/a512_zap.sha256
    # 4. Switch back to ZethraOS
    fastboot set_active b && fastboot reboot
