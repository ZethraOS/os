#!/usr/bin/env python3
"""Parse Android boot.img header to inspect format parameters."""
import struct
import sys

def parse_boot_img(path):
    with open(path, 'rb') as f:
        data = f.read(4096)
    
    # Check magic
    magic = data[0:8]
    print(f"Magic: {magic}")
    if magic != b'ANDROID!':
        print("ERROR: Not a valid Android boot image!")
        return
    
    # Boot image header v0/v1/v2 format
    # https://android.googlesource.com/platform/system/tools/mkbootimg/+/refs/heads/main/include/bootimg/bootimg.h
    (
        kernel_size,
        kernel_addr,
        ramdisk_size,
        ramdisk_addr,
        second_size,
        second_addr,
        tags_addr,
        page_size,
        header_version,
        os_version,
    ) = struct.unpack_from('<10I', data, 8)
    
    name = data[48:64].split(b'\x00')[0].decode('ascii', errors='replace')
    cmdline = data[64:576].split(b'\x00')[0].decode('ascii', errors='replace')
    
    # Compute base and offsets
    base = kernel_addr - 0x00008000
    kernel_offset = kernel_addr - base
    ramdisk_offset = ramdisk_addr - base
    second_offset = second_addr - base
    tags_offset = tags_addr - base
    
    print(f"\n=== Boot Image Header ===")
    print(f"Kernel size:     {kernel_size} bytes ({kernel_size/1024/1024:.1f} MB)")
    print(f"Kernel addr:     0x{kernel_addr:08x}")
    print(f"Ramdisk size:    {ramdisk_size} bytes ({ramdisk_size/1024/1024:.1f} MB)")
    print(f"Ramdisk addr:    0x{ramdisk_addr:08x}")
    print(f"Second size:     {second_size} bytes")
    print(f"Second addr:     0x{second_addr:08x}")
    print(f"Tags addr:       0x{tags_addr:08x}")
    print(f"Page size:       {page_size}")
    print(f"Header version:  {header_version}")
    print(f"OS version:      0x{os_version:08x}")
    print(f"Name:            '{name}'")
    print(f"Cmdline:         '{cmdline[:80]}...'")
    
    print(f"\n=== Computed mkbootimg Parameters ===")
    print(f"--base           0x{base:08x}")
    print(f"--kernel_offset  0x{kernel_offset:08x}")
    print(f"--ramdisk_offset 0x{ramdisk_offset:08x}")
    print(f"--second_offset  0x{second_offset:08x}")
    print(f"--tags_offset    0x{tags_offset:08x}")
    print(f"--pagesize       {page_size}")
    print(f"--header_version {header_version}")
    
    if header_version >= 1:
        recovery_dtbo_size = struct.unpack_from('<I', data, 1632)[0]
        recovery_dtbo_offset = struct.unpack_from('<Q', data, 1636)[0]
        header_size = struct.unpack_from('<I', data, 1644)[0]
        print(f"\n=== Header v1+ Fields ===")
        print(f"Recovery DTBO size:   {recovery_dtbo_size}")
        print(f"Recovery DTBO offset: 0x{recovery_dtbo_offset:016x}")
        print(f"Header size:          {header_size}")
    
    if header_version >= 2:
        dtb_size = struct.unpack_from('<I', data, 1648)[0]
        dtb_addr = struct.unpack_from('<Q', data, 1652)[0]
        dtb_offset = dtb_addr - base if dtb_addr > base else dtb_addr
        print(f"\n=== Header v2 Fields ===")
        print(f"DTB size:             {dtb_size} bytes")
        print(f"DTB addr:             0x{dtb_addr:016x}")
        print(f"--dtb_offset          0x{dtb_offset:08x}")

if __name__ == '__main__':
    for path in sys.argv[1:]:
        print(f"\n{'='*60}")
        print(f"Parsing: {path}")
        print(f"{'='*60}")
        parse_boot_img(path)
