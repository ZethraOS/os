# ZethraOS — Architecture Deep Dive

## Overview

ZethraOS is a mobile operating system built on four principles:

1. **Legal clarity** — every component has a clear, permissive license
2. **AI-native** — the OS heals itself using language models
3. **Privacy by design** — no data leaves the device without explicit consent
4. **Rust-first** — memory-safe userspace without garbage collection overhead

---

## Layer 1: Linux Kernel

ZethraOS uses an unmodified upstream Linux 6.x kernel. We do not fork the kernel — we contribute patches upstream where possible, and carry a minimal set of out-of-tree patches for hardware support.

**Why not fork?**
- Forks diverge quickly from security fixes
- Upstream review improves code quality
- No legal complications with GPL-2 compliance

**Our kernel patches (`kernel/patches/`):**
- `0001-zethra-crash-export.patch` — exports crash metadata to a `/proc/zethra/crashes` pseudo-file for the AI daemon to read
- `0002-zethra-perf-counters.patch` — adds extended hardware performance counters for battery and thermal profiling
- `0003-zethra-seccomp-profiles.patch` — per-app syscall filtering with a JSON profile format

**Key kernel config choices:**
- `CONFIG_PREEMPT=y` — full kernel preemption for low UI latency
- `CONFIG_CPU_FREQ_GOV_SCHEDUTIL=y` — schedutil governor ties CPU frequency to scheduler load, best for heterogeneous ARM CPUs (big.LITTLE)
- `CONFIG_BPF_LSM=y` — eBPF-based security policy enforcement; used by our app sandbox
- `CONFIG_IO_URING=y` — modern async I/O for storage-heavy workloads
- `CONFIG_F2FS_FS=y` — Flash-Friendly File System for `/data` partition

---

## Layer 2: Hardware Abstraction Layer (HAL)

The HAL is a set of Rust `async trait` definitions in `hal/src/lib.rs`. Each hardware subsystem has its own trait:

| Trait | Socket | Implementations |
|-------|--------|-----------------|
| `CameraHal` | `/run/zethra/camera.sock` | `qcom-cam-hal`, `sim-cam-hal` |
| `SensorHal` | `/run/zethra/sensors.sock` | `iio-sensor-hal`, `sim-sensor-hal` |
| `DisplayHal` | `/run/zethra/display.sock` | `drm-display-hal` |
| `BiometricHal` | `/run/zethra/biometric.sock` | `fpc-fp-hal`, `sim-biometric-hal` |
| `PowerHal` | `/run/zethra/power.sock` | `qcom-power-hal` |

Each HAL implementation runs in its own process for fault isolation. A crashing camera HAL does not crash the compositor.

**Simulator HALs** (`*-sim-hal`) are used in CI and QEMU for hardware-free testing. Every feature must be testable without real hardware.

---

## Layer 3: System Services

### zethrad (PID 1)

`zethrad` is our init system. It replaces both `systemd` and Android's `init`. Responsibilities:

- Mount essential filesystems (`proc`, `sys`, `devtmpfs`, `tmpfs`)
- Parse `.unit.toml` files from `/etc/zethra/units/`
- Topologically sort services by `after` dependency
- Spawn processes and supervise them (restart on crash)
- Write health JSON to `/run/zethra/health.json` every 30 seconds
- Expose a Unix socket for `zethractl` (service control CLI)

Unit file example:
```toml
name        = "zethra-telephony"
description = "ZethraOS telephony stack"
after       = ["zethrad-base", "zethra-modem-hal"]
exec_start  = "/usr/lib/zethra/telephony/zethra-telephonyd"
restart     = "on_failure"
restart_delay_ms = 2000
watchdog_sec = 30
```

### Telephony (zethra-telephonyd)

Manages voice calls and SMS over a modem. Supports two backends:
- `AtModem` — direct AT command interface over serial port (real hardware)
- `SimulatedModem` — software simulator for development/CI

All apps communicate via a typed JSON protocol over `/run/zethra/telephony.sock`.

### Network (zethra-networkd)

Manages Wi-Fi (nl80211 netlink), mobile data, DNS-over-HTTPS, and hotspot. Uses `iwd` or `wpa_supplicant` for WPA authentication. DNS is proxied through a local DoH resolver (default: Cloudflare 1.1.1.1 or Quad9).

---

## Layer 4: ZethraShell (UI)

