#!/usr/bin/env python3
import binascii
import struct
import sys

DEV = "/dev/mmcblk1"
SECTOR_SIZE = 512

def crc32(data):
    return binascii.crc32(data) & 0xffffffff

def update_gpt():
    print(f"Reading GPT from {DEV}...")
    with open(DEV, "r+b") as f:
        # Read Primary Header (LBA 1) and Entries (LBA 2..33)
        f.seek(1 * SECTOR_SIZE)
        hdr_bytes = bytearray(f.read(92))
        
        f.seek(2 * SECTOR_SIZE)
        num_parts = struct.unpack("<I", hdr_bytes[80:84])[0]
        part_size = struct.unpack("<I", hdr_bytes[84:88])[0]
        array_size = num_parts * part_size
        array_bytes = bytearray(f.read(array_size))

        boot_a_idx = -1
        boot_b_idx = -1

        for i in range(num_parts):
            off = i * part_size
            entry = array_bytes[off:off+part_size]
            name = entry[56:128].decode("utf-16le", errors="ignore").rstrip("\x00")
            if name == "boot_a":
                boot_a_idx = i
            elif name == "boot_b":
                boot_b_idx = i

        if boot_a_idx == -1 or boot_b_idx == -1:
            print("ERR: Could not find boot_a or boot_b in GPT!")
            sys.exit(1)

        print(f"Found boot_a at index {boot_a_idx}, boot_b at index {boot_b_idx}")

        # Update attributes for boot_a (clear active bit 50)
        off_a = boot_a_idx * part_size + 48
        attr_a = struct.unpack("<Q", array_bytes[off_a:off_a+8])[0]
        attr_a &= ~(1 << 50)  # Clear active
        array_bytes[off_a:off_a+8] = struct.pack("<Q", attr_a)

        # Update attributes for boot_b (set active bit 50, boot-successful bit 54, clear unbootable bit 55)
        off_b = boot_b_idx * part_size + 48
        attr_b = struct.unpack("<Q", array_bytes[off_b:off_b+8])[0]
        attr_b |= (1 << 50)   # Set active
        attr_b |= (1 << 54)   # Set boot-successful
        attr_b &= ~(1 << 55)  # Clear unbootable
        array_bytes[off_b:off_b+8] = struct.pack("<Q", attr_b)

        # Recalculate array CRC
        new_array_crc = crc32(array_bytes)
        struct.pack_into("<I", hdr_bytes, 88, new_array_crc)

        # Recalculate header CRC (with CRC field 16..20 set to zero)
        hdr_bytes[16:20] = b"\x00\x00\x00\x00"
        new_hdr_crc = crc32(hdr_bytes)
        struct.pack_into("<I", hdr_bytes, 16, new_hdr_crc)

        # Write back Primary Header & Array
        f.seek(2 * SECTOR_SIZE)
        f.write(array_bytes)
        f.seek(1 * SECTOR_SIZE)
        f.write(hdr_bytes)

        print("✓ Primary GPT successfully updated! Slot B is now ACTIVE.")

if __name__ == "__main__":
    update_gpt()
