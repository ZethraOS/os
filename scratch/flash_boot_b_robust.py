#!/usr/bin/env python3
"""
Robust, full-duplex boot_b flasher over USB CDC-ACM serial.
Drains incoming buffer continuously to prevent USB FIFO saturation.
"""
import os
import sys
import time
import base64
import hashlib
import termios
import glob
import select

BOOT_IMG_PATH = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/build/out/boot.img"
TARGET_PART = "/dev/disk/by-partlabel/boot_b"

def get_port():
    ports = glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/tty.usbmodem*")
    return sorted(ports)[0] if ports else None

def open_serial(port):
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
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
    buf = b""
    t = time.time()
    while time.time() - t < timeout:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                c = os.read(fd, 65536)
                if c:
                    buf += c
                    t = time.time()
            except BlockingIOError:
                pass
        else:
            break
    return buf

def cmd(fd, command, timeout=10.0, wait=0.2):
    drain(fd, 0.1)
    os.write(fd, command.encode() + b"\n")
    time.sleep(wait)
    resp = b""
    t = time.time()
    while time.time() - t < timeout:
        r, _, _ = select.select([fd], [], [], 0.2)
        if r:
            try:
                c = os.read(fd, 65536)
                if c:
                    resp += c
                    if b"/ # " in resp or b"~ # " in resp:
                        break
            except BlockingIOError:
                pass
        else:
            if resp:
                break
    return resp.decode(errors='replace')

def main():
    print("=" * 60)
    print("  ZethraOS Robust Direct boot_b Flasher")
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
        print("Error: No /dev/cu.usbmodem* port found!")
        sys.exit(1)
    print(f"Opening port {port}...")

    fd = open_serial(port)
    drain(fd, 0.3)
    os.write(fd, b"\n\n\n")
    time.sleep(0.2)
    drain(fd, 0.2)

    # Verify device handshake
    out = cmd(fd, "uname -a; ls -l " + TARGET_PART)
    print("Device handshake:")
    print(out.strip())
    if "Linux" not in out or "boot_b" not in out:
        print("Error: Device handshake failed!")
        sys.exit(1)

    print("\n[Step 1/5] Disabling TTY echo on device and preparing /tmp...")
    cmd(fd, "stty -F /dev/ttyGS0 -echo raw; rm -f /tmp/boot_new.*")

    print("[Step 2/5] Preparing base64 stream...")
    b64 = base64.b64encode(boot_data).decode('ascii')
    lines = [b64[i:i+76] for i in range(0, len(b64), 76)]
    total_lines = len(lines)
    print(f"Total lines: {total_lines}")

    # Launch decoder on device
    drain(fd, 0.1)
    os.write(fd, b"cat << 'HEREDOC_EOF' | busybox base64 -d > /tmp/boot_new.img\n")
    time.sleep(0.1)

    print("[Step 3/5] Streaming base64 payload to device (full-duplex drain)...")
    t0 = time.time()
    for i, line in enumerate(lines):
        # Wait until port is writable with retries
        for retry in range(15):
            _, w, _ = select.select([], [fd], [], 3.0)
            if w:
                break
            time.sleep(0.05)
        else:
            print(f"\nWrite timeout at line {i}!")
            sys.exit(1)

        os.write(fd, (line + "\n").encode())

        # Thoroughly drain incoming buffer every 10 lines
        if i % 10 == 0:
            while True:
                r, _, _ = select.select([fd], [], [], 0.0)
                if r:
                    try:
                        c = os.read(fd, 65536)
                        if not c:
                            break
                    except BlockingIOError:
                        break
                else:
                    break

        # Progress display
        if i % 1000 == 0 or i == total_lines - 1:
            pct = ((i + 1) / total_lines) * 100
            elapsed = time.time() - t0
            kb_sent = (i + 1) * 76 / 1024
            speed = kb_sent / max(elapsed, 0.001)
            sys.stdout.write(f"\r  Progress: {pct:5.1f}% | {kb_sent:6.0f} KB sent | {speed:5.0f} KB/s")
            sys.stdout.flush()

    # Send heredoc terminator
    time.sleep(0.1)
    os.write(fd, b"HEREDOC_EOF\n")
    total_time = time.time() - t0
    print(f"\n  Upload complete in {total_time:.1f}s ({local_size / 1024 / max(total_time, 0.001):.0f} KB/s)")

    # Restore normal terminal mode on device
    time.sleep(1.0)
    os.write(fd, b"\nstty -F /dev/ttyGS0 sane\n")
    time.sleep(0.5)
    drain(fd, 0.5)

    print("\n[Step 4/5] Verifying decoded image on device...")
    size_out = cmd(fd, "wc -c /tmp/boot_new.img", timeout=5.0)
    print(f"  Device wc -c: {size_out.strip()}")

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
        print("FATAL ERROR: SHA-256 checksum mismatch!")
        cmd(fd, "rm -f /tmp/boot_new.img")
        os.close(fd)
        sys.exit(1)
    print("  ✓ SHA-256 verified bit-exact!")

    print(f"\n[Step 5/5] Flashing {TARGET_PART} on eMMC...")
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
    print("  ✓ SUCCESS: boot_b FLASHED AND VERIFIED BIT-EXACT ON eMMC!")
    print("=" * 60)

    # Clean up temporary file
    cmd(fd, "rm -f /tmp/boot_new.img")

    print("\nRebooting device into new kernel...")
    os.write(fd, b"sync && sleep 0.5 && reboot -f\n")
    time.sleep(1.0)
    os.close(fd)
    print("Reboot command sent successfully!")

if __name__ == "__main__":
    main()
