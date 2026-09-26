#!/usr/bin/env python3
"""
Chunked boot_b flasher over USB serial.
Uploads boot.img in safe 512 KB chunks with per-chunk validation,
preventing any TTY buffer saturation or FIFO stalls.
"""
import os
import sys
import time
import base64
import hashlib
import termios
import glob

BOOT_IMG_PATH = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/build/out/boot.img"
TARGET_PART = "/dev/disk/by-partlabel/boot_b"
CHUNK_RAW_SIZE = 512 * 1024  # 512 KB per chunk

def get_port():
    ports = glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/tty.usbmodem*")
    return sorted(ports)[0] if ports else None

def open_serial(port):
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    os.set_blocking(fd, True)
    attrs = termios.tcgetattr(fd)
    attrs[2] |= termios.CLOCAL | termios.CREAD
    attrs[2] &= ~termios.CSIZE
    attrs[2] |= termios.CS8
    attrs[2] &= ~termios.CSTOPB
    attrs[2] &= ~termios.PARENB
    if hasattr(termios, 'CRTSCTS'):
        attrs[2] &= ~termios.CRTSCTS
    attrs[0] &= ~(termios.IXON | termios.IXOFF | termios.IXANY)
    attrs[3] &= ~(termios.ICANON | termios.ECHO | termios.ECHOE | termios.ISIG)
    attrs[1] &= ~termios.OPOST
    termios.tcsetattr(fd, termios.TCSANOW, attrs)
    return fd

def drain(fd, timeout=0.3):
    os.set_blocking(fd, False)
    buf = b""
    t = time.time()
    while time.time() - t < timeout:
        try:
            c = os.read(fd, 65536)
            if c:
                buf += c
                t = time.time()
        except BlockingIOError:
            time.sleep(0.01)
    return buf

def cmd(fd, command, timeout=8.0, wait=0.1):
    drain(fd, 0.05)
    os.set_blocking(fd, True)
    os.write(fd, command.encode() + b"\n")
    time.sleep(wait)
    resp = drain(fd, timeout)
    return resp.decode(errors='replace')

