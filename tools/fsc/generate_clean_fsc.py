#!/usr/bin/env python3
"""
generate_clean_fsc.py — Qualcomm EFS2 Clean File System Cookie (FSC) Generator
Part of ZethraOS / Nokia 6.1 Plus (DRGID18100509899) Phase 5N Bring-up.

Reads Sector 0 metadata from candidate FSC or stock backups, verifies and
ensures CRC bytes 508-511 are clean zeroed (00 00 00 00) per Qualcomm stock
superblock specification, and writes scratch/fsc_clean_sector0.img.
"""

import os
import sys
import hashlib

def main():
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "../.."))
    out_dir = os.path.join(repo_root, "scratch")
    os.makedirs(out_dir, exist_ok=True)
    out_path = os.path.join(out_dir, "fsc_clean_sector0.img")

    # Priority source order:
    # 1. scratch/fsc_active_gen1.img (contains active IMGEFS1 metadata with f486bcd1 at 508:512)
    # 2. scratch/efs_backup/fsc.img (if non-zero)
    # 3. scratch/efs_backup/modemst1.img (stock sector 0)
    source_candidates = [
        os.path.join(repo_root, "scratch/fsc_active_gen1.img"),
        os.path.join(repo_root, "scratch/efs_backup/fsc.img"),
        os.path.join(repo_root, "scratch/efs_backup/modemst1.img"),
    ]

    selected_src = None
    fsc_data = None

    for src in source_candidates:
        if os.path.exists(src):
            with open(src, "rb") as f:
                raw = f.read(512)
            if len(raw) >= 512 and any(b != 0 for b in raw):
                selected_src = src
                fsc_data = bytearray(raw[:512])
                break

    if fsc_data is None:
        # Fallback to fsc.img even if zeros
        fallback = os.path.join(repo_root, "scratch/efs_backup/fsc.img")
        if os.path.exists(fallback):
            selected_src = fallback
            with open(fallback, "rb") as f:
                fsc_data = bytearray(f.read(512))
        else:
            raise FileNotFoundError("No source FSC image found")

    print(f"[+] Loaded FSC source: {selected_src} (512 bytes)")
    orig_crc = bytes(fsc_data[508:512])
    print(f"[+] Original bytes 508-511: {orig_crc.hex()} ({list(orig_crc)})")

    # Verify and zero out bytes 508-511
    if orig_crc != b'\x00\x00\x00\x00':
        print(f"[!] Non-zero CRC detected ({orig_crc.hex()}). Zeroing out bytes 508-511...")
        fsc_data[508:512] = b'\x00\x00\x00\x00'
    else:
        print("[+] Bytes 508-511 are already clean zeroed.")

    # Write clean 512-byte FSC
    with open(out_path, "wb") as f:
        f.write(fsc_data)

    assert os.path.getsize(out_path) == 512, "Output file must be exactly 512 bytes"

    sha256 = hashlib.sha256(fsc_data).hexdigest()
    print(f"[✓] Successfully generated clean FSC Sector 0:")
    print(f"    Path:    {out_path}")
    print(f"    Size:    {len(fsc_data)} bytes")
    print(f"    Magic:   {bytes(fsc_data[0x28:0x30])}")
    print(f"    SHA-256: {sha256}")

if __name__ == "__main__":
    main()
