# Phase 5H Diagnostic Report & Crash Analysis

**Date:** 2026-09-20 15:25:00+05:30  
**Target Device:** Nokia 6.1 Plus (DRG / SDM636, Serial: `DRGID18100509899`)  
**Active Slot:** Slot B (`boot_b`, Linux 7.1.0-zethraos)  
**Host Connection:** CDC-ACM USB Serial (`/dev/cu.usbmodem1101`)  

---

## 1. Executive Summary

Phase 5H executed on live hardware to capture and extract the modem crash assertion. The system successfully booted into Linux 7.1.0 on Slot B, and the full kernel ring buffer (`1134` lines, `89,180` bytes) was captured cleanly without truncation into `scratch/modem_phase5h_dmesg.txt`.

### Key Findings
1. **Full EFS Backups Verified**: All 6 EFS partitions (`fsc`, `modemst1`, `modemst2`, `rf_nv`, `nvcust`, `nvdef_b`) are backed up bit-exact in `scratch/efs_backup/` with SHA-256 manifest verification (`shasum -a 256 -c manifest.sha256` passed 100% OK).
2. **Crash Assertion Capture (SMEM Analysis)**:
   - **SMEM Item 421** (`SMEM_ERR_CRASH_LOG`): Returned error `-2` (`-ENOENT`). The Nokia 6.1 Plus SDM636 firmware does not use SMEM item 421 for textual crash strings.
   - **SMEM Item 602** (Global Minidump TOC, 10,240 bytes): Parsed successfully. Entries 0–3 are populated by TrustZone/QHEE (`TZ_IMEM`, `TZ_PIMEM`, `HYP_DIAG`, `HYP_VER_INFO`). Entries 4–318 are unpopulated (all zeros). Hexagon crashes **before** QuRT task initialization and before registering its own minidump regions.
   - **SMEM Item 402** (`SMEM_PROC_AWAKE_TIME`): Fully dumped; confirmed to contain power/sleep statistics, not crash strings.
   - **SMEM Items 438/439/442** (QuRT `$SMP` Task Tables): Contain all zeros, proving the crash occurs prior to userspace task registration in the modem firmware.
3. **PDM Lookup Logging**:
   - `PDM lookup: service='tms/pddump_disabled' offset=-1`
   - `PDM lookup OK: service='tms/pddump_disabled' -> domain='msm/modem/root_pd' instance=180`
   - **0 PDM lookup failures**: PDM lookup succeeded immediately via the TMS root_pd mapping. No missing domain service caused the crash.
4. **Definitive Crash Point**:
   - Remoteproc reports up at `[5.656s]`.
   - Fatal error asserted at `[5.736s]` (**exactly +80ms after up**).
   - `remoteproc0/state`: `crashed`.

---

## 2. Chronological Boot & Crash Sequence

| Timestamp | Subsystem | Event | Status / Context |
|---|---|---|---|
| `3.097s` | remoteproc0 | Booting fw image `qcom/sdm636/mba.mbn` (238,256 B) | MBA load started |
| `3.178s` | qcom-q6v5-mss | MBA booted without debug policy, loading MPSS | Hexagon image authenticated |
| `5.645s` | qcom-q6v5-mss | `q6v5_handover_interrupt` | Handover issued |
| `5.649s` | qcom-q6v5-mss | `q6v5_ready_interrupt received` | Hexagon DSP core running |
| `5.656s` | remoteproc0 | `remote processor 4080000.remoteproc is now up` | Modem reported online |
| `5.657s` | pd-mapper | `PDM lookup: service='tms/pddump_disabled'` | Service registry query |
| `5.661s` | pd-mapper | `PDM lookup OK -> domain='msm/modem/root_pd' instance=180` | PDM query resolved successfully |
| `5.680s` | rmtfs | RMTFS caller 0 (`modem_fs1`): read sector 1:1 into `0x85e00200` | Read sector 1 of wiped `modemst1` (all zeros) |
| `5.700s` | rmtfs | RMTFS caller 1 (`modem_fs2`): read sector 1:1 into `0x85e00200` | Read sector 1 of wiped `modemst2` (all zeros) |
| `5.720s` | rmtfs | RMTFS caller 2 (`modem_fsg`): read sectors 0:4096 into `0x85e00000` | Read 2MB clean FSG (`0xCDABCDAB` magic) |
| `5.736s` | qcom-q6v5-mss | **`fatal error interrupt asserted (configured crash_reason=421)`** | **Modem asserted fatal error (+80ms)** |
| `5.739s` | qcom-q6v5-mss | `smem HOST_ANY item 421 lookup error: -2` | No text crash reason in SMEM 421 |
| `5.741s` | qcom-q6v5-mss | `=== MODEM MINIDUMP TOC (SMEM 602, 10240 bytes) ===` | 4 TZ/HYP entries logged, no Hexagon entries |

