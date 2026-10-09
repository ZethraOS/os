#!/usr/bin/env python3
"""
update_modemst1_gen17.py — Surgical Generation 17 Update for modemst1 EFS Superblock
Part of ZethraOS / Nokia 6.1 Plus (DRGID18100509899) Phase 5R Bring-up.

Aligns modemst1 (Bank 1 / Caller 0) to Generation 17 (seq 0x6c000052) matching
modemst2 (Bank 2 / Caller 1), while strictly preserving the Bank 1 superblock
magic string (IMGEFS1\\x9c at offset 0x28).

Superblock Field Layout (Sector 0, 512 bytes):
  0x00..0x04: Generation Identifier (0x10 -> 0x11 for Gen 17)
  0x28..0x30: Superblock Magic (IMGEFS1\\x9c for Bank 1)
  0x30..0x34: Sequence Counter (0x6c000052, little-endian: 52 00 00 6c)
  0x34..0x38: Generation Timestamp (0x9e8b1428, little-endian: 28 14 8b 9e)
  0x38..0x3C: Active Superblock Flag (0x00000001, little-endian: 01 00 00 00)
  0x3C..0x40: Pairing Information (0x00000102, little-endian: 02 01 00 00)
  0x1FC..0x200: CRC32 / Checksum bytes (clean zeroed: 00 00 00 00)
"""

import os
import sys
import hashlib

def main():
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "../.."))

    # Determine input files
    st1_candidates = [
        os.path.join(repo_root, "scratch/efs_backup/modemst1.img"),
        "/dev/disk/by-partlabel/modemst1",
    ]
    st2_candidates = [
        os.path.join(repo_root, "scratch/efs_backup/modemst2.img"),
        "/dev/disk/by-partlabel/modemst2",
    ]

    st1_src = sys.argv[1] if len(sys.argv) > 1 else None
    if not st1_src:
        for c in st1_candidates:
            if os.path.exists(c):
                st1_src = c
                break

    st2_src = sys.argv[2] if len(sys.argv) > 2 else None
    if not st2_src:
        for c in st2_candidates:
            if os.path.exists(c):
                st2_src = c
                break

    out_path = sys.argv[3] if len(sys.argv) > 3 else None
    if not out_path:
        # Default on host to scratch, or /etc on target
        if os.path.isdir(os.path.join(repo_root, "scratch")):
            out_path = os.path.join(repo_root, "scratch/modemst1_gen17_sector1.img")
        else:
            out_path = "/etc/modemst1_gen17_sector1.img"

    if not st1_src or not os.path.exists(st1_src):
        raise FileNotFoundError(f"modemst1 source not found: {st1_src}")
    if not st2_src or not os.path.exists(st2_src):
        raise FileNotFoundError(f"modemst2 source not found: {st2_src}")

    print(f"[+] Reading modemst2 from: {st2_src}")
    with open(st2_src, "rb") as f:
        st2_sec = f.read(512)
    assert len(st2_sec) >= 512, "modemst2 must be at least 512 bytes"

    # Extract Gen 17 fields from modemst2
    gen17_id = st2_sec[0x00:0x04]         # 11 00 00 00 (17)
    gen17_seq = st2_sec[0x30:0x34]        # 52 00 00 6c (0x6c000052)
    gen17_ts = st2_sec[0x34:0x38]         # 28 14 8b 9e
    gen17_flags = st2_sec[0x38:0x40]      # 01 00 00 00 02 01 00 00

    print(f"    Gen 17 ID:       {gen17_id.hex()}")
    print(f"    Gen 17 Sequence: {gen17_seq.hex()} ({list(gen17_seq)})")
    print(f"    Gen 17 Flags:    {gen17_flags.hex()}")

    print(f"[+] Reading modemst1 from: {st1_src}")
    with open(st1_src, "rb") as f:
        st1_data = bytearray(f.read(512))
    assert len(st1_data) >= 512, "modemst1 must be at least 512 bytes"

    # Verify/enforce IMGEFS1 magic string at offset 0x28
    magic = bytes(st1_data[0x28:0x30])
    print(f"    Current magic:   {magic}")
    if b"IMGEFS1" not in magic:
        print("    [!] Restoring IMGEFS1\\x9c magic on modemst1...")
        st1_data[0x28:0x30] = b"IMGEFS1\x9c"

    # Apply surgical Generation 17 alignment
    print("[+] Applying surgical Generation 17 update...")
    st1_data[0x00:0x04] = gen17_id
    st1_data[0x30:0x34] = gen17_seq
    st1_data[0x34:0x38] = gen17_ts
    st1_data[0x38:0x40] = gen17_flags
    st1_data[508:512] = b"\x00\x00\x00\x00"

    # Ensure output directory exists
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)

    with open(out_path, "wb") as f:
        f.write(st1_data[:512])

    print(f"[✓] Successfully wrote modified Sector to {out_path}")
    print(f"    Size:       {len(st1_data[:512])} bytes")
    print(f"    Magic:      {bytes(st1_data[0x28:0x30])}")
    print(f"    Gen ID:     {bytes(st1_data[0x00:0x04]).hex()}")
    print(f"    Sequence:   {bytes(st1_data[0x30:0x34]).hex()}")
    print(f"    Row 0x30:   {bytes(st1_data[0x30:0x40]).hex()}")
    print(f"    SHA-256:    {hashlib.sha256(st1_data[:512]).hexdigest()}")

if __name__ == "__main__":
    main()
