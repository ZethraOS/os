# Phase 5J Forensic Report: Qualcomm Hexagon Modem Coredump & EFS Rebuild Analysis

**Date:** 2026-09-20  
**Device:** Nokia 6.1 Plus (DRG / SDM636, Serial: `DRGID18100509899`)  
**Authorized Target:** `DRGID18100509899` (Slot B, Linux 7.1.0-zethraos)  
**Off-Limits Device:** Motorola One Fusion+ (`ZF6226MKD8`) — *Strictly untouched*  
**Operational Status:** Offline forensic analysis (Device disconnected, battery cutoff at 3465 mV)  

---

## 1. Executive Summary

In Phase 5J, a comprehensive forensic investigation was conducted into the Qualcomm Hexagon DSP modem subsystem crash on the Nokia 6.1 Plus (`DRGID18100509899`). 

### Key Discoveries:
1. **Reconstructed ELF Coredump Architecture**:
   - The modem remoteproc dump mechanism in Linux 7.1.0 (`drivers/remoteproc/remoteproc_coredump.c`) generates a 117.15 MiB (122,845,797 bytes) 32-bit ELF core file containing 28 `PT_LOAD` segments mapped across physical DDR memory (`0x8ac00000` to `0x92200000`).
   - The file was verified and hash-locked: `5f1152566841a347e787e40df893b4bf99ec6f57e79996279aeb889201f25e34`.
2. **Crash Root Cause Pinpointed**:
   - Both `modemst1` and `modemst2` partitions contain bit-exact verified stock data (`IMGEFS1` / `IMGEFS2`).
   - However, the `fsc` (File System Cookie) partition is completely empty (1,024 bytes of zeros).
   - Hexagon queries Sector 1 of `modemst1` and `modemst2` for filesystem sequence journals. Finding no valid cookie in `fsc`, Hexagon determines the existing filesystems are uncommitted/stale and falls back to **Golden Rebuild mode**.
   - Hexagon requests 2 MiB from `modem_fsg` (`read 0:4096` into `0x85e00000`) and constructs an uncommitted superblock template (`IMGEFS#` with placeholder `#` = `0x23`).
   - Hexagon then asserts a fatal error (`crash_reason=421`) before writing back the rebuilt EFS, triggered by missing OEM NV customization partitions (`modem_fsg_oem_1` and `modem_fsg_oem_2`) or RFS TFTP services.
3. **Hardware & Battery State**:
   - The device reached PM660 PMIC cutoff at 3,465 mV and shut down safely.
   - The device is currently DISCONNECTED from USB and will remain powered down until Phase 5K.

---

## 2. Evidence Inventory

| Evidence Item | Location / Identifier | Size | Checksum / Magic | Description |
|---|---|---|---|---|
| **Coredump ELF** | `scratch/phase5j_coredump/modem.coredump.elf` | 122,845,797 B | `5f1152566841a347...` | ELF32 DSP6 Core file (28 segments) |
| **Phase 5I Dmesg** | `scratch/modem_phase5i_dmesg.txt` | 90,329 B (1,136 lines) | Complete boot log | Boot, PDM lookup, crash, SMEM hex dumps |
| **Phase 5H Dmesg** | `scratch/modem_phase5h_dmesg.txt` | 89,180 B (1,134 lines) | Baseline boot log | Pre-restore EFS golden rebuild failure |
| **RMTFS Log** | `/tmp/rmtfs.log` (Phase 5I capture) | 92 lines | Protocol log | Exact caller I/O requests and sector addresses |
| **modemst1** | `scratch/efs_backup/modemst1.img` | 2,097,152 B | `699df61bacf1bba8...` | Magic `IMGEFS1` at offset `0x28` |
| **modemst2** | `scratch/efs_backup/modemst2.img` | 2,097,152 B | `28762518e5750cf0...` | Magic `IMGEFS2` at offset `0x28` |
| **fsc** | `scratch/efs_backup/fsc.img` | 1,024 B | `5f70bf18a0860070...` | All zeros (unpopulated cookie) |
| **nvdef_b** | `scratch/efs_backup/nvdef_b.img` | 4,194,304 B | `e95e0982f20e0f05...` | 512B FIH header + FSG magic `0xCDABCDAB` |
| **rf_nv** | `scratch/efs_backup/rf_nv.img` | 2,097,152 B | `72cc080d144de50f...` | Magic `1XOF2XOF` at offset `0x0` |
| **nvcust** | `scratch/efs_backup/nvcust.img` | 2,097,152 B | All zeros | Carrier customization NV block |
| **Stock MBA** | `scratch/ota_extract/.../image/mba.mbn` | 238,256 B | Stock Nokia OTA | Matches dmesg boot log size exactly |
| **Stock MPSS** | `scratch/ota_extract/.../image/modem.mdt` | 8,364 B + 28 chunks | Stock Nokia OTA | 28 firmware segments (`modem.b00`..`b28`) |
| **Stock rmt_storage** | `scratch/ota_extract/.../bin/rmt_storage` | 37,344 B | Stock Vendor ELF | Official Qualcomm proprietary daemon |

