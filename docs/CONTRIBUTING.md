# Contributing to AetherOS

Welcome! AetherOS is an open project and contributions are very welcome.

---

## Before you start

1. Read the [README](../README.md) to understand the architecture.
2. Check open issues on GitHub for things to work on.
3. For large changes, open an **RFC issue** first to discuss the approach.

---

## Legal requirements (DCO)

AetherOS uses the **Developer Certificate of Origin (DCO)** — a lightweight alternative to a CLA. By signing off your commits, you certify that you wrote the code and have the right to contribute it.

Add a `Signed-off-by` line to every commit:

```
git commit -s -m "fix: correct null pointer in aetherd service restart"
```

This produces:

```
fix: correct null pointer in aetherd service restart

Signed-off-by: Your Name <you@example.com>
```

**License requirements:**
- New userspace code → Apache-2.0
- Kernel modules → GPL-2.0
- Do NOT submit code under GPL-3.0 (incompatible with our kernel linking)
- Do NOT copy any AOSP or proprietary code, ever

---

## Code style

**Rust:**
- `cargo fmt` before every commit (CI enforces this)
- `cargo clippy -- -D warnings` must pass
- Prefer `anyhow` for error handling in binaries, `thiserror` in libraries
- Prefer safe Rust; document every `unsafe` block with a safety comment
- Use `tracing` for logging, not `println!`
- Write unit tests for all non-trivial logic

**C (kernel modules only):**
- Follow Linux kernel coding style (`checkpatch.pl`)
- No new C code in userspace — use Rust

**Commit messages:** Conventional Commits format:
```
<type>(<scope>): <description>

[optional body]

Signed-off-by: Name <email>
```
Types: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci`

---

## Project structure

| Directory | Language | What goes here |
|-----------|----------|----------------|
| `kernel/` | C | defconfig, kernel patches, custom kernel modules |
| `hal/` | Rust | HAL trait definitions and device-specific implementations |
| `services/aetherd/` | Rust | Init system — only touch if you really know what you're doing |
| `services/telephony/` | Rust | Telephony daemon |
| `services/network/` | Rust | Network manager |
| `shell/compositor/` | Rust | Wayland compositor |
| `shell/toolkit/` | Rust | UI widgets |
| `ai/daemon/` | Rust | Self-healing AI daemon |
| `ai/release-bot/` | Rust | Autonomous release pipeline |
| `apps/` | Rust/Flutter | First-party apps |
| `tools/` | Python/Bash | Developer tooling |

---

## Running tests

```bash
# All Rust unit tests
cargo test --all

# Kernel config security check
python3 tools/ci/check_kernel_config.py kernel/aether_defconfig

# Boot smoke test in QEMU (requires kernel image)
bash build/scripts/qemu_boot.sh

# Full CI locally (requires act)
act -j rust-check
```

---

## AI-generated patches

The AetherAI daemon automatically generates patches and opens PRs. These are clearly labelled `[AI-GENERATED]` in the title. Human reviewers should:

1. Verify the root cause analysis makes sense
2. Review the diff for correctness and safety
3. Check that the generated tests actually cover the fix
4. Approve normally if satisfied — the CI pipeline handles the rest

Never blindly merge AI patches without reading them. The AI can be wrong.

---

## Security issues

**Do not open public issues for security vulnerabilities.**
Email `security@aetheros.dev` with:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Your suggested fix (optional)

We aim to respond within 48 hours and will credit you in the release notes.

---

## Community

- Matrix: `#aetheros:matrix.org`
- GitHub Discussions for design questions
- GitHub Issues for bugs and feature requests

Thank you for contributing to the future of open mobile computing.