def main():
    print("=" * 60)
    print("  ZethraOS Chunked boot_b Flasher (512 KB Chunks)")
    print("=" * 60)

    if not os.path.exists(BOOT_IMG_PATH):
        print(f"Error: {BOOT_IMG_PATH} does not exist!")
        sys.exit(1)

    with open(BOOT_IMG_PATH, "rb") as f:
        boot_data = f.read()

    total_bytes = len(boot_data)
    expected_sha = hashlib.sha256(boot_data).hexdigest()
    block_count_4k = total_bytes // 4096

    print(f"File:        {BOOT_IMG_PATH}")
    print(f"Size:        {total_bytes} bytes ({total_bytes / 1024 / 1024:.2f} MB)")
    print(f"SHA-256:     {expected_sha}")
    print(f"Target:      {TARGET_PART}")
    print(f"Chunk size:  {CHUNK_RAW_SIZE // 1024} KB")

    port = get_port()
    if not port:
        print("Error: No /dev/cu.usbmodem* port found!")
        sys.exit(1)
    print(f"Connecting to {port}...")

    fd = open_serial(port)
    os.write(fd, b"\n\x03\n\x03\n")
    time.sleep(0.3)
    drain(fd, 0.2)

    handshake = cmd(fd, "uname -a; ls -l " + TARGET_PART)
    print("Handshake:")
    print(handshake.strip())
    if "Linux" not in handshake or "boot_b" not in handshake:
        print("Handshake failed!")
        sys.exit(1)

    print("\n[1/4] Preparing /tmp on device...")
    cmd(fd, "rm -f /tmp/boot_new.img /tmp/chunk.b64")

    # Split into chunks
    chunks = []
    for offset in range(0, total_bytes, CHUNK_RAW_SIZE):
        chunks.append(boot_data[offset:offset + CHUNK_RAW_SIZE])
    num_chunks = len(chunks)
    print(f"Total chunks to upload: {num_chunks}")

    print("\n[2/4] Uploading chunks...")
    t_start = time.time()
    for idx, raw_chunk in enumerate(chunks):
        chunk_num = idx + 1
        t_chunk = time.time()
        b64 = base64.b64encode(raw_chunk).decode('ascii')
        lines = [b64[i:i+76] for i in range(0, len(b64), 76)]

        # Stream chunk into /tmp/chunk.b64 via heredoc
        os.set_blocking(fd, True)
        os.write(fd, b"cat << 'CEOF' > /tmp/chunk.b64\n")
        for line in lines:
            os.write(fd, (line + "\n").encode())
            # tiny throttle to allow TTY buffering without overflow
            time.sleep(0.0005)
        os.write(fd, b"CEOF\n")
        time.sleep(0.1)

        # Append decoded chunk to /tmp/boot_new.img
        res = cmd(fd, "busybox base64 -d /tmp/chunk.b64 >> /tmp/boot_new.img && rm -f /tmp/chunk.b64 && echo CHUNK_OK", timeout=5.0)
        if "CHUNK_OK" not in res:
            print(f"\nError: Chunk {chunk_num} failed to decode!")
            print("Response:", res)
            cmd(fd, "rm -f /tmp/boot_new.img /tmp/chunk.b64")
            os.close(fd)
            sys.exit(1)

        elapsed = time.time() - t_chunk
        total_elapsed = time.time() - t_start
        bytes_uploaded = min((idx + 1) * CHUNK_RAW_SIZE, total_bytes)
        pct = (bytes_uploaded / total_bytes) * 100
        speed = (len(raw_chunk) / 1024) / max(elapsed, 0.001)
        avg_speed = (bytes_uploaded / 1024) / max(total_elapsed, 0.001)

        print(f"  Chunk {chunk_num:2d}/{num_chunks} OK | {bytes_uploaded / 1024:6.0f} KB ({pct:5.1f}%) | {speed:4.0f} KB/s (avg {avg_speed:4.0f} KB/s)")

    print(f"\nUpload completed in {time.time() - t_start:.1f}s!")

    print("\n[3/4] Verifying SHA-256 of assembled image on device...")
    size_out = cmd(fd, "wc -c /tmp/boot_new.img", timeout=5.0)
    print(f"  Device file size: {size_out.strip()}")

    sha_out = cmd(fd, "sha256sum /tmp/boot_new.img", timeout=15.0, wait=1.0)
    device_sha = None
    for l in sha_out.splitlines():
        parts = l.strip().split()
        if len(parts) >= 1 and len(parts[0]) == 64:
            device_sha = parts[0]
            break

    print(f"  Device SHA-256: {device_sha}")
    print(f"  Local  SHA-256: {expected_sha}")

    if device_sha != expected_sha:
        print("FATAL ERROR: Checksum mismatch on device!")
        cmd(fd, "rm -f /tmp/boot_new.img")
        os.close(fd)
        sys.exit(1)
    print("  ✓ Full boot.img checksum verified bit-exact!")

    print(f"\n[4/4] Flashing {TARGET_PART} on eMMC...")
    dd_res = cmd(fd, f"dd if=/tmp/boot_new.img of={TARGET_PART} bs=64k conv=fsync 2>&1 && sync", timeout=30.0, wait=2.0)
    print(f"  dd output:\n{dd_res.strip()}")

    print("\nVerifying partition hash on eMMC...")
    verify_cmd = f"dd if={TARGET_PART} bs=4096 count={block_count_4k} 2>/dev/null | sha256sum"
    part_sha_out = cmd(fd, verify_cmd, timeout=20.0, wait=1.0)
    part_sha = None
    for l in part_sha_out.splitlines():
        parts = l.strip().split()
        if len(parts) >= 1 and len(parts[0]) == 64:
            part_sha = parts[0]
            break

    print(f"  Partition SHA-256: {part_sha}")
    print(f"  Expected  SHA-256: {expected_sha}")

    if part_sha != expected_sha:
        print("FATAL ERROR: Partition verification failed!")
        os.close(fd)
        sys.exit(1)

    print("\n" + "=" * 60)
    print("  ✓ SUCCESS: boot_b FLASHED AND VERIFIED BIT-EXACT ON eMMC!")
    print("=" * 60)

    # Clean up temporary file
    cmd(fd, "rm -f /tmp/boot_new.img")

    print("\nRebooting device into new kernel...")
    os.write(fd, b"sync && sleep 0.5 && reboot -f\n")
    time.sleep(1.0)
    os.close(fd)
    print("✓ Reboot command sent! Device rebooting...")

if __name__ == "__main__":
    main()
