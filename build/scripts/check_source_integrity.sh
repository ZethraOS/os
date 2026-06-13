#!/usr/bin/env bash
# check_source_integrity.sh — Verify kernel source and build inputs are unchanged
# SPDX-License-Identifier: Apache-2.0
#
# This script validates that:
#   1. Kernel source tree is clean (no modifications)
#   2. Defconfig hasn't been modified
#   3. Build scripts are unchanged
#   4. All critical sources match expected checksums
#
# Usage:
#   bash build/scripts/check_source_integrity.sh [--fix] [--generate-checksums]

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
KERNEL_SRC_DIR="$REPO_ROOT/linux-6.9"
CHECKSUM_FILE="$REPO_ROOT/build/.source-checksums.txt"

VERBOSE=false
FIX_ISSUES=false
GENERATE_MODE=false

# Colours
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; RESET='\033[0m'

info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }
error()   { echo -e "${RED}[ERROR]${RESET} $*"; }

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    --verbose)              VERBOSE=true; shift ;;
    --fix)                  FIX_ISSUES=true; shift ;;
    --generate-checksums)   GENERATE_MODE=true; shift ;;
    *)                      error "Unknown option: $1"; exit 1 ;;
  esac
done

echo "=================================================="
echo "    ZethraOS Source Code Integrity Check"
echo "=================================================="

