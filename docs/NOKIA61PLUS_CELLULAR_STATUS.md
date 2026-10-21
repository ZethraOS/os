# Nokia 6.1 Plus — Cellular / Telephony Status

**Status: cellular is NOT functional on the Nokia 6.1 Plus.**
**Root cause of the modem crash: UNKNOWN.**

**Based on**: repository evidence at commit `5924d2f` (refreshed to the actual commit SHA immediately before committing), 2026-10-21, plus the local-only logs marked † below.
**Device**: Nokia 6.1 Plus (TA-1103, SDM636), Linux 7.1.0-zethraos, slot B.

This document records what the available evidence supports. It makes no claim
that any modem recovery approach works.

---

## Summary

| Area | Status |
|---|---|
| Cellular registration, voice calls, SMS, mobile data | **Never achieved.** No attempt was possible on this device. |
| Hexagon modem startup | **Fails.** The modem boots, reports "up", then asserts a fatal error within about 40–190 ms. |
| Telephony userspace (`services/telephony/`) | Written, with simulator tests. **Has not run against the device modem.** |
| Root cause of the modem crash | **UNKNOWN.** |
| Leading hypothesis | EFS / FSC filesystem state (**unproven**) |

---

## Telephony userspace

- `services/telephony/` (about 1,073 lines of Rust) contains an AT command engine,
  PDU SMS handling, emergency-call handling and a ModemManager D-Bus proxy
  (`proxy.rs`) with a simulator fallback gated by `ZETHRA_ALLOW_SIMULATED=1`.
- It has 8 unit tests, which run against the simulator.
- The minimal device initramfs does not include a telephony daemon or ModemManager.
  It runs only the QRTR/RMTFS/TFTP helpers and `fih_skuid_emulator`.
- This code has **not** been exercised against the device modem.

## Modem runtime on the device

In every logged run (Phases 5H, 5L, 5N–5R):

1. `mba.mbn` and the MPSS image load, and `remoteproc0` reports "is now up".
2. `rmtfs` serves EFS partition reads (modemst1, modemst2, and others).
3. The modem asserts a fatal error interrupt (`crash_reason=421`).
4. `/sys/class/remoteproc/remoteproc0/state` reads `crashed`.

The crash occurs before any telephony-related QRTR service appears. From the
modem node, only the following were observed:

- service 66 (service registry notification)
- service 43 (subsystem control)

Phase 5H found the SMEM minidump table empty for the Hexagon and the QuRT task
tables empty, which is consistent with a crash before modem userspace tasks start.

No registration, call, SMS or data test has ever been possible.

---

## Hypothesis (UNPROVEN)

The Phase 5J report hypothesized that an unpopulated FSC (file system cookie) led
the Hexagon to treat `modemst1`/`modemst2` as stale and fall back to a rebuild from
FSG data, ending in the fatal error. This is the leading hypothesis. It is
**not proven**: later phases changed the EFS/FSC state in several ways and the
crash persisted. Other causes (RFS/TFTP services, OEM NV partitions, firmware or
board configuration) have not been ruled out.

No EFS restore, Generation 17 alignment, FSC write or modem recovery approach is
established as correct or as safe. A separate, unreviewed tool on the Phase 5R
branch (`update_modemst1_gen17.py`, not on `dev`) makes claims about dual-bank
generation numbering that the logs in this repository do not settle.

---

## Phase 5A–5R timeline

Crash timing is the gap between "remote processor is now up" and the fatal error
interrupt in that phase's dmesg. † = evidence exists only as uncommitted local files.
"No write-up" means no report exists.

| Phase | What was tried | Outcome |
|---|---|---|
| 5A | Kernel config for remoteproc and QRTR | Implemented |
| 5B | DTS enablement and modem firmware staging | Implemented |
| 5C | `rmtfs` and `qrtr-ns` daemons in initramfs | Implemented |
| 5D | No commit or report found | — |
| 5E | VFAT and EFS partition mapping | Implemented; no crash-timing figure recorded |
| 5F, 5G | Only a 5F dmesg artifact exists | No write-up |
| 5H | Crash diagnostics, FSC cookie; modemst1/2 recorded as all zeros on device | Crash at about +80 ms; report in `scratch/phase5h_crash_analysis.md` |
| 5I | Dmesg referenced by the 5J report | No write-up |
| 5J | Offline coredump analysis; EFS backups in `scratch/efs_backup/` | Hypothesis above; no device run |
| 5K | No commit or report found | — |
| 5L | `fih_skuid_emulator` made persistent | Crash at about +185 ms†, before service 15; the `fih_skuid_emulator` hypothesis was not supported by this run†. Report is local-only. |
| 5M | `tqftpserv` serving `/mnt/persist/rfs` | State `crashed`†; no write-up |
| 5N | nvcust remapped to fsg; FSC CRC bytes zeroed | Crash at about +182 ms; state `crashed`; no write-up |
| 5O | FSC aligned to "Generation 17" | Crash at about +176 ms†; no write-up |
| 5P | modem_fs1/fs2 symlinks swapped | Crash at about +43 ms†; no write-up |
| 5Q | modemst2 copied to modemst1 (dual-bank sync) | Crash at about +49 ms†; no write-up |
| 5R | Surgical Gen 17 write to modemst1 | Crash at about +53 ms†; no write-up. The boot-time modemst1 write was reverted on a local, unpushed branch (commit `3ef2929`, SHA may change). The underlying modem crash remains unresolved. |

No phase changed the outcome.

---

## Not verified

- Any cellular registration, voice call, SMS or mobile data
- Telephony userspace against real modem hardware
- Any QMI service other than 66 and 43 appearing from the modem
- Whether any EFS/FSC state is correct for this device
- The cause of `crash_reason=421`

No recovery or EFS-writing procedure has been approved or validated.

## Evidence

Committed:
- `scratch/phase5h_crash_analysis.md`, `scratch/phase5j_coredump/phase5j_coredump_analysis.md`
- `scratch/modem_phase5h_dmesg.txt`, `scratch/modem_phase5l_dmesg.txt`, `modem_phase5f_dmesg.txt`
- `scratch/phase5n_dmesg.txt`, `scratch/phase5n_remoteproc_state.txt`
- `scratch/efs_backup/` (EFS partition backups with a SHA-256 manifest)
- `services/telephony/`, `docs/HARDWARE_NOKIA61PLUS.md`, `docs/ARCHITECTURE.md`

Local-only (†, not in any commit at time of writing):
- `scratch/phase5l_results.md`, `scratch/phase5m_*`
- `scratch/phase5{o,p,q,r}_dmesg.txt`, `scratch/phase5{l,m,o,p,q,r}_remoteproc_state.txt`, `scratch/phase5{q,r}_qrtr.txt`