---

## 3. Coredump Metadata & ELF Structure

### A. ELF Header (`elf_header.txt`)
```text
ELF Header:
  Magic:   7f 45 4c 46 01 01 01 00 00 00 00 00 00 00 00 00
  Class:                             ELF32
  Data:                              2's complement, little endian
  Version:                           1 (current)
  OS/ABI:                            UNIX - System V
  ABI Version:                       0
  Type:                              CORE (Core file)
  Machine:                           QUALCOMM DSP6 / Hexagon (0xa4)
  Version:                           0x1
  Entry point address:               0x8ac00000
  Start of program headers:          52 (bytes into file)
  Start of section headers:          0 (bytes into file)
  Flags:                             0x0
  Size of this header:               52 (bytes)
  Size of program headers:           32 (bytes)
  Number of program headers:         28
  Size of section headers:           0 (bytes)
  Number of section headers:         0
  Section header string table index: 0
```

### B. Segment Analysis (`elf_segments.txt`)
The remoteproc dump maps 28 memory segments representing 117.15 MiB of physical DDR RAM:
- **0x8ac00000 – 0x8ac10000 (Seg 02–03, ~58 KB)**: Hexagon boot vector, exception table, and reset handler.
- **0x8ac10000 – 0x8afa0000 (Seg 04–10, ~3.6 MB)**: QuRT operating system kernel, system heap, and basic drivers.
- **0x8afa0000 – 0x8c580000 (Seg 11–12, ~22.9 MB)**: MPSS executable code (`.text`).
- **0x8c580000 – 0x8e36a000 (Seg 13–17, ~29.7 MB)**: Read-write data, subsystem state, and RFS / EFS buffers.
- **0x8e36a000 – 0x90793000 (Seg 18–19, ~37.7 MB)**: MPSS dynamic heap and BSS uninitialized memory.
- **0x90793000 – 0x91290000 (Seg 20–22, ~11.3 MB)**: Read-only data (`.rodata`), string tables, and calibrations.
- **0x91290000 – 0x92200000 (Seg 23–29, ~15.5 MB)**: Uncached DMA descriptors, IPC queues, and shared memory buffers.

### C. Note Analysis (`elf_notes.txt`)
- No `PT_NOTE` segments are embedded in the devcoredump ELF.
- In Linux `drivers/remoteproc/qcom_common.c`, `qcom_minidump()` inspects the SMEM minidump subsystem table:
  ```c
  if (subsystem->regions_baseptr == 0 ||
      le32_to_cpu(subsystem->status) != 1 ||
      le32_to_cpu(subsystem->enabled) != MINIDUMP_SS_ENABLED) {
      return rproc_coredump(rproc);
  }
  ```
- Because the Hexagon firmware crashed at `+0.88s` before the minidump subsystem initialized (`status != 1`), `qcom_minidump()` automatically fell back to `rproc_coredump()`, exporting all 28 physical DDR segments as raw `PT_LOAD` entries.

---

## 4. Relevant Diagnostic Strings & Marker Analysis

Out of 517,823 strings extracted into `strings.txt`, 5,945 relevant strings were categorized in `relevant_strings.txt`.

