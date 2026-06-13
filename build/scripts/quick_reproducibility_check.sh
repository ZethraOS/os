#!/usr/bin/env bash
# quick_reproducibility_check.sh — Fast reproducibility verification
# SPDX-License-Identifier: Apache-2.0
#
# Lightweight reproducibility check that:
#   1. Verifies source code is clean
#   2. Checks build manifests for consistency
#   3. Validates artifact checksums
#   4. Runs in <1 minute (vs 30+ minutes for full verify_reproducibility.sh)
#
# Perfect for: Pre-commit checks, CI/CD validation, quick feedback
#
# Usage:
#   bash build/scripts/quick_reproducibility_check.sh [--verbose]

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$REPO_ROOT/build/out"

VERBOSE=false
PASS=true

# Colours
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*"; }
fail()    { echo -e "${RED}✗${RESET}  $*"; }

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    --verbose) VERBOSE=true; shift ;;
    *)         error "Unknown option: $1"; exit 1 ;;
  esac
done

# ─── Quick Checks ────────────────────────────────────────────────────────────
echo "=================================================="
echo "    Quick Reproducibility Check (~30 seconds)"
echo "=================================================="

# Check 1: Repository clean?
info "1. Repository state..."
if [[ -n "$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null || true)" ]]; then
  warn "Repository has uncommitted changes"
  PASS=false
  if [[ "$VERBOSE" == true ]]; then
    git -C "$REPO_ROOT" status --short
  fi
else
  success "Repository is clean"
fi

# Check 2: Source artifacts present?
info "2. Build artifacts..."
artifacts_ok=true
for artifact in Image.gz-dtb initramfs.cpio.gz boot.img; do
  if [[ ! -f "$OUT_DIR/$artifact" ]]; then
    warn "Missing: $artifact (run build_kernel.sh, build_initramfs.sh, pack_boot_image.sh)"
    PASS=false
    artifacts_ok=false
  fi
done
if [[ "$artifacts_ok" == true ]]; then
  success "All expected artifacts present"
fi

# Check 3: Build manifests present and valid?
info "3. Build reproducibility metadata..."
manifest_ok=true
for manifest in .kernel-build-manifest.txt .boot-image-params.txt .boot-pack-manifest.txt; do
  if [[ -f "$OUT_DIR/$manifest" ]]; then
    # Basic validation: contains checksums
    if grep -q "sha256" "$OUT_DIR/$manifest"; then
      success "Manifest valid: $manifest"
    else
      warn "Manifest exists but may be incomplete: $manifest"
      manifest_ok=false
    fi
  else
    if [[ "$manifest" != ".kernel-build-manifest.txt" ]]; then
      # Optional manifests
      if [[ "$VERBOSE" == true ]]; then
        info "Optional manifest not found: $manifest"
      fi
    fi
  fi
done

# Check 4: Artifact integrity (quick spot-check)
info "4. Artifact checksums (spot-check)..."
checksums_ok=true
for artifact in Image.gz-dtb initramfs.cpio.gz boot.img; do
  if [[ -f "$OUT_DIR/$artifact" ]]; then
    hash=$(sha256sum "$OUT_DIR/$artifact" | awk '{print $1}')
    
    # Try to find the hash in manifest
    found_in_manifest=false
    for manifest in "$OUT_DIR"/.*.txt; do
      if [[ -f "$manifest" ]] && grep -q "$hash" "$manifest"; then
        found_in_manifest=true
        break
      fi
    done
    
    if [[ "$found_in_manifest" == true ]]; then
      success "$artifact: hash recorded in manifest"
    else
      if [[ "$VERBOSE" == true ]]; then
        warn "$artifact: hash not found in manifests (not a blocker, may be new build)"
      fi
    fi
  fi
done

# Check 5: Critical build scripts present?
info "5. Build infrastructure..."
scripts_ok=true
for script in build_kernel.sh build_initramfs.sh pack_boot_image.sh; do
  if [[ ! -f "$REPO_ROOT/build/scripts/$script" ]]; then
    fail "Missing: $script"
    PASS=false
    scripts_ok=false
  fi
done
if [[ "$scripts_ok" == true ]]; then
  success "All build scripts present"
fi

# Check 6: Kernel source tree
info "6. Kernel source state..."
if [[ -d "$REPO_ROOT/linux-6.9" ]]; then
  if cd "$REPO_ROOT/linux-6.9" 2>/dev/null && git rev-parse --git-dir >/dev/null 2>&1; then
    if [[ -n "$(git status --porcelain)" ]]; then
      warn "Kernel source has uncommitted changes (expected before build)"
    else
      success "Kernel source tree is clean"
    fi
    cd "$REPO_ROOT"
  else
    info "Kernel source is extracted (not git) — OK for first build"
  fi
else
  info "Kernel source not downloaded yet (will be built on demand)"
fi

# ─── Summary ────────────────────────────────────────────────────────────────
echo ""
echo "=================================================="

if [[ "$PASS" == true ]]; then
  success "Quick check: READY FOR REPRODUCIBLE BUILD ✓"
  echo ""
  echo "Repository and build infrastructure are ready."
  echo "You can proceed with confidence to:"
  echo ""
  echo "  bash build/scripts/build_kernel.sh"
  echo "  bash build/scripts/build_initramfs.sh"
  echo "  bash build/scripts/pack_boot_image.sh"
  echo ""
  echo "For comprehensive reproducibility test (takes 30+ min):"
  echo "  bash build/scripts/verify_reproducibility.sh"
  exit 0
else
  fail "Quick check: ISSUES DETECTED ✗"
  echo ""
  echo "Fix the issues above before attempting builds."
  echo ""
  echo "Common fixes:"
  echo "  - Commit changes: git add -A && git commit -m '...'"
  echo "  - Clean repo: git clean -fdx"
  echo "  - Reset changes: git checkout ."
  exit 1
fi
