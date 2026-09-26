#!/bin/bash
# SPDX-License-Identifier: GPL-2.0
# stage_modem_firmware.sh — Verify and stage Qualcomm modem firmware for ZethraOS
#
# Hybrid staging strategy:
#   • mba.mbn (238 KB)  → copied into initramfs at lib/firmware/qcom/sdm636/
#   • modem.mdt + modem.b* segments (~55 MB) → loaded at runtime from
#     /lib/firmware/qcom/sdm636/ (mounted firmware partition or tmpfs)
#
# This script verifies SHA-256 hashes of ALL firmware files against known-good
# values extracted from the Nokia 6.1 Plus stock OTA (SDM660.LA.3.0.1-30037).
# It stages mba.mbn into the initramfs staging directory and prepares a manifest
# of modem segment files for runtime firmware loading.
#
# Usage:
#   MODEM_FIRMWARE_DIR=/path/to/modem_out/image  bash build/scripts/stage_modem_firmware.sh
#
# Environment:
#   MODEM_FIRMWARE_DIR  (required) — absolute path to directory containing mba.mbn,
#                        modem.mdt, and modem.b00–b28 extracted from stock OTA.
#   INITRAMFS_DIR       (optional) — initramfs staging root; default: build/out/initramfs
#
# Exit codes:
#   0  — all files verified and staged successfully
#   1  — missing environment variable or directory
#   2  — firmware file missing
#   3  — SHA-256 hash mismatch
#
set -euo pipefail

# ─── Configuration ───────────────────────────────────────────────────────────

FIRMWARE_SUBDIR="qcom/sdm636"

# Nokia 6.1 Plus stock OTA firmware checksum table (SHA-256 and filename)
# Source: SDM660.LA.3.0.1-30037-STD.PROD-1 (2019-12-11)
# MPSS version: MPSS.AT.3.1-00822-SDM660_GEN_PACK-1
# Note: segments b15 and b19 have filesz == 0 (NOBITS/BSS) and are omitted per spec.
EXPECTED_CHECKSUMS="
860fed1dfa06083d6c8218ce1416bed55751295660bdc5165e21f79d843bb336  mba.mbn
65fec9eafb8726fb6916b7ee569bd718cda12a4166435519b10c76a6b7995d86  modem.mdt
89c56c0fa492921136c410ac9efcb8298347b6c46d0253ff860c569e82eec85f  modem.b00
29c77aa2383f174322e0e741a21a2e0ef93eae6e5883848f5c4807c27c494248  modem.b01
8e5fe5308d1de82a6d974fb797427a89d84748265e4b60c45e024049d5e6eebc  modem.b02
39aad8e03aed3df085012dde7d630f54c5ff747675beb295f4695365519d211b  modem.b03
48cec12f9e7414ffc29397f2c92fc7443d1a3816f8509bdc5461bae7557b5a1d  modem.b04
0af36124860bcfd22c06e8a5561b6312865e620fd7ac45c9a391077d8703f991  modem.b05
deaea51bb67661422c493fd0f079c31ed75a4d1db550b1a4f53039a068a4df3d  modem.b06
d8fbc4afb0bba8ee1c3fffe9c23514f0fbd87411f91af5f93d99d61b5f4827c3  modem.b07
b8644328d69d3fa351f2ec86b486da6e5b9befb97596ee3ecec4450de4cbfd4f  modem.b08
6d51bcfe3a76f6997ca47c7b23afe8f2856220e14192c083d45b8f78a34568f0  modem.b09
8b85fc30988a60b3fa8c5fa31a4e5e821ef1890ea6e0c8d3ba64c0de88521e87  modem.b10
4b1977a17a9e2e06783281cdf663951414bf83cdaa9cfe8b8dd122332927ad1f  modem.b11
4b4df63f6cd0d9af9b5c5e3866b9766b93900e13f27802db6a3f893a8b1a02c2  modem.b12
1ba33f4f7b5eb2a221da6a77b83c515e75e6161f4d1635ff65088a66d0093fe5  modem.b13
b975c6d0904ebdfe9729bc0ebe85e2b60aab1a73a72858f9be2fe7f735880c85  modem.b14
3328f89ff290d10ddd671695862e73c11202e72001aae16326917f70faa19d9b  modem.b16
8686a19f80be9d641bc5c743acfe41044513ecb719459b2e01bccbbcf49ed035  modem.b17
9cc26cc36dbec1945d9e4b0a50f85830fd81cfb1d649c29e76e243e1fd6c8afa  modem.b18
6acd26250c0aef6331c4fefc87c4616822d9dd8598d8bccfa9f20a6d528f5789  modem.b20
129756c7c5cd78729d4ca094e95f5d5f5414a6fe1f44c76a3d132ca00fc60f77  modem.b21
eb8e367d1e8bd6739eaeff5eabb30d8ea8447aaf66a6b6f390d9882f1562534e  modem.b22
93ea4f6895e3f8d04dc931235c2bfaa160edb08312ec6ab4a8f180c2f8237d1b  modem.b23
80b5a7535aedb215ec648aa9fc6fe0a1f0c79633cf2e5a33fa6002cda100175f  modem.b24
21373f3ac456b9dd34f033e850e72f8698a78eaeef9afd1536aced37412b6256  modem.b25
df5070de3d1094f04b83187570d70375a08d95b57fa5b61e35750c06380a0cc5  modem.b26
d1fd1001bc1dd5af2d300cc35b83d2ccbb5f9fc607f41634983b77ab28fe7011  modem.b27
7878d3d4731854792bf396c9a40765a79bdd26b438814d1d641ba16ef612f250  modem.b28
"

