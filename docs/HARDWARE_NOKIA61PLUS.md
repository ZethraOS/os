# ZethraOS — Nokia 6.1 Plus Hardware Target

**Device**: Nokia 6.1 Plus (TA-1103)  
**Status**: Primary reference hardware target for ZethraOS v0.3.0

---

## Hardware Specifications

| Component | Detail |
| :--- | :--- |
| **SoC** | Qualcomm Snapdragon 636 (SDM636) |
| **CPU** | Kryo 260 — 4× Cortex-A73 @ 1.8GHz + 4× Cortex-A53 @ 1.6GHz |
| **GPU** | Adreno 509 |
| **RAM** | 4GB LPDDR4 |
| **Storage** | 64GB eMMC 5.1 |
| **Display** | 5.8" IPS LCD, 1080×2280, OTM1911A controller |
| **Wi-Fi** | Qualcomm WCN3680 (wcn36xx) — 802.11 a/b/g/n/ac |
| **Bluetooth** | Bluetooth 5.0 (QCA BT HCI UART) |
| **Audio** | Qualcomm WCD9335 codec |
| **PMIC** | Qualcomm PM660/PM660L |
| **USB** | USB-C 2.0, DWC3 controller |
| **Camera** | Dual (16MP + 5MP), Qualcomm CAMSS |
| **Sensors** | Accelerometer, Gyro, Proximity, Ambient Light |
| **NFC** | ❌ Not present |
| **Bootloader** | Unlockable via `fastboot flashing unlock` |

---

## Bootloader Unlock — One-Time Setup

> [!CAUTION]
> Unlocking the bootloader **wipes all user data** on the device. Do this on a fresh/test device.

1. Enable **Developer Options**: Settings → About Phone → tap *Build Number* 7 times
2. Enable **OEM Unlocking**: Settings → Developer Options → OEM Unlocking ✅
3. Enable **USB Debugging**: Settings → Developer Options → USB Debugging ✅
4. Boot to fastboot:
   ```
   Power off → hold Volume Down + Power until fastboot screen
   ```
5. Connect USB-C to your Mac, then:
   ```bash
   fastboot flashing unlock
   ```
6. Confirm on the device screen with Volume Up

---

## Building ZethraOS for Nokia 6.1 Plus

```bash
# 1. Build the kernel (cross-compile for aarch64)
bash build/scripts/build_kernel.sh

# 2. Build initramfs with zethrad
bash build/scripts/build_initramfs.sh

# 3. Flash to device (boot to fastboot first)
bash build/scripts/flash_nokia61plus.sh

# Dry-run to verify without flashing
bash build/scripts/flash_nokia61plus.sh --dry-run

# Flash to slot B (for A/B testing)
bash build/scripts/flash_nokia61plus.sh --slot b
```

---

## Boot Image Parameters

These are the exact `mkbootimg` parameters for Nokia 6.1 Plus, extracted from stock firmware analysis:

| Parameter | Value |
| :--- | :--- |
| `--pagesize` | `4096` |
| `--base` | `0x00000000` |
| `--kernel_offset` | `0x00008000` |
| `--ramdisk_offset` | `0x01000000` |
| `--tags_offset` | `0x00000100` |
| `--dtb_offset` | `0x01f00000` |

---

## Device Tree (DTB)

The Nokia 6.1 Plus device tree is available in mainline Linux 6.x:
```
arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dts
```

Build alongside the kernel:
```bash
make ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu- dtbs
# Output: arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dtb
```

---

## A/B Partition Layout

Nokia 6.1 Plus uses A/B (seamless) partitions. ZethraOS's OTA service (`zethra-otad`) targets this layout:

| Partition | Slot A | Slot B |
| :--- | :--- | :--- |
| Boot | `boot_a` | `boot_b` |
| System | `system_a` | `system_b` |
| Vendor | `vendor_a` | `vendor_b` |
| Userdata | `userdata` | *(shared)* |

---

## Serial Debug Console

The kernel boots with virtual USB CDC ACM serial console enabled. The host computer detects it at:
```
/dev/tty.usbmodemZETHRA0000011
```

To connect to the interactive console:
```bash
screen /dev/tty.usbmodemZETHRA0000011 115200
```
*(If the terminal hangs or is stuck in an echo state, send a reset sequence: `\n\x03\n\x04\n\n\n` to recover the `~ # ` prompt.)*

System supervisor logs can be checked via:
```bash
cat /mnt/persist/zethrad.log
```

---

## Mainline Kernel Support Status (SDM636)

| Subsystem | Mainline Status |
| :--- | :--- |
| CPU / SMP | ✅ Full |
| GPU (Adreno 509 / freedreno) | ✅ Full (DRM_MSM) |
| Display (DSI / NT35597) | 🟡 Partial — needs DTS tuning |
| Wi-Fi (WCN36xx) | ✅ Full |
| Bluetooth (QCA HCI) | ✅ Full |
| Audio (WCD9335) | 🟡 Partial — driver present, DAI links need config |
| eMMC (SDHCI-MSM) | ✅ Full |
| USB (DWC3-QCOM) | ✅ Full |
| PMIC / Regulators (PM8953) | 🟡 Partial — basic regulator support |
| Modem / Telephony | 🔴 Requires Qualcomm proprietary firmware blobs |
| Camera (CAMSS) | 🟡 Partial |

> [!NOTE]
> For the initial boot target (v0.3.0), focus is on: **CPU + eMMC + USB + Serial console**.
> Display, Wi-Fi, Audio, and Telephony come in subsequent milestones.

---

## Rollback

If ZethraOS fails to boot, switch back to the stock Android slot:
```bash
# Boot to fastboot (Volume Down + Power)
fastboot set_active b     # or 'a' — whichever has stock Android
fastboot reboot
```