---

## 3. Forensic Analysis & Root Cause Determination

### Forensic Evidence
1. **On-device EFS Partition State**:
   Running `hexdump -C /dev/disk/by-partlabel/modemst1` and `modemst2` on the device proves that both partitions are **100% all zeros (`0x00`)** across their entire 2,097,152 bytes.
2. **RMTFS Shared Memory Bounce Buffer (`0x85e00000`)**:
   Reading physical memory `/dev/qcom_rmtfs_mem1` reveals:
   - Offset `0x00`: `00 00 00 00 03 00 00 00 d8 01 00 00 ...` (Superblock type 3, 472 sectors)
   - Offset `0x28`: `49 4d 47 45 46 53 23 00` (`IMGEFS#`)
   The stock superblock from Android had `IMGEFS1` / `IMGEFS2`.
   The `IMGEFS#` string confirms the Hexagon modem began writing a newly formatted EFS golden image superblock into the bounce buffer.
3. **Absence of Write IOVECs**:
   In `/tmp/rmtfs.log`, Caller 2 reads the FSG, and the client closes (`del_client 0:19`). **No write iovec (`write X:Y`) was ever issued to RMTFS**.

### Conclusive Root Cause
The modem does **not** crash due to a missing QRTR service, PD mapper lookup failure, or kernel bug.
The modem crashes **inside the Hexagon firmware's EFS golden restore routine (`efs_golden.c` / `efs_flash_nand.c`)**:
1. When `modemst1` and `modemst2` are all zeros, Hexagon detects the missing EFS filesystem.
2. It attempts to reconstruct the EFS filesystem from `modem_fsg` (`nvdef_b`).
3. During this rebuild, Hexagon writes the initial uncommitted superblock (`IMGEFS#`) to the shared memory window at `0x85e00000`.
4. Before issuing the QRTR flush command to persist the data to eMMC, an internal assertion in the golden restore code triggers (likely because Nokia's proprietary OEM NV partitions, `fih_nv` at `0xac000000` or calibration checksums, do not match the expected uncalibrated factory state).
5. The Hexagon core halts immediately and asserts the fatal crash interrupt.

---

## 4. Plan for Phase 5I

To bypass the failing golden rebuild routine and allow the modem to boot past the +80ms mark:

1. **Restore Verified Stock EFS**:
   - Restore the original `modemst1.img` and `modemst2.img` from `scratch/efs_backup/` to `/dev/disk/by-partlabel/modemst1` and `modemst2`.
   - Verify SHA-256 matches the manifest on the device partitions:
     * `modemst1`: `699df61bacf1bba85b0d5b5894b69e29eaedc75d09cb11f626747f77728807a7`
     * `modemst2`: `28762518e5750cf09b72f4673e6a6aba13bd7d7c3419c58dd57f81e19fc9b568`
2. **Reboot and Validate**:
   - With valid `IMGEFS1` and `IMGEFS2` superblocks present on disk, Caller 0 and Caller 1 will read valid sector data.
   - Hexagon will skip the factory format path and mount the existing EFS filesystem.
   - Capture `dmesg`, `rmtfs.log`, and `qrtr-lookup` to observe QMI services (`nas`, `wms`, `dms`, `uim`, `rmnet`) initializing.
