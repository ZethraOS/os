#!/usr/bin/env bash
# verify_reproducibility.sh — Automated reproducibility verification for ZethraOS builds
# SPDX-License-Identifier: Apache-2.0
#
# This script:
#   1. Performs 2 consecutive builds with identical inputs
#   2. Compares all build artifacts and manifests
#   3. Detects non-determinism sources
#   4. Generates a reproducibility report
#
# Usage:
#   bash build/scripts/verify_reproducibility.sh [--verbose] [--preserve] [--quick]
#
# Output:
#   build/out/.reproducibility-report.txt
#   build/out/.reproducibility-errors.txt (if issues found)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_DIR="$REPO_ROOT/build"
OUT_DIR="$BUILD_DIR/out"
SCRIPTS_DIR="$BUILD_DIR/scripts"

# Build directories for comparison
BUILD1_DIR=$(mktemp -d)
BUILD2_DIR=$(mktemp -d)
REPORT_FILE="$OUT_DIR/.reproducibility-report.txt"
ERRORS_FILE="$OUT_DIR/.reproducibility-errors.txt"

VERBOSE=false
PRESERVE=false
QUICK_MODE=false

# Colours
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*"; }
fail()    { echo -e "${RED}✗${RESET}  $*"; }
section() { echo -e "\n${BOLD}${CYAN}$*${RESET}"; }

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    --verbose)   VERBOSE=true; shift ;;
    --preserve)  PRESERVE=true; shift ;;
    --quick)     QUICK_MODE=true; shift ;;
    *)           error "Unknown option: $1"; exit 1 ;;
  esac
done

# Cleanup function
cleanup() {
  if [[ "$PRESERVE" != true ]]; then
    rm -rf "$BUILD1_DIR" "$BUILD2_DIR"
  else
    info "Preserved build directories for inspection:"
    echo "  Build 1: $BUILD1_DIR"
    echo "  Build 2: $BUILD2_DIR"
  fi
}
trap cleanup EXIT

# ─── Header ───────────────────────────────────────────────────────────────────
echo "=================================================="
echo "    ZethraOS Reproducibility Verification"
echo "=================================================="
echo "Device:      Nokia 6.1 Plus (TA-1103) / SDM636"
echo "Date:        $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
echo "Repo:        $REPO_ROOT"
echo ""

# ─── Pre-flight Checks ─────────────────────────────────────────────────────────
section "Pre-Flight Checks"

info "Verifying repository state..."

# Check for uncommitted changes (will cause non-reproducibility)
if [[ -n "$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null || true)" ]]; then
  warn "Repository has uncommitted changes (will cause non-determinism)"
  if [[ "$VERBOSE" == true ]]; then
    git -C "$REPO_ROOT" status --short
  fi
fi
success "Repository state checked"

# Check for required tools
for tool in git sha256sum; do
  if ! command -v "$tool" &>/dev/null; then
    error "Required tool not found: $tool"
    exit 1
  fi
done
success "Required tools available"

# ─── Build 1 ──────────────────────────────────────────────────────────────────
section "Build 1: Initial Compilation"

info "Preparing first build..."
mkdir -p "$BUILD1_DIR"

# Record environment
{
  echo "# Build 1 Environment"
  echo "timestamp:      $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo "git_commit:     $(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo 'unknown')"
  echo "hostname:       $(hostname)"
  echo "uname:          $(uname -s) $(uname -m)"
  if command -v gcc &>/dev/null; then
    echo "gcc_version:    $(gcc --version | head -1)"
  fi
  if command -v aarch64-linux-gnu-gcc &>/dev/null; then
    echo "cross_gcc:      $(aarch64-linux-gnu-gcc --version | head -1)"
  fi
  if command -v rustc &>/dev/null; then
    echo "rustc_version:  $(rustc --version)"
  fi
} > "$BUILD1_DIR/build_env.txt"

success "Build 1 environment recorded"

info "Running build 1 (this may take 15-30 minutes)..."
if [[ "$VERBOSE" == true ]]; then
  bash "$SCRIPTS_DIR/build_kernel.sh" --pin-check 2>&1 | tee "$BUILD1_DIR/build.log"
  bash "$SCRIPTS_DIR/build_initramfs.sh" 2>&1 | tee -a "$BUILD1_DIR/build.log"
  bash "$SCRIPTS_DIR/pack_boot_image.sh" 2>&1 | tee -a "$BUILD1_DIR/build.log"