# ─── Validation ──────────────────────────────────────────────────────────────

if [ -z "${MODEM_FIRMWARE_DIR:-}" ]; then
    echo "[ERROR] MODEM_FIRMWARE_DIR is not set." >&2
    echo "  Usage: MODEM_FIRMWARE_DIR=/path/to/modem_out/image $0" >&2
    exit 1
fi

if [ ! -d "$MODEM_FIRMWARE_DIR" ]; then
    echo "[ERROR] MODEM_FIRMWARE_DIR does not exist: $MODEM_FIRMWARE_DIR" >&2
    exit 1
fi

INITRAMFS_DIR="${INITRAMFS_DIR:-build/out/initramfs}"

echo "=================================================="
echo "    ZethraOS Modem Firmware Staging"
echo "=================================================="
echo "==> Source:    $MODEM_FIRMWARE_DIR"
echo "==> Initramfs: $INITRAMFS_DIR"
echo "==> Firmware subdir: $FIRMWARE_SUBDIR"
echo ""

# ─── Phase 1: Verify all firmware files exist ───────────────────────────────

echo "==> Phase 1: Checking firmware file presence..."
missing=0
total_files=0
while read -r expected file; do
    [ -z "$file" ] && continue
    total_files=$((total_files + 1))
    if [ ! -f "$MODEM_FIRMWARE_DIR/$file" ]; then
        echo "  [MISSING] $file" >&2
        missing=1
    fi
done <<< "$EXPECTED_CHECKSUMS"

if [ "$missing" -eq 1 ]; then
    echo "[ERROR] One or more firmware files are missing from $MODEM_FIRMWARE_DIR" >&2
    exit 2
fi
echo "  ✓ All $total_files firmware files present."

# ─── Phase 2: Verify SHA-256 hashes ─────────────────────────────────────────

echo ""
echo "==> Phase 2: Verifying SHA-256 hashes..."
hash_fail=0
verified_count=0
while read -r expected file; do
    [ -z "$file" ] && continue
    actual=$(shasum -a 256 "$MODEM_FIRMWARE_DIR/$file" | awk '{print $1}')
    if [ "$actual" != "$expected" ]; then
        echo "  [FAIL] $file" >&2
        echo "    expected: $expected" >&2
        echo "    actual:   $actual" >&2
        hash_fail=1
    else
        size=$(wc -c < "$MODEM_FIRMWARE_DIR/$file" | tr -d ' ')
        echo "  [OK] $file  ($size bytes)"
        verified_count=$((verified_count + 1))
    fi