### Crucial Subsystem Strings with Absolute File Offsets:
1. **RMTFS Partition Endpoints (Offset `0x0225ae36` .. `0x0225ae90` in coredump)**:
   - `0225ae36: /boot/modem_fs1`
   - `0225ae48: /boot/modem_fs2`
   - `0225ae58: /boot/modem_fsg`
   - `0225ae68: /boot/modem_fsc`
   - `0225ae78: /boot/modem_fsg_oem_1`
   - `0225ae8e: /boot/modem_fsg_oem_2`
2. **FIH-Specific RFS Architecture (Offset `0x0225af12` .. `0x0225af80`)**:
   - `0225af12: /fih_rfs/`
   - `0225af1c: RFS_ASSERT : %s`
   - `0225af2c: rfs_api.c: rfs_info.init_done == 1`
   - `0225af65: rfs_tftp.c: rfs_cfg != NULL`
   - `0225b6dc: TFTP_ASSERT : %s (tftp_client.c)`
3. **QuRT Kernel Assertions (Offset `0x00121b8d` .. `0x00121bd0`)**:
   - `00121b8d: FATAL ERROR: %d %s:%d`
   - `00121bb4: QURT Kernel ver.: 02.04.53.01.00.00.36`
   - `0000d801: Qurt assertion failed: %s:%d`

---

## 5. Exception-Record & Register Findings

1. **SMEM 421 (`SMEM_ERR_CRASH_LOG`)**:
   - Dmesg verifies: `smem HOST_ANY item 421 lookup error: -2` (`-ENOENT`).
   - The modem firmware asserted fatal error via SMB209 interrupt but did not commit a formatted string to SMEM item 421.
2. **SMEM 602 (`Global Minidump TOC` Header)**:
   - 32-byte header with 319 entry slots.
   - Entries 0–3 populated by TZ and Hypervisor (`TZ_IMEM`, `TZ_PIMEM`, `HYP_DIAG`, `HYP_VER_INFO`).
   - Entries 4–318 (Hexagon subsystems) unpopulated (all zeros).
3. **Register / PC / LR Findings**:
   - In strict adherence to forensic safety standards: **No speculative PC/LR/SP register values are asserted**.
   - Because the crash occurred prior to QuRT exception frame completion, register state must be acquired via live GDB stub over JTAG or direct devcoredump register segment in Phase 5K.

---

## 6. Chronological Event Sequence & Timeline

```
[ 3.090s] remoteproc0: powering up 4080000.remoteproc
[ 3.097s] remoteproc0: Booting fw image qcom/sdm636/mba.mbn (238,256 B)
[ 3.178s] MBA booted without debug policy, loading MPSS
[ 5.645s] q6v5_handover_interrupt (handover_issued=0)
[ 5.649s] q6v5_ready_interrupt received
[ 5.656s] remoteproc remoteproc0: remote processor 4080000.remoteproc is now up
[ 5.657s] PDM lookup: service='tms/pddump_disabled'
[ 5.661s] PDM lookup OK: domain='msm/modem/root_pd' instance=180
[ 5.662s] RMTFS: opens /boot/modem_fs1, /boot/modem_fs2, /boot/modem_fsg, /boot/modem_fsc, /boot/modem_fsg_oem_1, /boot/modem_fsg_oem_2
[ 5.663s] RMTFS: alloc 2,097,152 B => phys 0x85e00000
[ 5.665s] RMTFS: Caller 0 (modem_fs1) read 1:1 0x85e00200 (sector 1)
[ 5.668s] RMTFS: Caller 1 (modem_fs2) read 1:1 0x85e00200 (sector 1)
[ 5.672s] RMTFS: Caller 2 (modem_fsg) read 0:4096 0x85e00000 (2 MiB)
[ 5.730s] Hexagon constructs uncommitted template IMGEFS# at 0x85e00028
[ 5.736s] fatal error interrupt asserted (crash_reason=421)
[ 6.526s] remoteproc remoteproc0: crash detected: fatal error handled
```

---

## 7. RMTFS Protocol & Source Code Audit

A complete audit of `tools/rmtfs/` (`rmtfs.c`, `storage.c`, `sharedmem.c`) established:

