#!/usr/bin/env python3
"""Unpack Android boot.img v0 header into kernel, ramdisk, and metadata."""
import argparse
import json
import struct
import sys
from pathlib import Path


def unpack_boot_img(path: Path, out_dir: Path) -> dict:
    data = path.read_bytes()
    magic = data[0:8]
    if magic != b"ANDROID!":
        raise ValueError(f"{path}: not a valid Android boot image")

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
    ) = struct.unpack_from("<10I", data, 8)

    cmdline = data[64:576].split(b"\x00")[0].decode("ascii", errors="replace")
    base = kernel_addr - 0x00008000

    kernel_offset = page_size
    kernel_pages = (kernel_size + page_size - 1) // page_size
    ramdisk_offset = page_size * (1 + kernel_pages)
    ramdisk_pages = (ramdisk_size + page_size - 1) // page_size
    second_offset = page_size * (1 + kernel_pages + ramdisk_pages)

    out_dir.mkdir(parents=True, exist_ok=True)

    kernel = data[kernel_offset : kernel_offset + kernel_size]
    ramdisk = data[ramdisk_offset : ramdisk_offset + ramdisk_size]
    second = data[second_offset : second_offset + second_size] if second_size else b""

    kernel_path = out_dir / "kernel"
    ramdisk_path = out_dir / "ramdisk"
    kernel_path.write_bytes(kernel)
    ramdisk_path.write_bytes(ramdisk)
    if second:
        (out_dir / "second").write_bytes(second)

    meta = {
        "source": str(path),
        "kernel_size": kernel_size,
        "ramdisk_size": ramdisk_size,
        "second_size": second_size,
        "page_size": page_size,
        "header_version": header_version,
        "os_version": f"0x{os_version:08x}",
        "cmdline": cmdline,
        "base": f"0x{base:08x}",
        "kernel_offset": f"0x{kernel_addr - base:08x}",
        "ramdisk_offset": f"0x{ramdisk_addr - base:08x}",
        "second_offset": f"0x{second_addr - base:08x}",
        "tags_offset": f"0x{tags_addr - base:08x}",
        "kernel_path": str(kernel_path),
        "ramdisk_path": str(ramdisk_path),
    }
    (out_dir / "metadata.json").write_text(json.dumps(meta, indent=2) + "\n")
    return meta


def main() -> int:
    parser = argparse.ArgumentParser(description="Unpack Android boot.img v0")
    parser.add_argument("image", type=Path)
    parser.add_argument("-o", "--out", type=Path, required=True)
    args = parser.parse_args()

    meta = unpack_boot_img(args.image, args.out)
    print(json.dumps(meta, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