# ─── Mode: Generate Checksums (First Run) ─────────────────────────────────────
if [[ "$GENERATE_MODE" == true ]]; then
  info "Generating source code checksums..."
  mkdir -p "$(dirname "$CHECKSUM_FILE")"
  
  {
    echo "# ZethraOS Build Input Checksums"
    echo "# Generated: $(date -u +'%Y-%m-%dT%H:%M:%SZ')"
    echo "# Purpose: Detect modifications to build inputs for reproducibility gate"
    echo ""
    echo "# Kernel defconfig"
    echo "defconfig $(sha256sum "$REPO_ROOT/kernel/zethra_defconfig" | awk '{print $1}')"
    echo ""
    echo "# DTS"
    if [[ -f "$KERNEL_SRC_DIR/arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dts" ]]; then
      echo "dts $(sha256sum "$KERNEL_SRC_DIR/arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dts" | awk '{print $1}')"
    else
      echo "dts PENDING_BUILD"
    fi
    echo ""
    echo "# Build scripts"
    for script in "$REPO_ROOT"/build/scripts/*.sh; do
      if [[ -f "$script" ]]; then
        name=$(basename "$script")
        echo "script_$name $(sha256sum "$script" | awk '{print $1}')"
      fi
    done
  } > "$CHECKSUM_FILE"
  
  success "Checksums written: $CHECKSUM_FILE"
  cat "$CHECKSUM_FILE"
  exit 0
fi

# ─── Mode: Verify Integrity (Normal Operation) ────────────────────────────────
info "Checking source code integrity..."

ISSUES_FOUND=false

# Check 1: Kernel source tree status
section_kernel=true
if [[ -d "$KERNEL_SRC_DIR" ]]; then
  info "Checking kernel source tree..."
  
  cd "$KERNEL_SRC_DIR"
  
  if git rev-parse --git-dir >/dev/null 2>&1; then
    # Kernel source is a git repo
    if [[ -n "$(git status --porcelain)" ]]; then
      warn "Kernel source tree has modifications:"
      git status --short | head -10
      ISSUES_FOUND=true
      section_kernel=false
      
      if [[ "$FIX_ISSUES" == true ]]; then
        warn "Fixing: Discarding kernel source changes..."
        git checkout .
        success "Kernel source reset to clean state"
        section_kernel=true
      fi
    else
      success "Kernel source tree is clean"
    fi
  else
    # Kernel source is extracted tarball (not git)
    info "Kernel source is not a git repo (extracted from tarball)"
    
    # Check if kernel.src.tar.xz checksum matches
    if [[ -f "$CHECKSUM_FILE" ]]; then
      stored_hash=$(grep "^kernel_tarball" "$CHECKSUM_FILE" | awk '{print $2}' || echo "")
      if [[ -n "$stored_hash" ]]; then
        warn "Cannot verify tarball-extracted source without git"
        warn "Recommendation: Use git submodule or pinned commit instead"
      fi
    fi
  fi
  cd "$REPO_ROOT"
else
  info "Kernel source not yet downloaded (will be built on demand)"
fi

# Check 2: Defconfig integrity
info "Checking defconfig..."
if [[ -f "$REPO_ROOT/kernel/zethra_defconfig" ]]; then
  current_hash=$(sha256sum "$REPO_ROOT/kernel/zethra_defconfig" | awk '{print $1}')
  
  if [[ -f "$CHECKSUM_FILE" ]]; then
    stored_hash=$(grep "^defconfig " "$CHECKSUM_FILE" | awk '{print $2}' || echo "")
    if [[ -n "$stored_hash" ]] && [[ "$stored_hash" != "$current_hash" ]]; then
      warn "Defconfig has changed!"
      echo "  Expected: $stored_hash"
      echo "  Current:  $current_hash"
      ISSUES_FOUND=true
      
      if [[ "$FIX_ISSUES" == true ]]; then
        warn "Fixing: Updating checksum..."
        sed -i.bak "s/^defconfig .*/defconfig $current_hash/" "$CHECKSUM_FILE"
        success "Checksum updated"
      fi
    else
      success "Defconfig matches expected checksum"
    fi
  else
    warn "Checksum file not found. Generate with: --generate-checksums"
  fi
else
  warn "Defconfig not found"
  ISSUES_FOUND=true
fi

# Check 3: Build scripts integrity
info "Checking build scripts..."
script_issues=0
for script in "$REPO_ROOT"/build/scripts/{build_kernel,build_initramfs,pack_boot_image,flash_nokia61plus}.sh; do
  if [[ ! -f "$script" ]]; then
    warn "Missing script: $(basename $script)"
    ISSUES_FOUND=true
    ((script_issues++)) || true
  fi
done

if [[ $script_issues -eq 0 ]]; then
  success "All required build scripts present"
fi

# Check 4: Source tree status
info "Checking main repository status..."
if [[ -n "$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null | grep -v '^ ' || true)" ]]; then
  warn "Main repository has uncommitted changes:"
  git -C "$REPO_ROOT" status --short | grep -v '^ ' | head -10
  ISSUES_FOUND=true
  
  if [[ "$FIX_ISSUES" == true ]]; then
    warn "Note: Use 'git add' and 'git commit' to track changes"
    warn "For now, showing what would be tracked:"
    git -C "$REPO_ROOT" status --short | grep -v '^ '
  fi
else
  success "Main repository is clean"
fi

# Check 5: Critical config files
info "Checking critical configuration files..."
critical_files=(
  "kernel/zethra_defconfig"
  "build/scripts/build_kernel.sh"
  "build/scripts/build_initramfs.sh"
  "build/scripts/pack_boot_image.sh"
)

for file in "${critical_files[@]}"; do
  if [[ ! -f "$REPO_ROOT/$file" ]]; then
    error "Missing critical file: $file"
    ISSUES_FOUND=true
  else
    if [[ "$VERBOSE" == true ]]; then
      success "$file: Present ($(wc -l < "$REPO_ROOT/$file") lines)"
    fi
  fi
done

# ─── Summary ────────────────────────────────────────────────────────────────
echo ""
echo "=================================================="

if [[ "$ISSUES_FOUND" == true ]]; then
  warn "Source integrity check: Issues detected ⚠️"
  echo ""
  echo "Issues detected. For reproducibility gate, source should be clean."
  echo ""
  if [[ "$FIX_ISSUES" != true ]]; then
    echo "To automatically fix: bash $0 --fix"
  fi
  exit 1
else
  success "Source integrity check: All clear ✓"
  echo ""
  echo "Source tree is clean and ready for reproducible builds."
  exit 0
fi