else
  bash "$SCRIPTS_DIR/build_kernel.sh" --pin-check >"$BUILD1_DIR/kernel.log" 2>&1
  bash "$SCRIPTS_DIR/build_initramfs.sh" >"$BUILD1_DIR/initramfs.log" 2>&1
  bash "$SCRIPTS_DIR/pack_boot_image.sh" >"$BUILD1_DIR/pack.log" 2>&1
fi

# Copy artifacts
cp "$OUT_DIR"/Image.gz-dtb "$BUILD1_DIR/" 2>/dev/null || true
cp "$OUT_DIR"/*.dtb "$BUILD1_DIR/" 2>/dev/null || true
cp "$OUT_DIR"/initramfs.cpio.gz "$BUILD1_DIR/" 2>/dev/null || true
cp "$OUT_DIR"/boot.img "$BUILD1_DIR/" 2>/dev/null || true
cp "$OUT_DIR"/.*.txt "$BUILD1_DIR/" 2>/dev/null || true

success "Build 1 complete"
info "Build 1 artifacts:"
ls -lh "$BUILD1_DIR"/ | grep -E "Image|initramfs|boot|\.txt" | awk '{print "  " $9 " (" $5 ")"}'

# ─── Clean Between Builds ────────────────────────────────────────────────────
section "Cleaning for Build 2"

info "Removing build artifacts and kernel source..."
rm -rf "$REPO_ROOT/linux-6.9" "$OUT_DIR"/*

if [[ "$QUICK_MODE" == true ]]; then
  info "Quick mode: skipping full clean (keeping system dependencies)"
else
  # Full clean
  if [[ -d "$REPO_ROOT/target" ]]; then
    rm -rf "$REPO_ROOT/target"
  fi
fi

success "Clean complete"

# Wait before second build to ensure timestamps differ if embedded
sleep 2

# ─── Build 2 ──────────────────────────────────────────────────────────────────
section "Build 2: Reproduction Attempt"

info "Preparing second build..."
mkdir -p "$BUILD2_DIR"

# Record environment
{
  echo "# Build 2 Environment"
  echo "timestamp:      $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo "git_commit:     $(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo 'unknown')"
  echo "hostname:       $(hostname)"
  echo "uname:          $(uname -s) $(uname -m)"
} > "$BUILD2_DIR/build_env.txt"

success "Build 2 environment recorded"

info "Running build 2 (this may take 15-30 minutes)..."
if [[ "$VERBOSE" == true ]]; then
  bash "$SCRIPTS_DIR/build_kernel.sh" --pin-check 2>&1 | tee "$BUILD2_DIR/build.log"
  bash "$SCRIPTS_DIR/build_initramfs.sh" 2>&1 | tee -a "$BUILD2_DIR/build.log"
  bash "$SCRIPTS_DIR/pack_boot_image.sh" 2>&1 | tee -a "$BUILD2_DIR/build.log"
else
  bash "$SCRIPTS_DIR/build_kernel.sh" --pin-check >"$BUILD2_DIR/kernel.log" 2>&1
  bash "$SCRIPTS_DIR/build_initramfs.sh" >"$BUILD2_DIR/initramfs.log" 2>&1
  bash "$SCRIPTS_DIR/pack_boot_image.sh" >"$BUILD2_DIR/pack.log" 2>&1
fi

# Copy artifacts
cp "$OUT_DIR"/Image.gz-dtb "$BUILD2_DIR/" 2>/dev/null || true
cp "$OUT_DIR"/*.dtb "$BUILD2_DIR/" 2>/dev/null || true
cp "$OUT_DIR"/initramfs.cpio.gz "$BUILD2_DIR/" 2>/dev/null || true
cp "$OUT_DIR"/boot.img "$BUILD2_DIR/" 2>/dev/null || true
cp "$OUT_DIR"/.*.txt "$BUILD2_DIR/" 2>/dev/null || true

success "Build 2 complete"
info "Build 2 artifacts:"
ls -lh "$BUILD2_DIR"/ | grep -E "Image|initramfs|boot|\.txt" | awk '{print "  " $9 " (" $5 ")"}'

# ─── Reproducibility Analysis ────────────────────────────────────────────────
section "Reproducibility Analysis"

# Compare artifacts
REPRODUCE_PASS=true
DIFFS_FOUND=()

info "Comparing artifacts..."

# Function to compare files
compare_files() {
  local file1="$1"
  local file2="$2"
  local name="$3"
  
  if [[ ! -f "$file1" ]]; then
    warn "Build 1 artifact missing: $name"
    DIFFS_FOUND+=("Missing in Build 1: $name")
    return 1
  fi
  
  if [[ ! -f "$file2" ]]; then
    warn "Build 2 artifact missing: $name"
    DIFFS_FOUND+=("Missing in Build 2: $name")
    return 1
  fi
  
  local hash1=$(sha256sum "$file1" | awk '{print $1}')
  local hash2=$(sha256sum "$file2" | awk '{print $1}')
  
  if [[ "$hash1" == "$hash2" ]]; then
    success "$name: MATCH"
    echo "  $name: $hash1"
    return 0
  else
    fail "$name: MISMATCH"
    echo "  Build 1: $hash1"
    echo "  Build 2: $hash2"
    DIFFS_FOUND+=("Hash mismatch: $name")
    REPRODUCE_PASS=false
    return 1
  fi
}

# Compare each artifact
compare_files "$BUILD1_DIR/Image.gz-dtb" "$BUILD2_DIR/Image.gz-dtb" "Image.gz-dtb"
compare_files "$BUILD1_DIR/initramfs.cpio.gz" "$BUILD2_DIR/initramfs.cpio.gz" "initramfs.cpio.gz"
compare_files "$BUILD1_DIR/boot.img" "$BUILD2_DIR/boot.img" "boot.img"

# Compare manifests (if present)
if [[ -f "$BUILD1_DIR/.kernel-build-manifest.txt" ]] && [[ -f "$BUILD2_DIR/.kernel-build-manifest.txt" ]]; then
  info "Analyzing build manifests..."
  
  # Extract key info
  echo "Build 1 kernel manifest:"
  grep "sha256" "$BUILD1_DIR/.kernel-build-manifest.txt" | head -3
  echo "Build 2 kernel manifest:"
  grep "sha256" "$BUILD2_DIR/.kernel-build-manifest.txt" | head -3
fi

# ─── Detailed Difference Analysis ──────────────────────────────────────────────
section "Difference Analysis"

if [[ "$REPRODUCE_PASS" == true ]]; then
  success "All artifacts match! Build is reproducible ✓"
else
  warn "Artifacts differ. Investigating sources of non-determinism..."
  
  # Check build logs for warnings
  info "Checking for compiler/build warnings..."
  for log in "$BUILD1_DIR"/kernel.log "$BUILD2_DIR"/kernel.log; do
    if [[ -f "$log" ]]; then
      count=$(grep -c "warning:" "$log" || true)
      if [[ $count -gt 0 ]]; then
        warn "Found $count compiler warnings in $(basename $(dirname $log)):"
        grep "warning:" "$log" | head -3
      fi
    fi
  done
  
  # Attempt binary diff
  info "Attempting binary diff (first 50 differences)..."
  if command -v hexdump &>/dev/null; then
    (
      cmp -l "$BUILD1_DIR/Image.gz-dtb" "$BUILD2_DIR/Image.gz-dtb" 2>/dev/null || true
    ) | head -50 | awk '{printf "  Byte %d: %s vs %s\n", $1, $2, $3}'
  fi
fi

# ─── Generate Report ──────────────────────────────────────────────────────────
section "Generating Report"

{
  echo "# ZethraOS Build Reproducibility Report"
  echo "Generated: $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
  echo ""
  
  if [[ "$REPRODUCE_PASS" == true ]]; then
    echo "## ✓ REPRODUCIBLE"
    echo ""
    echo "Consecutive builds produced **byte-identical** artifacts:"
    echo ""
    echo "| Artifact | Hash |"
    echo "|----------|------|"
    for artifact in Image.gz-dtb initramfs.cpio.gz boot.img; do
      if [[ -f "$BUILD1_DIR/$artifact" ]]; then
        hash=$(sha256sum "$BUILD1_DIR/$artifact" | awk '{print $1}')
        echo "| $artifact | \`$hash\` |"
      fi
    done
    echo ""
    echo "**Implications**: The build process is deterministic. Any changes to output must"
    echo "come from source code changes (kernel, DTS, defconfig, initramfs). This gate"
    echo "is **PASSED** and provides strong evidence for Gate 0 reproducibility."
    echo ""
  else
    echo "## ✗ NON-REPRODUCIBLE"
    echo ""
    echo "Builds produced **different** artifacts despite identical inputs."
    echo ""
    echo "### Differences Found"
    echo ""
    for diff in "${DIFFS_FOUND[@]}"; do
      echo "- $diff"
    done
    echo ""
    echo "### Common Causes"
    echo ""
    echo "1. **Timestamps embedded in binaries** (e.g., __DATE__, __TIME__)"
    echo "   - Fix: Use SOURCE_DATE_EPOCH or compiler flags to disable timestamps"
    echo "   - Command: \`SOURCE_DATE_EPOCH=0 make ...\`"
    echo ""
    echo "2. **Non-deterministic file ordering** (in archives/cpio)"
    echo "   - Fix: Sort file lists before packing"
    echo ""
    echo "3. **Compiler version differences** (between rebuilds or machines)"
    echo "   - Check: \`gcc --version\`, \`aarch64-linux-gnu-gcc --version\`"
    echo ""
    echo "4. **Kernel source tree not clean**"
    echo "   - Check: \`git -C linux-6.9 status\`"
    echo "   - Fix: Delete linux-6.9/ and rebuild"
    echo ""
    echo "5. **Uncommitted source changes in main repo**"
    echo "   - Check: \`git status\`"
    echo ""
    echo "### Investigation Steps"
    echo ""
    echo "1. Review build logs for non-deterministic output:"
    echo "   \`\`\`"
    echo "   diff <(grep -v 'timestamp\|date' '$BUILD1_DIR/kernel.log') \\"
    echo "        <(grep -v 'timestamp\|date' '$BUILD2_DIR/kernel.log')"
    echo "   \`\`\`"
    echo ""
    echo "2. Check for embedded timestamps:"
    echo "   \`\`\`"
    echo "   strings '$BUILD1_DIR/Image.gz-dtb' | grep '202[0-9]-'"
    echo "   \`\`\`"
    echo ""
    echo "3. Verify source files haven't changed:"
    echo "   \`\`\`"
    echo "   diff '$BUILD1_DIR/.kernel-build-manifest.txt' \\"
    echo "        '$BUILD2_DIR/.kernel-build-manifest.txt'"
    echo "   \`\`\`"
    echo ""
    echo "### Next Steps"
    echo ""
    echo "This is **not a failure**. Many builds are non-deterministic by design."
    echo "For production hardening, investigate the root cause and use determinism"
    echo "techniques (date freezing, sorted archives, etc.)."
    echo ""
  fi
  
  echo "---"
  echo ""
  echo "## Build Environment"
  echo ""
  echo "### Build 1"
  cat "$BUILD1_DIR/build_env.txt" | sed 's/^/  /'
  echo ""
  echo "### Build 2"
  cat "$BUILD2_DIR/build_env.txt" | sed 's/^/  /'
  echo ""
  
  echo "## Artifact Sizes"
  echo ""
  echo "| Artifact | Build 1 | Build 2 |"
  echo "|----------|---------|---------|"
  for artifact in Image.gz-dtb initramfs.cpio.gz boot.img; do
    if [[ -f "$BUILD1_DIR/$artifact" ]]; then
      size1=$(du -h "$BUILD1_DIR/$artifact" | awk '{print $1}')
      size2=$(du -h "$BUILD2_DIR/$artifact" | awk '{print $1}')
      echo "| $artifact | $size1 | $size2 |"
    fi
  done
  echo ""
  
  echo "## Test Result"
  if [[ "$REPRODUCE_PASS" == true ]]; then
    echo "**Gate 0 (Reproducibility): PASSED ✓**"
  else
    echo "**Gate 0 (Reproducibility): NEEDS INVESTIGATION ⚠️**"
  fi
  
} > "$REPORT_FILE"

success "Report written: $REPORT_FILE"

# Print report summary
echo ""
section "Summary"
head -30 "$REPORT_FILE" | tail -20

# ─── Exit Status ──────────────────────────────────────────────────────────────
if [[ "$REPRODUCE_PASS" == true ]]; then
  success "✓ Reproducibility verification PASSED"
  exit 0
else
  warn "⚠️  Build is non-deterministic (see report for details)"
  exit 1
fi