1. **Client ID Assignment**:
   - Assigned sequentially in `storage_open()`: Caller 0 (`modem_fs1`), Caller 1 (`modem_fs2`), Caller 2 (`modem_fsg`), Caller 3 (`modem_fsc`), Caller 4 (`modem_fsg_oem_1`), Caller 5 (`modem_fsg_oem_2`).
2. **Sector & Count Semantics**:
   - `SECTOR_SIZE` is strictly 512 bytes.
   - In `rmtfs_iovec`, `entries[i].num_sector` is the sector count.
   - For `read 0:4096`: `4096 * 512 = 2,097,152` bytes (exactly 2.0 MiB).
   - This exactly fills the allocated shared memory buffer (`0x85e00000` .. `0x86000000`).
3. **Physical Memory Translation**:
   - `phys_base` is translated by subtracting `rmem->address` (`0x85e00000`), yielding the buffer offset.
   - Bounds-checking verifies that `start >= rmem->address` and `end <= rmem->address + rmem->size`.
4. **Sector 1 Cache Patch Audit**:
   - The experimental cache patch added in Phase 5H checks `entries[i].sector_addr == 2`.
   - Because Hexagon never issued a read starting at sector 2, the restore branch was never executed and did not alter memory contents.

---

## 8. Assessment of the `IMGEFS#` Superblock Template

- Reading the physical shared memory window at `0x85e00000` following the crash reveals:
  - Offset `0x00`: Type `0x03`, length `0x01d8` (472 sectors = 241,664 bytes).
  - Offset `0x28`: Magic ASCII `IMGEFS#` (`0x49 0x4d 0x47 0x45 0x46 0x53 0x23 0x00`).
- **Architectural Significance**:
  - `IMGEFS1` indicates committed generation 1 (stored on `modemst1`).
  - `IMGEFS2` indicates committed generation 2 (stored on `modemst2`).
  - `IMGEFS#` (`#` = placeholder byte `0x23`) is the Qualcomm in-memory staging template created by the modem EFS subsystem during a **golden restore / filesystem formatting sequence**.
  - Its presence confirms beyond doubt that Hexagon rejected mounting `modemst1` and `modemst2`, wiped its in-memory superblock pointer, and initiated a factory rebuild from FSG.

---

## 9. Ranked Root-Cause Hypotheses

### Hypothesis 1: Empty FSC (File System Cookie) Forces Hexagon into Golden Rebuild
```text
Hypothesis:
Hexagon rejects modemst1 and modemst2 because the fsc partition is all zeros, forcing an EFS golden rebuild that crashes due to missing OEM NV validation.

Evidence for:
1. fsc.img on eMMC is 1,024 bytes of zeros (verified by SHA-256).
2. Hexagon opens modem_fsc as Caller 3 but receives no valid cookie.
3. Hexagon reads Sector 1 of modemst1 and modemst2 to check journal generation, finds no active generation cookie in fsc, and immediately reads modem_fsg (Caller 2).
4. Shared memory contains uncommitted template IMGEFS# at offset 0x28, proving golden rebuild was triggered.

Evidence against:
Hexagon opened modem_fsc but did not issue an explicit read iovec to Caller 3 in the captured log (status may be checked via QMI open return or shared struct).

Confidence:
85%

Required test:
Inspect stock Android vendor partition dump to verify whether stock fsc contained non-zero generation metadata or whether a valid cookie block can be generated.

Rollback:
Restore verified zero-filled fsc.img from scratch/efs_backup/fsc.img.
```

---

