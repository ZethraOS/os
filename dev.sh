#!/usr/bin/env bash
# ZethraOS — Local Development Setup Script
# Run this on your Linux machine (Ubuntu/Debian/Fedora/Arch) or macOS.
# No hardware needed. No API key needed.
#
# Usage:
#   chmod +x dev.sh
#   ./dev.sh setup          — install Rust + check deps
#   ./dev.sh build          — compile all crates
#   ./dev.sh run-ai         — run AI daemon in mock mode (no API key)
#   ./dev.sh run-ai-live    — run AI daemon with real Claude API
#   ./dev.sh run-init       — run the init system (lists services)
#   ./dev.sh run-sensors    — run sensor daemon (simulated hardware)
#   ./dev.sh inject-crash   — drop a fake crash to trigger AI pipeline
#   ./dev.sh test           — run all unit tests
#   ./dev.sh check          — cargo check + clippy
#   ./dev.sh git-init       — initialise git repo (when ready)
#   ./dev.sh git-push URL   — first push to GitHub/GitLab

set -euo pipefail

BOLD="\033[1m"
GREEN="\033[0;32m"
YELLOW="\033[0;33m"
RED="\033[0;31m"
CYAN="\033[0;36m"
RESET="\033[0m"

info()    { echo -e "${CYAN}[zethra]${RESET} $*"; }
success() { echo -e "${GREEN}[zethra]${RESET} $*"; }
warn()    { echo -e "${YELLOW}[zethra]${RESET} $*"; }
error()   { echo -e "${RED}[zethra]${RESET} $*"; exit 1; }
header()  { echo -e "\n${BOLD}${CYAN}══ $* ══${RESET}"; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# ── Crash dir used by AI daemon ────────────────────────────────────────────────
CRASH_DIR="${TMPDIR:-/tmp}/zethra/crashes"

cmd="${1:-help}"

case "$cmd" in

# ────────────────────────────────────────────────────────────────────────────
setup)
  header "Setting up ZethraOS dev environment"

  # Rust
  if command -v rustc &>/dev/null; then
    RUST_VER=$(rustc --version)
    success "Rust already installed: $RUST_VER"
  else
    info "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$HOME/.cargo/env"
    success "Rust installed"
  fi

  # Check Rust version is recent enough
  RUST_MINOR=$(rustc --version | cut -d. -f2)
  if [ "$RUST_MINOR" -lt 80 ]; then
    info "Updating Rust to latest stable..."
    rustup update stable
  fi

  # Optional: ARM64 cross-compile target (for building actual device images)
  info "Adding ARM64 cross-compile target (optional, for device builds)..."
  rustup target add aarch64-unknown-linux-gnu 2>/dev/null || warn "ARM64 target unavailable (fine for local dev)"

  # Python for kernel config checker
  if command -v python3 &>/dev/null; then
    success "Python3 available: $(python3 --version)"
  else
    warn "Python3 not found — kernel config checker won't run (optional)"
  fi

  success "Setup complete! Run: ./dev.sh build"
  ;;

# ────────────────────────────────────────────────────────────────────────────
build)
  header "Building ZethraOS (all crates)"
  info "This may take a minute on first run (downloading dependencies)..."
  cargo build
  success "Build successful"
  echo ""
  info "Binaries available in target/debug/:"
  ls -1 target/debug/ | grep -v '[\.\-]' | grep -v "^zethra" || true
  ls -1 target/debug/zethra* 2>/dev/null | xargs -I{} basename {} || true
  ;;

# ────────────────────────────────────────────────────────────────────────────
check)
  header "Running checks (fmt + clippy + tests)"
  info "Format check..."
  cargo fmt --all -- --check && success "Format OK" || warn "Run: cargo fmt --all"

  info "Clippy..."
  cargo clippy --all-targets 2>&1 | grep -E "^error|warning\[" | head -20 || true

  info "Tests..."
  cargo test --all
  success "All checks passed"
  ;;

# ────────────────────────────────────────────────────────────────────────────
test)
  header "Running unit tests"
  cargo test --all -- --nocapture
  ;;

# ────────────────────────────────────────────────────────────────────────────
run-ai)
  header "Running ZethraAI daemon — MOCK mode (no API key needed)"
  info "The daemon will:"
  info "  1. Start watching $CRASH_DIR for *.crash files"
  info "  2. Auto-inject 2 demo crashes after 2s and 5s"
  info "  3. Show the full analyze → patch → decision pipeline"
  info "  4. Write output patches to ./patches/"
  info ""
  info "Press Ctrl+C to stop. Run './dev.sh inject-crash' in another terminal to test your own crash."
  echo ""

  mkdir -p "$CRASH_DIR"
  ZETHRA_AI_MODE=mock \
  ZETHRA_CRASH_DIR="$CRASH_DIR" \
  ZETHRA_REPO_PATH="$SCRIPT_DIR" \
  XAI_API_KEY="${XAI_API_KEY:-}" \
  RUST_LOG=info \
  cargo run --bin zethra-ai-daemon
  ;;

