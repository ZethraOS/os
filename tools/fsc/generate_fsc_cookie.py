#!/usr/bin/env python3
"""
generate_fsc_cookie.py — Qualcomm EFS2 File System Cookie (FSC) Generator
Part of ZethraOS / Nokia 6.1 Plus (DRGID18100509899) Bring-up.

Reads modemst1.img and modemst2.img superblock headers, extracts active
sequence counters and generation timestamps, and generates candidate
1,024-byte FSC cookie partitions to validate modem EFS mounting.
"""

import os
import sys
import struct
import zlib
import hashlib

def read_superblock_metadata(img_path):
    if not os.path.exists(img_path):
        raise FileNotFoundError(f"Partition image not found: {img_path}")
    
    with open(img_path, "rb") as f:
        data = f.read(512)
    
    gen_val = struct.unpack("<I", data[0x00:0x04])[0]
    sb_type = struct.unpack("<I", data[0x04:0x08])[0]
    sb_len = struct.unpack("<I", data[0x08:0x0c])[0]
    magic = data[0x28:0x30]
    seq_cnt = struct.unpack("<I", data[0x30:0x34])[0]
    timestamp = struct.unpack("<I", data[0x34:0x38])[0]
    active_flag = struct.unpack("<I", data[0x38:0x3c])[0]
    pairing_info = struct.unpack("<I", data[0x3c:0x40])[0]
    
    return {
        "path": img_path,
        "gen_val": gen_val,
        "sb_type": sb_type,
        "sb_len": sb_len,
        "magic": magic,
        "seq_cnt": seq_cnt,
        "timestamp": timestamp,
        "active_flag": active_flag,
        "pairing_info": pairing_info,
        "sector0": data
    }

def build_fsc_sector(base_meta, target_seq, target_gen=1):
    sector = bytearray(512)
    
    # 0x00..0x04: Generation identifier (0x10 for gen 1, 0x11 for gen 2)
    gen_id = 0x00000010 if target_gen == 1 else 0x00000011
    struct.pack_into("<I", sector, 0x00, gen_id)
    
    # 0x04..0x08: Superblock type (0x03)
    struct.pack_into("<I", sector, 0x04, base_meta["sb_type"])
    
    # 0x08..0x0c: Length in sectors (0x01d8)
    struct.pack_into("<I", sector, 0x08, base_meta["sb_len"])
    
    # 0x0c..0x10: Memory/DDR base pointer from active superblock
    sector[0x0c:0x10] = base_meta["sector0"][0x0c:0x10]
    
    # 0x10..0x18: Total size fields (0x001ffe00)
    struct.pack_into("<I", sector, 0x10, 0x001ffe00)
    struct.pack_into("<I", sector, 0x14, 0x001ffe00)
    
    # 0x18..0x28: Commit hash/pointers from active superblock
    sector[0x18:0x28] = base_meta["sector0"][0x18:0x28]
    
    # 0x28..0x30: Superblock magic
    magic_str = b"IMGEFS1\x9c" if target_gen == 1 else b"IMGEFS2\x9b"
    sector[0x28:0x30] = magic_str
    
    # 0x30..0x34: Target sequence counter
    struct.pack_into("<I", sector, 0x30, target_seq)
    
    # 0x34..0x38: Generation timestamp
    struct.pack_into("<I", sector, 0x34, base_meta["timestamp"])
    
    # 0x38..0x3c: Active flag
    struct.pack_into("<I", sector, 0x38, 0x00000001)
    
    # 0x3c..0x40: Pairing info
    struct.pack_into("<I", sector, 0x3c, base_meta["pairing_info"])
    
    # 0x40..0x1fc: Copy secondary metadata table from active superblock
    sector[0x40:0x1fc] = base_meta["sector0"][0x40:0x1fc]
    
    # 0x1fc..0x200: CRC32 checksum over the first 508 bytes
    crc = zlib.crc32(sector[:508]) & 0xffffffff
    struct.pack_into("<I", sector, 508, crc)
    
    return bytes(sector)

def generate_fsc_image(base_meta, target_seq, target_gen=1):
    sec0 = build_fsc_sector(base_meta, target_seq, target_gen=target_gen)
    sec1 = build_fsc_sector(base_meta, target_seq, target_gen=target_gen)
    return sec0 + sec1

def main():
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "../.."))
    efs_dir = os.path.join(repo_root, "scratch/efs_backup")
    out_dir = os.path.join(repo_root, "scratch")
    
    st1_path = os.path.join(efs_dir, "modemst1.img")
    st2_path = os.path.join(efs_dir, "modemst2.img")
    
    print("=" * 65)
    print("      Qualcomm EFS2 File System Cookie (FSC) Generator")
    print("=" * 65)
    
    meta1 = read_superblock_metadata(st1_path)
    meta2 = read_superblock_metadata(st2_path)
    
    print(f"Loaded modemst1: magic={meta1['magic']}, seq=0x{meta1['seq_cnt']:08x}, ts=0x{meta1['timestamp']:08x}")
    print(f"Loaded modemst2: magic={meta2['magic']}, seq=0x{meta2['seq_cnt']:08x}, ts=0x{meta2['timestamp']:08x}")
    
    base_seq = meta1["seq_cnt"]  # 0x20000052
    
    candidates = [
        ("fsc_active_gen1.img", base_seq, 1, "Active Generation 1 (matches modemst1 seq 0x20000052)"),
        ("fsc_next_gen2.img", base_seq + 1, 2, "Next Generation 2 (forward seq 0x20000053)"),
        ("fsc_prev_gen0.img", base_seq - 1, 1, "Previous Generation 0 (prior seq 0x20000051)")
    ]
    
    print("\nGenerating candidate FSC cookie images:")
    for fname, seq_val, gen_id, desc in candidates:
        img_bytes = generate_fsc_image(meta1, seq_val, target_gen=gen_id)
        assert len(img_bytes) == 1024, "FSC image must be exactly 1024 bytes"
        
        target_path = os.path.join(out_dir, fname)
        with open(target_path, "wb") as f:
            f.write(img_bytes)
        
        sha = hashlib.sha256(img_bytes).hexdigest()
        print(f"✓ {fname:22s} ({len(img_bytes)} B) [seq: 0x{seq_val:08x}] sha256: {sha}")
        print(f"  Description: {desc}")

if __name__ == "__main__":
    main()