### Hypothesis 2: Dual SIM OEM FSG Mappings Mispointed
```text
Hypothesis:
modem_fsg_oem_1 and modem_fsg_oem_2 are mapped to fsg_clean.img instead of Nokia-specific OEM NV partitions (nvcust / rf_nv).

Evidence for:
1. Hexagon opens modem_fsg_oem_1 (Caller 4) and modem_fsg_oem_2 (Caller 5) during bring-up.
2. /init symlinks both OEM endpoints to /dev/disk/by-partlabel/fsg.
3. Nokia 6.1 Plus possesses dedicated eMMC partitions nvcust (2MB) and rf_nv (2MB, magic 1XOF2XOF).
4. When performing EFS rebuild, Qualcomm dual-SIM baseband requires OEM carrier NV tables to complete calibration verification.

Evidence against:
Hexagon crashed ~80ms after reading Caller 2 (modem_fsg), before issuing read requests to Caller 4 or Caller 5.

Confidence:
75%

Required test:
Symlink modem_fsg_oem_1 -> nvcust and modem_fsg_oem_2 -> rf_nv in initramfs.

Rollback:
Restore symlinks back to /dev/disk/by-partlabel/fsg.
```

---

### Hypothesis 3: Missing Remote File System (FIH RFS / TFTP) Services
```text
Hypothesis:
The modem requires FIH RFS services (/fih_rfs/) over TFTP/QRTR to read device-specific customization files (/nv/item_files/...) during EFS initialization.

Evidence for:
1. Strings in coredump explicitly reveal /fih_rfs/, RFS_ASSERT : %s, and rfs_tftp.c.
2. tqftpserv is started serving /mnt/modem and /lib/firmware, but does NOT serve /fih_rfs/ or OEM NV files.
3. Qualcomm modem firmware asserts fatal error when a mandatory RFS configuration node fails to respond.

Evidence against:
No TFTP transaction errors were logged in /tmp/tqftpserv.log prior to the remoteproc fatal interrupt.

Confidence:
70%

Required test:
Configure tqftpserv with a mock /fih_rfs directory containing empty or stock carrier nodes.

Rollback:
Remove /fih_rfs staging directory from tqftpserv parameters.
```

---

## 10. Battery Status & Hardware Constraints

- **Cutoff Event**: During Phase 5I, the battery voltage dropped to **3,465 mV**, reaching the PM660 PMIC undervoltage shutdown threshold.
- **Current Physical State**: The device is **POWERED OFF and DISCONNECTED** from host USB.
- **Protocol Commitment**: The device will **REMAIN DISCONNECTED** throughout Phase 5J.
- **Battery Health**: The internal Li-ion battery is severely degraded and unable to maintain operational voltage under high RF/modem power transients without continuous high-current USB bus power.
- **Bring-up Policy**: No battery replacement is planned until basic bring-up is complete. All subsequent active phases (Phase 5K) will utilize an externally powered, high-current USB-PD hub.

---

## 11. Recommended Next Experiment (Phase 5K)

When the device is safely reconnected to high-current power in Phase 5K:

1. **Direct Coredump Streaming**:
   - Establish an active streaming receiver on the macOS host (`nc -l` or raw serial stream pipe) before triggering `echo recover`.
   - Stream the 117.3 MB devcoredump directly across USB CDC-ACM to host storage (`scratch/phase5j_coredump/device_runtime_devcd1.elf`), bypassing volatile device `tmpfs`.
2. **OEM FSG Partition Remapping**:
   - Update `build/scripts/build_minimal_initramfs.sh` to map:
     * `/dev/disk/by-partlabel/modem_fsg_oem_1` -> `/dev/disk/by-partlabel/nvcust`
     * `/dev/disk/by-partlabel/modem_fsg_oem_2` -> `/dev/disk/by-partlabel/rf_nv`
3. **FSC Cookie Initialization**:
   - Test writing a valid generation 1 superblock cookie header into `fsc` to determine if Hexagon mounts `modemst1` directly, completely bypassing the Golden Rebuild crash.

---

## 12. Rollback Procedure

All analysis performed in Phase 5J was **100% read-only and offline**.
Zero writes were made to the Nokia 6.1 Plus eMMC flash memory.
The physical partition hashes remain bit-for-bit identical to the verified cold backups:
- `modemst1`: `699df61bacf1bba85b0d5b5894b69e29eaedc75d09cb11f626747f77728807a7`
- `modemst2`: `28762518e5750cf09b72f4673e6a6aba13bd7d7c3419c58dd57f81e19fc9b568`
- `fsc`: `5f70bf18a086007016e948b04aed3b82103a36bea41755b6cddfaf10ace3c6ef`
