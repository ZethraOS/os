#!/usr/bin/env bash
# build/docker/build_image.sh — One-time build of the zethra-build-env Docker image
# SPDX-License-Identifier: Apache-2.0
#
# Run this ONCE to create the pre-baked build environment.
# Subsequent kernel/initramfs builds use this image and skip all apt-get/rustup steps.
#
# Usage:
#   bash build/docker/build_image.sh              # build with defaults
#   bash build/docker/build_image.sh --push        # build and push to registry
#   RUST_VERSION=1.87.0 bash build/docker/build_image.sh  # pin Rust version
#
# Estimated time: 5-10 minutes (one-time cost)
# Resulting image size: ~1.2 GB
# After this, kernel builds: ~3-5 min (ccache hit), initramfs: ~2-3 min (cargo incremental)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

IMAGE_NAME="zethra-build-env"
IMAGE_TAG="${ZETHRA_BUILD_TAG:-1}"
FULL_IMAGE="${IMAGE_NAME}:${IMAGE_TAG}"
RUST_VERSION="${RUST_VERSION:-stable}"
BUILD_DATE="$(date -u +'%Y-%m-%dT%H:%M:%SZ')"
PUSH=false

GREEN='\033[0;32m'; CYAN='\033[0;36m'; YELLOW='\033[1;33m'; RESET='\033[0m'
info()    { echo -e "${CYAN}==>${RESET} $*"; }
success() { echo -e "${GREEN}✓${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET} $*"; }

for arg in "$@"; do
  [[ "$arg" == "--push" ]] && PUSH=true
done

echo ""
echo "╔══════════════════════════════════════════════════════╗"
echo "║   ZethraOS Build Environment — Docker Image Build   ║"
echo "╚══════════════════════════════════════════════════════╝"
echo ""
info "Image:        $FULL_IMAGE"
info "Rust version: $RUST_VERSION"
info "Build date:   $BUILD_DATE"
echo ""

# Ensure Docker is running
if ! docker info &>/dev/null; then
  echo "Error: Docker is not running. Start Docker Desktop first."
  exit 1
fi

# Check if image already exists
if docker image inspect "$FULL_IMAGE" &>/dev/null; then
  warn "Image $FULL_IMAGE already exists."
  warn "To rebuild: docker rmi $FULL_IMAGE && bash build/docker/build_image.sh"
  warn "Proceeding with existing image (use --force to rebuild)..."
  if [[ "${1:-}" != "--force" ]]; then
    success "Using existing $FULL_IMAGE"
    exit 0
  fi
fi

info "Building $FULL_IMAGE ..."
info "This is a one-time operation (~5-10 minutes). Go get a coffee ☕"
echo ""

BUILD_START="$(date +%s)"

docker build \
  --file "$SCRIPT_DIR/Dockerfile" \
  --tag "$FULL_IMAGE" \
  --tag "${IMAGE_NAME}:latest" \
  --build-arg "RUST_VERSION=${RUST_VERSION}" \
  --build-arg "BUILD_DATE=${BUILD_DATE}" \
  --progress=plain \
  "$REPO_ROOT"

BUILD_END="$(date +%s)"
BUILD_ELAPSED=$((BUILD_END - BUILD_START))

echo ""
success "Image built in ${BUILD_ELAPSED}s: $FULL_IMAGE"
echo ""
info "Image details:"
docker image inspect "$FULL_IMAGE" --format \
  "  Size:    {{.Size | printf \"%.0f\"}} bytes
  Created: {{.Created}}
  ID:      {{.Id | printf \"%.12s\"}}"

# Record the image SHA digest for supply-chain auditability
IMAGE_DIGEST="$(docker inspect "$FULL_IMAGE" --format '{{.Id}}')"
DIGEST_FILE="$REPO_ROOT/build/docker/.image-digest"
echo "$IMAGE_DIGEST" > "$DIGEST_FILE"
success "Image digest recorded: $DIGEST_FILE"
echo ""

if [[ "$PUSH" == "true" ]]; then
  info "Pushing $FULL_IMAGE ..."
  docker push "$FULL_IMAGE"
  success "Pushed: $FULL_IMAGE"
fi

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Setup Complete — Next Steps                               ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "  1. Build initramfs:  bash build/scripts/build_initramfs.sh"
echo "  2. Run Experiment 1: bash build/scripts/run_experiment.sh --image 01-headless"
echo ""
echo "  Cache locations (auto-created on first run):"
echo "    Cargo registry: ~/.cargo/registry  (crates, never re-downloaded)"
echo "    Cargo git deps: ~/.cargo/git"
echo "    ccache:         ~/.ccache          (kernel compiler cache)"
echo "    Cargo target:   <repo>/target      (incremental Rust builds)"
echo ""
