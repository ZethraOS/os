# ZethraOS

> An open, AI-native mobile operating system — built on Linux, beyond Android.

[![CI](https://github.com/ZethraOS/os/actions/workflows/ci.yml/badge.svg)](https://github.com/ZethraOS/os/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)
[![Kernel](https://img.shields.io/badge/kernel-Linux%206.x-orange)](kernel/)

---

## What is ZethraOS?

ZethraOS is a mobile operating system built from scratch on the Linux kernel. It is:

- **Legal** — zero AOSP code, clean-room design, Apache 2.0 + GPL-2
- **Open source** — everything except signing keys is public
- **AI-native** — the ZethraAI daemon autonomously patches bugs, fixes CVEs, and ships OTA updates with minimal human intervention
- **Privacy-first** — no Google Play Services, no mandatory telemetry, on-device AI inference
- **Performance-first** — Rust userspace, Wayland compositor, sub-2s cold boot target

---

## Architecture

```
┌─────────────────────────────────────────────┐
│            ZethraAI Daemon                  │  ← Self-healing brain
│  crash analysis · patch gen · auto-release  │
├─────────────────────────────────────────────┤
│            ZethraShell (UI)                 │  ← Wayland compositor (Rust/Smithay)
│  compositor · toolkit · launcher            │
├─────────────────────────────────────────────┤
│           System Services                   │  ← All written in Rust
│  zethrad (init) · telephony · network       │
├─────────────────────────────────────────────┤
│    Hardware Abstraction Layer (HAL)         │  ← Modular, Treble-inspired
│  camera · sensors · display · modem         │
├─────────────────────────────────────────────┤
│          Linux Kernel 6.x                   │  ← GPL-2, unmodified upstream
│  ARM64 · eBPF · cgroups v2 · io_uring       │
└─────────────────────────────────────────────┘
```

---

## AI Self-Healing Pipeline

The core differentiator of ZethraOS is the `zethra-ai-daemon`:

1. **Monitor** — watches `/var/log/zethra/crashes/` and CVE feeds
2. **Analyze** — sends crash data + stack traces to Claude API for root cause analysis
3. **Patch** — Claude generates a unified diff patch + regression tests
4. **Test** — CI boots a QEMU image with the patch and runs test suite
5. **Release** — if confidence ≥ 0.92 and risk ≤ Medium, auto-merges and triggers OTA

Human maintainers receive a Slack notification but are not required to act.

---

## Repository Layout

```
zethraos/
├── kernel/               # Kernel defconfig, patches, modules
├── hal/                  # Hardware abstraction layer
├── services/
│   ├── zethrad/          # PID 1 init & service manager (Rust)
│   ├── telephony/        # Phone / SMS stack
│   ├── network/          # Wi-Fi, BT, cellular data
│   └── sensors/          # Sensor HAL bridge
├── shell/
│   ├── compositor/       # Wayland compositor (Smithay)
│   ├── toolkit/          # UI toolkit
│   └── launcher/         # Home screen
├── ai/
│   ├── daemon/           # ZethraAI self-healing daemon
│   ├── analyzer/         # Issue classification
│   ├── patcher/          # Patch generation helpers
│   └── release-bot/      # OTA release automation
├── apps/                 # First-party apps (dialer, messages, settings)
├── build/
│   ├── scripts/          # ci.yml, build_ota.sh, publish_ota.sh
│   └── configs/          # Unit files, default configs
├── tools/
│   └── ci/               # check_kernel_config.py, etc.
└── docs/                 # Architecture, contributing, legal
```

---

## Building

### Prerequisites

```bash
# Ubuntu 24.04 recommended
sudo apt-get install -y \
  gcc-aarch64-linux-gnu bc libssl-dev flex bison \
  qemu-system-aarch64 python3 git

# Rust (stable + ARM64 cross target)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add aarch64-unknown-linux-gnu
```

### Build everything

```bash
# Cross-compile all Rust crates for ARM64
cargo build --release --target aarch64-unknown-linux-gnu

# Build kernel (downloads Linux 6.x source)
bash build/scripts/build_kernel.sh

# Boot in QEMU for testing
bash build/scripts/qemu_boot.sh
```

### Run ZethraAI daemon locally (for development)

```bash
export ANTHROPIC_API_KEY=your_key_here
cargo run --bin zethra-ai-daemon
```

---

## Legal

ZethraOS is built to be fully legally clean:

| Component | License | Notes |
|-----------|---------|-------|
| Linux kernel | GPL-2.0 | Unmodified upstream; our modules are GPL-2 |
| Userspace (Rust) | Apache-2.0 | All original code |
| Smithay (compositor) | MIT | Pure Rust Wayland lib |
| Mesa (GPU) | MIT | Open GPU drivers |
| BlueZ (BT) | GPL-2.0 | Standard Linux BT stack |

**No AOSP code is used.** The design is clean-room. We recommend joining the [Open Invention Network](https://openinventionnetwork.com/) for patent protection once the project reaches v1.0.

---

## Roadmap

| Phase | Timeline | Milestone |
|-------|----------|-----------|
| 1 | 0–6 months | Bootable QEMU image, zethrad, ZethraAI scaffold, CI |
| 2 | 6–12 months | Phone/SMS, Wi-Fi/BT, app sandbox, OTA system |
| 3 | 12–18 months | ZethraAI live on device, auto-patch + auto-release |
| 4 | 18–24 months | Reference device port, community launch, v1.0 stable |

---

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md). All contributions require:
- Apache-2.0 or GPL-2 compatible license
- `Signed-off-by` line (DCO)
- Tests for any new functionality

---

## License

- Userspace: [Apache-2.0](LICENSE-APACHE)
- Kernel modules: GPL-2.0
