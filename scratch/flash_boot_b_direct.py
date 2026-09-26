#!/usr/bin/env python3
"""
Directly and reliably flash boot_b partition from live Linux via USB CDC-ACM serial.
Uses raw stty + head -c streaming for high-speed, deadlock-free transfer.
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

def drain(fd, timeout=0.5):
    os.set_blocking(fd, False)
    buf = b""
    t = time.time()
    while time.time() - t < timeout:
        try:
            chunk = os.read(fd, 65536)
            if chunk:
                buf += chunk
                t = time.time()
        except BlockingIOError:
            time.sleep(0.01)
    return buf

def cmd(fd, command, timeout=10.0, wait=0.1):
    drain(fd, 0.1)
    os.set_blocking(fd, True)
    os.write(fd, command.encode() + b"\n")
    time.sleep(wait)
    return drain(fd, timeout).decode(errors='replace')

def main():
    print("=" * 60)
    print("  ZethraOS Direct boot_b Flasher (High-Speed Raw Stream)")
    print("=" * 60)

    if not os.path.exists(BOOT_IMG_PATH):
        print(f"Error: {BOOT_IMG_PATH} does not exist!")
        sys.exit(1)

    with open(BOOT_IMG_PATH, "rb") as f:
        boot_data = f.read()

    local_size = len(boot_data)
    local_sha = hashlib.sha256(boot_data).hexdigest()
    block_count_4k = local_size // 4096

    print(f"File:       {BOOT_IMG_PATH}")
    print(f"Size:       {local_size} bytes ({local_size / 1024 / 1024:.2f} MB)")
    print(f"SHA-256:    {local_sha}")
    print(f"Target:     {TARGET_PART}")
    print(f"4K Blocks:  {block_count_4k}")

    port = get_port()
    if not port:
        print("Waiting for /dev/cu.usbmodem* serial port...")
        for _ in range(30):
            time.sleep(1)
            port = get_port()
            if port:
                break
        if not port:
            print("Error: Device serial port not found after 30s!")
            sys.exit(1)

    print(f"Connecting to device via {port}...")
    fd = open_serial(port)
    os.write(fd, b"\n\x03\n\x03\n")
    time.sleep(0.3)
    drain(fd, 0.3)

    out = cmd(fd, "uname -a; ls -l " + TARGET_PART)
    print("Device handshake:")
    print(out.strip())
    if "Linux" not in out or "boot_b" not in out:
        print("Error: Device did not respond with expected system prompt!")
        sys.exit(1)

    print("\n[Step 1/5] Preparing device /tmp and disabling TTY echo...")
    cmd(fd, "rm -f /tmp/boot_new.b64 /tmp/boot_new.img")
    
    b64_data = base64.b64encode(boot_data)
    b64_len = len(b64_data)
    print(f"Base64 stream length: {b64_len} bytes")

    # Command device to capture exact byte count in raw mode without echo
    print("[Step 2/5] Initiating raw stream capture on device...")
    os.write(fd, f"stty -F /dev/ttyGS0 -echo raw && head -c {b64_len} > /tmp/boot_new.b64 && stty -F /dev/ttyGS0 sane\n".encode())
    time.sleep(0.3)

    print("[Step 3/5] Streaming base64 data to device...")
    t0 = time.time()
    chunk_size = 65536
    total_sent = 0

    while total_sent < b64_len:
        chunk = b64_data[total_sent:total_sent + chunk_size]
        os.write(fd, chunk)
        total_sent += len(chunk)
        elapsed = time.time() - t0
        speed = (total_sent / 1024) / max(elapsed, 0.001)
        pct = (total_sent / b64_len) * 100
        sys.stdout.write(f"\r  Progress: {pct:5.1f}% | {total_sent / 1024:6.0f} KB sent | {speed:5.0f} KB/s")
        sys.stdout.flush()

    total_time = time.time() - t0
    print(f"\n  Upload finished in {total_time:.2f}s ({local_size / 1024 / max(total_time, 0.001):.0f} KB/s)")

    # Send a newline and small delay to allow device to return to shell
    time.sleep(0.5)
    os.write(fd, b"\n")
    time.sleep(0.3)
    drain(fd, 0.3)

    print("\n[Step 4/5] Decoding and verifying SHA-256 on device...")
    cmd(fd, "busybox base64 -d /tmp/boot_new.b64 > /tmp/boot_new.img", timeout=10.0, wait=1.0)
    
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
    print(f"  Local  SHA-256: {local_sha}")

    if device_sha != local_sha:
        print("FATAL ERROR: SHA-256 checksum mismatch on uploaded image!")
        cmd(fd, "rm -f /tmp/boot_new.b64 /tmp/boot_new.img")
        os.close(fd)
        sys.exit(1)
    print("  ✓ Image checksum verified bit-exact!")

    print(f"\n[Step 5/5] Writing to {TARGET_PART} on eMMC...")
    dd_out = cmd(fd, f"dd if=/tmp/boot_new.img of={TARGET_PART} bs=64k conv=fsync 2>&1 && sync", timeout=30.0, wait=2.0)
    print(f"  dd output:\n{dd_out.strip()}")

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
    print(f"  Expected  SHA-256: {local_sha}")

    if part_sha != local_sha:
        print("FATAL ERROR: Partition verification failed!")
        os.close(fd)
        sys.exit(1)

    print("\n" + "=" * 60)
    print("  ✓ SUCCESS: boot_b FLASHED AND VERIFIED BIT-EXACT!")
    print("=" * 60)

    # Clean up temporary files
    cmd(fd, "rm -f /tmp/boot_new.b64 /tmp/boot_new.img")

    print("\nRebooting device into new kernel...")
    os.write(fd, b"sync && sleep 0.5 && reboot -f\n")
    time.sleep(1.0)
    os.close(fd)
    print("Reboot issued. Done!")

if __name__ == "__main__":
    main()
