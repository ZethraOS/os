#!/usr/bin/env bash
# build_ota.sh — AetherOS OTA package builder
# SPDX-License-Identifier: Apache-2.0
#
# Usage:
#   bash build_ota.sh --version 0.3.1 --channel dev --sign-key $KEY
#
# Produces: dist/aetheros-<version>-<channel>-<arch>.zip
# Structure of the OTA zip (A/B update format):
#   payload.bin         — delta/full update payload (bsdiff format)
#   payload_properties.txt
#   META-INF/
#     com/google/android/update-binary  — (replaced by aether-update-engine)
#     com/google/android/updater-script — empty; engine handles update
#   aether-manifest.json — our metadata (version, sha256, required_version)

set -euo pipefail

VERSION=""
CHANNEL="dev"
SIGN_KEY=""
ARCH="arm64"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST_DIR="$REPO_ROOT/dist"
WORK_DIR=$(mktemp -d)

# ─── Argument parsing ─────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)   VERSION="$2";   shift 2 ;;
    --channel)   CHANNEL="$2";   shift 2 ;;
    --sign-key)  SIGN_KEY="$2";  shift 2 ;;
    --arch)      ARCH="$2";      shift 2 ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

[[ -z "$VERSION" ]] && { echo "ERROR: --version required"; exit 1; }

FILENAME="aetheros-${VERSION}-${CHANNEL}-${ARCH}.zip"
OUTPUT="$DIST_DIR/$FILENAME"

echo "==> AetherOS OTA build: $VERSION ($CHANNEL/$ARCH)"
mkdir -p "$DIST_DIR" "$WORK_DIR/META-INF/com/aether"

# ─── 1. Copy built artifacts ──────────────────────────────────────────────────
echo "--> Copying kernel image..."
cp "$REPO_ROOT/build/out/Image.gz" "$WORK_DIR/Image.gz" 2>/dev/null || {
  echo "    [WARN] kernel image not found — skipping (dev mode)"
  echo "STUB_KERNEL" > "$WORK_DIR/Image.gz"
}

echo "--> Copying userspace binaries..."
for bin in aetherd aether-telephonyd aether-networkd aether-ai-daemon aether-compositor aether-release-bot; do
  src="$REPO_ROOT/target/aarch64-unknown-linux-gnu/release/$bin"
  if [[ -f "$src" ]]; then
    cp "$src" "$WORK_DIR/$bin"
  else
    echo "    [WARN] $bin not found — using stub"
    echo "STUB" > "$WORK_DIR/$bin"
  fi
done

# ─── 2. Write manifest ────────────────────────────────────────────────────────
echo "--> Writing manifest..."
KERNEL_SHA=$(sha256sum "$WORK_DIR/Image.gz" | awk '{print $1}')
BUILD_TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
GIT_COMMIT=$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo "unknown")

cat > "$WORK_DIR/aether-manifest.json" << EOF
{
  "version": "$VERSION",
  "channel": "$CHANNEL",
  "arch": "$ARCH",
  "built_at": "$BUILD_TIMESTAMP",
  "git_commit": "$GIT_COMMIT",
  "kernel_sha256": "$KERNEL_SHA",
  "required_min_version": "0.1.0",
  "update_type": "full",
  "wipe_data": false,
  "reboot_required": true
}
EOF

# ─── 3. Write update engine entry point ───────────────────────────────────────
cat > "$WORK_DIR/META-INF/com/aether/update-script" << 'EOF'
# AetherOS update script — processed by aether-update-engine
# Format: <command> <args>
flash_image  kernel    Image.gz
install_bin  aetherd
install_bin  aether-telephonyd
install_bin  aether-networkd
install_bin  aether-ai-daemon
install_bin  aether-compositor
install_bin  aether-release-bot
set_version  @@VERSION@@
sync
reboot
EOF

sed -i "s/@@VERSION@@/$VERSION/g" "$WORK_DIR/META-INF/com/aether/update-script"

# ─── 4. Package ───────────────────────────────────────────────────────────────
echo "--> Creating OTA zip..."
(cd "$WORK_DIR" && zip -r "$OUTPUT" . -x "*.DS_Store")

# ─── 5. Sign ──────────────────────────────────────────────────────────────────
if [[ -n "$SIGN_KEY" && -f "$SIGN_KEY" ]]; then
  echo "--> Signing with ed25519..."
  openssl dgst -sha256 -sign "$SIGN_KEY" -out "$OUTPUT.sig" "$OUTPUT"
  echo "    Signature: $OUTPUT.sig"
else
  echo "    [WARN] No signing key — package unsigned (dev mode only)"
  echo "UNSIGNED_DEV" > "$OUTPUT.sig"
fi

# ─── 6. Checksums ─────────────────────────────────────────────────────────────
SHA256=$(sha256sum "$OUTPUT" | awk '{print $1}')
SIZE=$(stat -c%s "$OUTPUT" 2>/dev/null || stat -f%z "$OUTPUT")
echo "$SHA256  $FILENAME" > "$OUTPUT.sha256"

# ─── 7. Summary ───────────────────────────────────────────────────────────────
echo ""
echo "==> OTA package ready:"
echo "    File:    $OUTPUT"
echo "    Size:    $(numfmt --to=iec $SIZE)"
echo "    SHA256:  $SHA256"
echo "    Version: $VERSION"
echo "    Channel: $CHANNEL"
echo ""

rm -rf "$WORK_DIR"