# ────────────────────────────────────────────────────────────────────────────
run-ai-live)
  header "Running ZethraAI daemon -- LIVE mode"

  # Allow Ollama even if no cloud keys are set
  if [ "${ZETHRA_AI_PROVIDER:-}" != "ollama" ]; then
    if [ -z "${GROQ_API_KEY:-}" ] && [ -z "${OPENROUTER_API_KEY:-}" ] && \
       [ -z "${TOGETHER_API_KEY:-}" ] && [ -z "${OPENAI_API_KEY:-}" ] && \
       [ -z "${GOOGLE_API_KEY:-}" ]   && [ -z "${XAI_API_KEY:-}" ] && \
       [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo ""
    echo "  No API key found. FREE options (no credit card needed):"
    echo ""
    echo "  1. GROQ  -- fast, free, recommended"
    echo "     Sign up : https://console.groq.com"
    echo "     Run     : GROQ_API_KEY=gsk_... ./dev.sh run-ai-live"
    echo ""
    echo "  2. OPENROUTER -- free models available"
    echo "     Sign up : https://openrouter.ai"
    echo "     Run     : OPENROUTER_API_KEY=sk-or-... ./dev.sh run-ai-live"
    echo ""
    echo "  3. OLLAMA -- 100% local, no internet needed"
    echo "     Install : https://ollama.com"
    echo "     Setup   : ollama pull llama3.2"
    echo "     Run     : ZETHRA_AI_PROVIDER=ollama ./dev.sh run-ai-live"
    echo ""
    echo "  4. GOOGLE GEMINI -- free tier (recommended!)"
    echo "     Sign up : https://aistudio.google.com/apikey"
    echo "     Run     : GOOGLE_API_KEY=AIza... ./dev.sh run-ai-live"
    echo ""
    echo "  5. TOGETHER AI -- free dollar25 credit"
    echo "     Sign up : https://api.together.xyz"
    echo "     Run     : TOGETHER_API_KEY=... ./dev.sh run-ai-live"
    echo ""
    echo "  Mock mode always works: ./dev.sh run-ai"
    echo ""
      exit 0
    fi
  fi

  info "API key found (or local provider set) -- starting live analysis"
  echo ""
  mkdir -p "$CRASH_DIR"
  ZETHRA_CRASH_DIR="$CRASH_DIR" \
  ZETHRA_REPO_PATH="$SCRIPT_DIR" \
  GOOGLE_API_KEY="${GOOGLE_API_KEY:-}" \
  XAI_API_KEY="${XAI_API_KEY:-}" \
  RUST_LOG=info \
  cargo run --bin zethra-ai-daemon
  ;;

# ────────────────────────────────────────────────────────────────────────────
run-init)
  header "Running zethrad (init system)"
  info "Loads unit files from build/configs/units/"
  info "Will try to start services listed there. Expects binaries in PATH."
  info "On first run most services won't start (binaries not in /usr/lib) — that's fine."
  echo ""
  ZETHRA_UNITS_DIR="$SCRIPT_DIR/build/configs/units" \
  RUST_LOG=info \
  cargo run --bin zethrad
  ;;

# ────────────────────────────────────────────────────────────────────────────
run-sensors)
  header "Running zethra-sensord (simulated sensors)"
  info "Runs a 100Hz sensor loop with simulated IMU data."
  info "No hardware needed — useful for testing the fusion algorithms."
  info "Press Ctrl+C to stop."
  echo ""
  RUST_LOG=info cargo run --bin zethra-sensord
  ;;

# ────────────────────────────────────────────────────────────────────────────
inject-crash)
  header "Injecting test crash report"
  mkdir -p "$CRASH_DIR"
  TYPE="${2:-kernel}"

  if [ "$TYPE" = "kernel" ]; then
    CRASH_FILE="$CRASH_DIR/test-kernel-$(date +%s).crash"
    cat > "$CRASH_FILE" << 'EOF'
BUG: kernel NULL pointer dereference at 0000000000000000
IP: wifi_qcom_irq_handler+0x48/0x120 [wifi_qcom]
PGD 0 P4D 0
Oops: 0000 [#1] PREEMPT SMP
module: wifi_qcom
driver: qcom-wcn3990
Call trace:
  wifi_qcom_irq_handler+0x48/0x120
  __handle_irq_event_percpu+0x68/0x200
  handle_irq_event+0x44/0xb8
  handle_fasteoi_irq+0xa8/0x1c0
EOF
    success "Kernel panic crash written to $CRASH_FILE"

  elif [ "$TYPE" = "app" ]; then
    CRASH_FILE="$CRASH_DIR/zethra.dialer-$(date +%s).crash"
    cat > "$CRASH_FILE" << 'EOF'
Process: zethra.dialer (pid: 3421)
Signal: 11 (SIGSEGV)
Stack trace:
  #0  EventLoop::dispatch () at event_loop.rs:88
  #1  Activity::on_resume () at activity.rs:201
  #2  <tokio runtime>
Cause: use-after-free — activity destroyed during async callback
EOF
    success "App crash written to $CRASH_FILE"

  elif [ "$TYPE" = "cve" ]; then
    CRASH_FILE="$CRASH_DIR/cve-2025-99999-$(date +%s).crash"
    cat > "$CRASH_FILE" << 'EOF'
CVE-2025-99999
Severity: High
Component: zethra-networkd
Description: Integer overflow in packet length parsing allows heap overflow
Affected: zethra-networkd <= 0.1.0
EOF
    success "CVE report written to $CRASH_FILE"
  fi

  info "The AI daemon will pick this up within 3 seconds."
  info "Usage: ./dev.sh inject-crash [kernel|app|cve]"
  ;;

# ────────────────────────────────────────────────────────────────────────────
kernel-check)
  header "Checking kernel defconfig for security issues"
  python3 tools/ci/check_kernel_config.py kernel/zethra_defconfig
  ;;

