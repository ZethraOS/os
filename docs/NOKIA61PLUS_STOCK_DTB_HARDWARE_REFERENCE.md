# Nokia 6.1 Plus — Stock DTB Hardware Reference

**Status:** FROZEN 2026-07-15  
**Source:** `build/out/stock_panel.dtb` (SHA-256 `7e97af1b...`) — identical to `extracted_from_boot.dtb` and `sdm636-nokia-frt.dtb`  
**Extracted from:** `/mnt/persist/boot_a_backup.img.gz` on the live device (Slot A, v0.4.0 baseline)  
**Do not modify this document.** It is the definitive hardware reference.

---

## Panel Node (stock Nokia ABL DTB)

```
panel@0 {
    compatible = "orisetech,otm1911a";
    reg = <0>;
    vdda-supply  = <pm660 l1>;   /* phandle 0x56 */
    vddi-supply  = <pm660 l2>;   /* phandle 0x59 */
    reset-gpios  = <tlmm 82 GPIO_ACTIVE_LOW>;
    backlight    = <pm660l wled @d800>;
    port → DSI0 (dsi@c994000)
};
```

---

## Confirmed Hardware Parameters

| Field | Stock Value | Our DTS (`sdm636-nokia-frt.dts`) | Match |
|---|---|---|---|
| `compatible` | `"orisetech,otm1911a"` | `"orisetech,otm1911a"` | ✅ |
| `reset-gpios` | TLMM GPIO **82**, `ACTIVE_LOW` | `&tlmm 82 GPIO_ACTIVE_LOW` | ✅ |
| `vdda-supply` | **PM660 l1**, 1.150 – 1.250 V | `&vreg_l1a_1p225` | ✅ |
| `vddi-supply` | **PM660 l2**, 0.950 – 1.010 V | `&vreg_l2a_1p0` | ✅ |
| `backlight` | `pm660l-wled` (`@d800`), **enabled** | `&pm660l_wled` | ✅ |
| `labibb` | **absent** — PM660L regulators `{}` empty | disabled in defconfig | ✅ |
| DSI host | DSI0 (`dsi@c994000`) **enabled**; DSI1 disabled | `mdss_dsi0` okay | ✅ |
| Data lanes | 4 lanes (`<0 1 2 3>`) | `<0 1 2 3>` | ✅ |
| DSI PHY | `qcom,dsi-phy-14nm-660` `vcca` → PM660L b1 | `vreg_l1b_0p925` | ✅ |

**All critical parameters match. No DTS corrections are needed.**

---

## Regulator Rail Details (from stock DTB)

### PM660 l1 — `vdda-supply` (DSI analog power)
- `compatible = "qcom,rpm-pm660-regulators"` (regulators-1 block)
- `regulator-min-microvolt = <1150000>` (1.150 V)
- `regulator-max-microvolt = <1250000>` (1.250 V)
- `regulator-allow-set-load;`

### PM660 l2 — `vddi-supply` (DSI I/O voltage)
- Same `regulators-1` block as l1
- `regulator-min-microvolt = <950000>` (0.950 V)
- `regulator-max-microvolt = <1010000>` (1.010 V)

### PM660L WLED — backlight
- Node: `leds@d800`, `compatible = "qcom,pm660l-wled"`
- Interrupts: `ovp`, `short`
- `status = "okay"`

### PM660L regulators block
```
regulators {
    compatible = "qcom,pm660l-regulators";
};  /* empty — labibb absent on this SKU */
```

---

## What the Stock DTB Confirms Is NOT Present

- No `labibb` (lab/ibb boost regulators) — PM660L SKU on this device omits them
- No `enable-gpios` on the panel node — power sequencing is purely via PMIC regulators
- No TE (tearing effect) GPIO wired in the DT — TE may be used via internal DSI mechanism
- No secondary DSI (DSI1 disabled)

---

## Kernel Config Status at Time of Freeze

| Symbol | Value | Notes |
|---|---|---|
| `CONFIG_DRM_MSM` | `y` | Main MSM DRM driver |
| `CONFIG_DRM_MSM_DSI` | `y` | DSI sub-driver |
| `CONFIG_DRM_MSM_DSI_14NM_PHY` | `y` | SDM660-class PHY |
| `CONFIG_DRM_PANEL_ORISETECH_OTM1911A` | `y` | Out-of-tree panel driver |
| `CONFIG_BACKLIGHT_QCOM_WLED` | `y` | WLED backlight |
| `CONFIG_BACKLIGHT_CLASS_DEVICE` | `y` | Generic backlight class |
| `labibb` | disabled | Correct — absent on this SKU |

---

## DTB File Integrity

| File | SHA-256 |
|---|---|
| `build/out/stock_panel.dtb` | `7e97af1b385c89e39cfb62e6cab2e1c385432024ece30b284ae9140283086947` |
| `build/out/extracted_from_boot.dtb` | `7e97af1b385c89e39cfb62e6cab2e1c385432024ece30b284ae9140283086947` |
| `build/out/sdm636-nokia-frt.dtb` | `7e97af1b385c89e39cfb62e6cab2e1c385432024ece30b284ae9140283086947` |

All three are byte-identical. The compiled DTS produces exactly the same binary as the stock Nokia firmware DTB for the panel-relevant subtree.

---

*Frozen by: ZethraOS bring-up session, 2026-07-15*  
*Do not modify. Future experiments must reference, not replace, this document.*