The compositor is built on [Smithay](https://github.com/Smithay/smithay) — a pure Rust Wayland compositor library. We chose Smithay because:

- Pure Rust — no C/C++ memory safety issues
- Direct DRM/KMS access — no X11 or Weston dependency
- Active maintenance with a responsive upstream

**Rendering pipeline:**
```
App (Wayland client)
  → Wayland protocol (XDG shell + ZethraShell protocol extensions)
  → ZethraCompositor (Smithay)
  → OpenGL ES 3.x (Mesa/freedreno/panfrost)
  → DRM/KMS plane commit
  → Display panel
```

**ZethraShell protocol extensions** (custom Wayland protocol):
- `zethra_gesture` — mobile gesture events (swipe-up-to-home, back swipe, etc.)
- `zethra_app_lifecycle` — foreground/background/stop signals to apps
- `zethra_quick_settings` — status bar overlay protocol

**Animation system:**
All animations go through the `Animation` struct with three easing modes:
- `Linear` — for progress bars
- `EaseOut` (cubic) — for most UI transitions
- `Spring` — for app open/close, feels physical and natural

Target: 120fps on 120Hz panels, 60fps minimum on all supported hardware.

---

## Layer 5: ZethraAI Daemon

This is the most novel part of ZethraOS. The daemon runs continuously, watching for problems and fixing them.

### Data inputs

| Source | Path | Update rate |
|--------|------|-------------|
| Crash logs | `/var/log/zethra/crashes/` | On crash |
| Kernel panic log | `/proc/zethra/crashes` | On panic |
| Service health | `/run/zethra/health.json` | Every 30s |
| CVE feed | NVD API | Every 6 hours |
| Performance metrics | `/proc/zethra/perf` | Every 5 min |

### Decision pipeline

```
Issue detected
    ↓
Claude API: analyze_issue()
    → root cause, affected files, proposed patch, confidence, risk level
    ↓
confidence > 0.7?
    → YES: Claude API: review_patch() (second opinion)
    → NO: save to patches/needs-review/ for human
    ↓
review.approved?
    → YES: write patch to patches/staged/
    → NO: save for human review
    ↓
CI triggered (ci.yml repository_dispatch)
    ↓
All CI jobs pass?
    → YES + confidence >= 0.92 + risk <= Medium → AUTO-MERGE + OTA trigger
    → NO or high risk → human review queue
```

### Confidence thresholds

| Confidence | Action |
|-----------|--------|
| < 0.50 | Discard — likely hallucination |
| 0.50–0.70 | Save for human review with full analysis |
| 0.70–0.92 | Trigger CI; require human merge approval |
| >= 0.92 + Low/Medium risk | Full auto-merge + OTA to dev channel |

### What Claude is NOT allowed to auto-merge

- `risk_level = Critical` or `High` — always human reviewed
- Changes to `zethrad` (PID 1) — too risky to auto-update
- Changes to HAL signing or key management
- Changes to the ZethraAI daemon itself (no self-modification)
- Changes to the auto-merge confidence thresholds themselves

---

## Release Bot

The release bot (`zethra-release-bot`) runs alongside the AI daemon. It:

1. Polls CI for successful builds
2. Infers semver bump from risk level (Critical → minor, else patch)
3. Generates human-readable changelogs with Claude
4. Builds and signs the OTA package
5. Publishes to the OTA server with gradual rollout (1% → 10% → 100%)
6. Monitors error rates post-rollout (5% threshold triggers auto-rollback)
7. Notifies community channels

**Rollout schedule:**

| Channel | Initial rollout | Soak time | Full rollout |
|---------|----------------|-----------|--------------|
| dev | 100% | 0 hours | Immediate |
| beta | 10% | 24 hours | Manual or auto if clean |
| stable | 1% | 168 hours (1 week) | Manual sign-off required |

---

## App Sandbox

Apps run in a sandbox using:
- Linux **namespaces** (PID, mount, network, IPC)
- **cgroups v2** for CPU and memory limits
- **seccomp-bpf** with per-app syscall profiles (JSON format in `/etc/zethra/seccomp/`)
- **OverlayFS** for filesystem isolation (`/data/app/<id>/` as writable layer)

Apps communicate with system services only through typed IPC sockets. There is no shared memory between apps and system services.

---

## OTA Update System

ZethraOS uses **A/B partition updates** (seamless updates):

```
Partition layout:
  /boot_a, /system_a  ← currently running
  /boot_b, /system_b  ← update written here in background
  /data               ← persistent user data (never wiped by OTA)
```

The update engine writes to the inactive slot while the device is running normally. On next boot, the bootloader switches to the new slot. If the new slot fails to boot 3 times, it rolls back automatically.

Update payload format: `bsdiff` binary deltas over the full partition image, compressed with `zstd`. Typical patch size for a security fix: 2–20 MB. Full system update: 150–400 MB.

---

## Security model

- **Verified boot**: every partition is signed with the device-specific key; bootloader rejects unsigned images
- **dm-verity**: system partition is verified on every block read; tampering is detected at runtime
- **SELinux enforcing**: all processes run under an SELinux domain; policy is in `security/selinux/`
- **No root shell by default**: `su` is not present in production builds
- **KASLR + PIE**: kernel and all userspace binaries are position-independent with ASLR
- **Stack canaries + FORTIFY_SOURCE**: all Rust and C code compiled with these protections
- **Memory tagging (MTE)** on ARM64 v8.5+: hardware-enforced memory safety for heap bugs

---

## Legal compliance checklist

| Requirement | Status | Notes |
|------------|--------|-------|
| No AOSP code | ✅ | Clean-room implementation throughout |
| Linux kernel GPL-2 compliance | ✅ | All kernel module source included |
| Apache-2.0 patent grant | ✅ | Covers all userspace contributors |
| No Android trademark use | ✅ | We use "ZethraOS", never "Android" |
| No Google API stubs | ✅ | No GMS, no Firebase, no Google Play |
| Smithay (MIT) compatible | ✅ | MIT is compatible with Apache-2.0 |
| Mesa (MIT) compatible | ✅ | Same |
| BlueZ (GPL-2) | ✅ | Only in kernel space, not linked into userspace |

**Recommended next steps for legal hardening:**
1. Join [Open Invention Network (OIN)](https://openinventionnetwork.com) at v1.0
2. Get a trademark on "ZethraOS" before public launch
3. Commission a formal IP audit before seeking VC funding
4. Consider [Software Freedom Conservancy](https://sfconservancy.org) for fiscal sponsorship
l IP audit before seeking VC funding
4. Consider [Software Freedom Conservancy](https://sfconservancy.org) for fiscal sponsorship
