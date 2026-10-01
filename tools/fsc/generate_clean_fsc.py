#!/usr/bin/env python3
"""
generate_clean_fsc.py — Qualcomm EFS2 Clean File System Cookie (FSC) Generator
Part of ZethraOS / Nokia 6.1 Plus (DRGID18100509899) Phase 5O Bring-up.

Reads Sector 0 metadata from candidate FSC or stock backups, aligns sequence
to Generation 17 (0x6c000052, matching modemst2 active generation), ensures
CRC bytes 508-511 are clean zeroed (00 00 00 00) per Qualcomm stock superblock
specification, and writes scratch/fsc_clean_sector0.img.
"""

import os
import sys
import hashlib

def main():
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "../.."))
    out_dir = os.path.join(repo_root, "scratch")
    os.makedirs(out_dir, exist_ok=True)
    out_path = os.path.join(out_dir, "fsc_clean_sector0.img")

    # Priority source order for Generation 17 alignment:
    # 1. scratch/efs_backup/modemst2.img (stock Gen 17 sector 0 with sequence 0x6c000052)
    # 2. scratch/fsc_real_gen2.img (Gen 17 metadata)
    # 3. scratch/fsc_next_gen2.img (Gen 17 candidate)
    source_candidates = [
        os.path.join(repo_root, "scratch/efs_backup/modemst2.img"),
        os.path.join(repo_root, "scratch/fsc_real_gen2.img"),
        os.path.join(repo_root, "scratch/fsc_next_gen2.img"),
        os.path.join(repo_root, "scratch/efs_backup/fsc.img"),
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

    # Ensure Generation 17 alignment: sequence 0x6c000052 matching modemst2
    # In Qualcomm EFS superblock:
    # Offset 0x00: Generation counter = 17 (0x11)
    # Offset 0x28: Magic = IMGEFS2\x9b
    # Offset 0x30: Sequence number = 0x6c000052 (bytes 52 00 00 6c)
    current_gen = int.from_bytes(fsc_data[0:4], "little")
    current_seq = int.from_bytes(fsc_data[0x30:0x34], "little")
    print(f"[+] Current generation: {current_gen} (0x{current_gen:02x}), sequence: 0x{current_seq:08x}")

    if current_gen != 17 or current_seq != 0x6c000052:
        print("[!] Aligning FSC to Generation 17 (seq 0x6c000052, magic IMGEFS2)...")
        fsc_data[0:4] = (17).to_bytes(4, "little")
        fsc_data[0x28:0x30] = b"IMGEFS2\x9b"
        fsc_data[0x30:0x34] = (0x6c000052).to_bytes(4, "little")

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
    gen = int.from_bytes(fsc_data[0:4], "little")
    seq = int.from_bytes(fsc_data[0x30:0x34], "little")
    print(f"[✓] Successfully generated clean FSC Sector 0:")
    print(f"    Path:       {out_path}")
    print(f"    Size:       {len(fsc_data)} bytes")
    print(f"    Generation: {gen} (0x{gen:02x})")
    print(f"    Sequence:   0x{seq:08x}")
    print(f"    Magic:      {bytes(fsc_data[0x28:0x30])}")
    print(f"    SHA-256:    {sha256}")

if __name__ == "__main__":
    main()
