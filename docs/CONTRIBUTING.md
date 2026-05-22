# Contributing to ZethraOS

Welcome! ZethraOS is an open project and contributions are very welcome.

---

## Before you start

1. Read the [README](../README.md) to understand the architecture.
2. Check open issues on GitHub for things to work on.
3. **Fork the repository** to your own account to start working.
4. Create a branch in your fork (e.g., `feature/my-new-feature`).
5. For large changes, open an **RFC issue** first to discuss the approach.

---

## Legal requirements (DCO)

ZethraOS uses the **Developer Certificate of Origin (DCO)** — a lightweight alternative to a CLA. By signing off your commits, you certify that you wrote the code and have the right to contribute it.

Add a `Signed-off-by` line to every commit:

```
git commit -s -m "fix: correct null pointer in zethrad service restart"
```

This produces:

```
fix: correct null pointer in zethrad service restart

## Branch Strategy & Governance

ZethraOS maintains a high-security posture. Direct pushes to the official repository are disabled for all community members. 

### 1. The Branch Hierarchy
- **`main`** — **Production (Strictly Protected)**. Only contains tagged, verified releases (e.g., `v0.1.0`). Direct commits are forbidden.
- **`staging`** — **Release Candidates**. Used for pre-release testing and stabilization. No new features are merged here.
- **`dev`** — **Integration (Default Branch)**. This is the integration point for all new features and bug fixes. All community PRs must target this branch.

### 2. The Contribution Workflow (Fork & PR)
To contribute, you **must** follow the Fork-and-Pull model:
1.  **Fork** the official `ZethraOS/os` repository to your personal GitHub account.
2.  **Clone** your fork locally and create a descriptive branch from `dev` (e.g., `feature/ai-patch-validation`).
3.  **Implement** your changes following our coding standards.
4.  **Push** to your fork.
5.  **Open a Pull Request** from your fork's branch to the official ZethraOS `dev` branch.

### 3. Release Lifecycle
1.  **Development**: Features are merged into `dev` via PRs.
2.  **Staging**: When a release is planned, `dev` is merged into `staging`.
3.  **Testing**: Final verification, stress testing, and version bumping occur on `staging`.
4.  **Ship**: `staging` is merged into `main` and a release is tagged.

## Before Every PR
Ensure your code meets the quality bar:
```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all
```

## Code Standards
- **Naming**: `PascalCase` for types/structs, `snake_case` for functions/files/modules, `SCREAMING_SNAKE` for constants.
- **Modularity**: Any file exceeding 400 lines must be split into sub-modules.
- **Visibility**: Minimize `pub` usage; only expose what is necessary for cross-module integration.

## Commit Message Format
We follow [Conventional Commits](https://www.conventionalcommits.org/):
`type(scope): description`
Types: `feat`, `fix`, `hotfix`, `perf`, `refactor`, `test`, `docs`, `chore`
Example: `feat(sensord): add gyroscope fusion with complementary filter`

Signed-off-by: Your Name <you@example.com>
```

**License requirements:**
- New userspace code → Apache-2.0
- Kernel modules → GPL-2.0
- Do NOT submit code under GPL-3.0 (incompatible with our kernel linking)
- Do NOT copy any ZethraOS or proprietary code, ever

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
| `services/zethrad/` | Rust | Init system — only touch if you really know what you're doing |
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
python3 tools/ci/check_kernel_config.py kernel/zethra_defconfig

# Boot smoke test in QEMU (requires kernel image)
bash build/scripts/qemu_boot.sh

# Full CI locally (requires act)
act -j rust-check
```

---

## Dependency security policy

For production branches, ZethraOS enforces a strict dependency security bar:

1. `cargo audit` vulnerabilities are **blocking** on PR and merge.
2. Security warnings (`unmaintained`, `unsound`, `yanked`) are tracked by CI monitoring artifacts and scheduled drift reports.
3. Weekly dependency hygiene is required (at minimum one lockfile refresh and audit review per week).

### Critical dependency ownership

| Domain | Paths | Owner |
|--------|-------|-------|
| Sandbox runtime | `services/sandbox/` | `@er-mayanka` |
| TLS / HTTP client stack | `services/network/`, `services/ota/`, `ai/*` | `@er-mayanka` |
| Netlink / Linux networking | `services/network/` | `@er-mayanka` |

### Major dependency bump rule

Any major dependency bump in sandboxing, networking, cryptography, or update pipelines requires:

1. Security review sign-off in PR
2. `cargo audit` clean result
3. Compatibility validation via `cargo check --all` and `cargo test --all`

---

## AI-generated patches

The ZethraAI daemon automatically generates patches and opens PRs. These are clearly labelled `[AI-GENERATED]` in the title. Human reviewers should:

1. Verify the root cause analysis makes sense
2. Review the diff for correctness and safety
3. Check that the generated tests actually cover the fix
4. Approve normally if satisfied — the CI pipeline handles the rest

Never blindly merge AI patches without reading them. The AI can be wrong.

---

## Security issues

**Do not open public issues for security vulnerabilities.**
Email `security@zethraos.com` with:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Your suggested fix (optional)

We aim to respond within 48 hours and will credit you in the release notes.

---

## Community

- Matrix: `#zethraos:matrix.org`
- GitHub Discussions for design questions
- GitHub Issues for bugs and feature requests

Thank you for contributing to the future of open mobile computing.