done <<< "$EXPECTED_CHECKSUMS"

if [ "$hash_fail" -eq 1 ]; then
    echo "[ERROR] SHA-256 hash mismatch detected. Firmware may be corrupted or wrong version." >&2
    exit 3
fi
echo "  ✓ All $verified_count hashes verified."

# ─── Phase 3: Stage MBA into initramfs ──────────────────────────────────────

echo ""
echo "==> Phase 3: Staging initramfs files..."
initramfs_fw_dir="$INITRAMFS_DIR/lib/firmware/$FIRMWARE_SUBDIR"
mkdir -p "$initramfs_fw_dir"

# Stage mba.mbn only
cp "$MODEM_FIRMWARE_DIR/mba.mbn" "$initramfs_fw_dir/mba.mbn"
mba_size=$(wc -c < "$initramfs_fw_dir/mba.mbn" | tr -d ' ')
echo "  ✓ mba.mbn → initramfs ($mba_size bytes)"

# ─── Phase 4: Generate runtime firmware manifest ────────────────────────────

echo ""
echo "==> Phase 4: Generating runtime firmware manifest..."
manifest_file="$initramfs_fw_dir/.modem-manifest.txt"

total_runtime_size=0
runtime_file_count=0

{
    echo "# ZethraOS Modem Firmware Manifest"
    echo "# Source OTA: SDM660.LA.3.0.1-30037-STD.PROD-1 (2019-12-11)"
    echo "# MPSS: MPSS.AT.3.1-00822-SDM660_GEN_PACK-1"
    echo "# Generated: $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    echo "#"
    echo "# These files must be available at /lib/firmware/$FIRMWARE_SUBDIR/"
    echo "# at modem boot time. They are NOT included in initramfs due to size (~55 MB)."
    echo "# Mount the modem firmware partition or use tmpfs + serial transfer."
    echo "#"
    echo "# Format: sha256  filename  size_bytes"
    while read -r expected file; do
        [ -z "$file" ] && continue
        if [ "$file" != "mba.mbn" ]; then
            size=$(wc -c < "$MODEM_FIRMWARE_DIR/$file" | tr -d ' ')
            echo "$expected  $file  $size"
        fi
    done <<< "$EXPECTED_CHECKSUMS"
} > "$manifest_file"

while read -r expected file; do
    [ -z "$file" ] && continue
    if [ "$file" != "mba.mbn" ]; then
        size=$(wc -c < "$MODEM_FIRMWARE_DIR/$file" | tr -d ' ')
        total_runtime_size=$((total_runtime_size + size))
        runtime_file_count=$((runtime_file_count + 1))
    fi
done <<< "$EXPECTED_CHECKSUMS"

echo "  ✓ Manifest written: $manifest_file"
echo "  Runtime files: $runtime_file_count files"
echo "  Total runtime firmware size: $((total_runtime_size / 1024 / 1024)) MB"

# ─── Summary ─────────────────────────────────────────────────────────────────

echo ""
echo "=================================================="
echo "    Staging Complete"
echo "=================================================="
echo "  Initramfs: 1 file staged (mba.mbn, $mba_size bytes)"
echo "  Runtime:   $runtime_file_count files in manifest (load from /lib/firmware)"
echo ""
echo "  Next steps:"
echo "    1. Rebuild initramfs with build/scripts/build_minimal_initramfs.sh"
echo "    2. At runtime, mount modem firmware to /lib/firmware/$FIRMWARE_SUBDIR/"
echo "       or transfer via serial and place in tmpfs"
echo "    3. The kernel PIL driver will request_firmware() for modem.mdt and segments"
echo "=================================================="