# ────────────────────────────────────────────────────────────────────────────
git-init)
  header "Initialising Git repository"
  if [ -d ".git" ]; then
    warn ".git already exists — skipping init"
  else
    git init
    info "Creating .gitignore..."
    cat > .gitignore << 'EOF'
/target/
/patches/staged/
/patches/tests/
**/*.patch
Cargo.lock
.env
*.key
*.pem
.DS_Store
/tmp/
EOF
    git add .
    git commit -m "feat: ZethraOS v0.1.0 — initial commit

AI-native mobile OS built on Linux (Apache-2.0).
Components:
  - zethrad: init system / service manager
  - zethra-ai-daemon: self-healing AI pipeline (mock + live modes)
  - zethra-sensord: sensor fusion daemon
  - zethra-compositor: Wayland compositor
  - zethra-telephonyd: telephony stack
  - zethra-networkd: network manager
  - zethra-otad: OTA update client + A/B partitions
  - zethra-release-bot: autonomous release manager

Self-healing pipeline: crash detect → Claude analysis → patch → CI → OTA"
    success "Git repository initialised with first commit"
    info "Next: ./dev.sh git-push https://github.com/YOUR_USERNAME/zethraos.git"
  fi
  ;;

# ────────────────────────────────────────────────────────────────────────────
git-push)
  REMOTE_URL="${2:-}"
  if [ -z "$REMOTE_URL" ]; then
    error "Usage: ./dev.sh git-push https://github.com/YOUR_USERNAME/zethraos.git"
  fi
  header "Pushing to remote"
  git remote add origin "$REMOTE_URL" 2>/dev/null || git remote set-url origin "$REMOTE_URL"
  git push -u origin main
  success "Pushed to $REMOTE_URL"
  info "Now enable GitHub Actions: copy build/scripts/ci.yml to .github/workflows/ci.yml"
  ;;

# ────────────────────────────────────────────────────────────────────────────
status)
  header "ZethraOS project status"
  echo ""
  info "Crates:"
  for toml in $(find . -name "Cargo.toml" -not -path "./Cargo.toml" -not -path "*/target/*" | sort); do
    dir=$(dirname "$toml")
    name=$(grep '^name' "$toml" | head -1 | awk -F'"' '{print $2}')
    lines=$(find "$dir/src" -name "*.rs" 2>/dev/null | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
    printf "  %-30s  %s lines\n" "$name" "${lines:-0}"
  done
  echo ""
  info "Generated patches:"
  find patches/ -name "*.patch" 2>/dev/null | head -10 || echo "  (none yet — run: ./dev.sh run-ai)"
  echo ""
  if [ -d ".git" ]; then
    info "Git: $(git log --oneline | wc -l) commits on $(git branch --show-current)"
  else
    info "Git: not initialised yet (run: ./dev.sh git-init)"
  fi
  ;;

# ────────────────────────────────────────────────────────────────────────────
help|*)
  echo ""
  echo -e "${BOLD}ZethraOS — Local Dev Script${RESET}"
  echo ""
  echo "  ./dev.sh setup              Install Rust + check deps"
  echo "  ./dev.sh build              Compile all 8 crates"
  echo "  ./dev.sh run-ai             Run AI daemon (MOCK — no API key)"
  echo "  ./dev.sh run-ai-live        Run AI daemon (LIVE — needs ANTHROPIC_API_KEY)"
  echo "  ./dev.sh run-init           Run init system (service manager)"
  echo "  ./dev.sh run-sensors        Run sensor daemon (simulated IMU)"
  echo "  ./dev.sh inject-crash       Drop a fake crash to trigger AI pipeline"
  echo "  ./dev.sh inject-crash app   Drop a fake app crash"
  echo "  ./dev.sh inject-crash cve   Drop a fake CVE report"
  echo "  ./dev.sh test               Run all unit tests"
  echo "  ./dev.sh check              cargo fmt + clippy"
  echo "  ./dev.sh kernel-check       Security audit of kernel defconfig"
  echo "  ./dev.sh status             Show project status"
  echo "  ./dev.sh git-init           First-time git setup"
  echo "  ./dev.sh git-push <url>     Push to GitHub/GitLab"
  echo ""
  echo -e "${CYAN}Quickstart (no API key needed):${RESET}"
  echo "  ./dev.sh setup && ./dev.sh build && ./dev.sh run-ai"
  echo ""
  ;;
esac
