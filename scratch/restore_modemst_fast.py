#!/usr/bin/env python3
"""
Fast restore of modemst1 and modemst2 from host backup files via USB serial.
Uses chunked base64 piped directly to dd for maximum throughput.
"""
import os, sys, time, base64, hashlib, termios, glob

def get_port():
    ports = glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/tty.usbmodem*")
    return ports[0] if ports else None

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
                t = time.time()  # reset on data
        except BlockingIOError:
            time.sleep(0.01)
    return buf

def cmd(fd, command, timeout=5.0, wait=0.1):
    drain(fd, 0.1)
    os.set_blocking(fd, True)
    os.write(fd, command.encode() + b"\n")
    time.sleep(wait)
    return drain(fd, timeout).decode(errors='replace')

def restore_partition(fd, name, target_dev, local_path, expected_sha):
    print(f"\n{'='*60}")
    print(f"Restoring {name} -> {target_dev}")
    print(f"{'='*60}")

    with open(local_path, "rb") as f:
        data = f.read()

    local_sha = hashlib.sha256(data).hexdigest()
    if local_sha != expected_sha:
        print(f"ERROR: Local file hash mismatch! Got {local_sha}, want {expected_sha}")
        sys.exit(1)
    print(f"Local SHA-256 verified: {local_sha}")
    print(f"Size: {len(data)} bytes ({len(data)//1024} KB)")

    # Use a single base64 pipe approach for speed
    # We'll write the entire image as one large base64 heredoc
    b64 = base64.b64encode(data).decode('ascii')
    # Split into 76-char lines for heredoc
    lines = [b64[i:i+76] for i in range(0, len(b64), 76)]
    print(f"Base64 lines: {len(lines)}")

    # Clear any stale file
    out = cmd(fd, f"rm -f /tmp/restore_{name}.bin", timeout=2.0)

    # Stream the base64 heredoc
    print("Uploading via heredoc...")
    os.set_blocking(fd, True)
    os.write(fd, f"cat << 'HEREDOC_EOF' | busybox base64 -d > /tmp/restore_{name}.bin\n".encode())
    t0 = time.time()
    for i, line in enumerate(lines):
        os.write(fd, (line + "\n").encode())
        if i % 500 == 0:
            pct = (i / len(lines)) * 100
            elapsed = time.time() - t0
            speed = (i * 76 / 1024) / max(elapsed, 0.001)
            sys.stdout.write(f"\r  [{pct:.0f}%] {speed:.0f} KB/s    ")
            sys.stdout.flush()
    os.write(fd, b"HEREDOC_EOF\n")
    print(f"\n  Upload done in {time.time()-t0:.1f}s, waiting for decode...")
    time.sleep(3.0)  # wait for decode

    # Verify file size
    out = cmd(fd, f"wc -c /tmp/restore_{name}.bin", timeout=5.0)
    print(f"  wc -c: {out.strip()}")

    # Verify SHA-256 on device
    print("  Verifying SHA-256 on device...")
    out = cmd(fd, f"sha256sum /tmp/restore_{name}.bin", timeout=15.0, wait=2.0)
    device_sha = None
    for line in out.splitlines():
        parts = line.strip().split()
        if len(parts) >= 1 and len(parts[0]) == 64:
            device_sha = parts[0]
            break
    print(f"  Device SHA-256: {device_sha}")
    if device_sha != expected_sha:
        print(f"ERROR: SHA-256 mismatch after upload!")
        print(f"  Expected: {expected_sha}")
        print(f"  Got:      {device_sha}")
        sys.exit(1)
    print("  SHA-256 VERIFIED!")

    # Write to target partition
    print(f"  Writing to {target_dev}...")
    out = cmd(fd, f"dd if=/tmp/restore_{name}.bin of={target_dev} bs=4096 conv=fsync 2>&1 && sync", timeout=20.0, wait=5.0)
    print(f"  dd output: {out.strip()}")

    # Verify partition hash
    print(f"  Final verification of {target_dev}...")
    out = cmd(fd, f"sha256sum {target_dev}", timeout=15.0, wait=3.0)
    part_sha = None
    for line in out.splitlines():
        parts = line.strip().split()
        if len(parts) >= 1 and len(parts[0]) == 64:
            part_sha = parts[0]
            break
    print(f"  Partition SHA-256: {part_sha}")
    if part_sha != expected_sha:
        print(f"ERROR: Partition SHA-256 mismatch!")
        sys.exit(1)
    print(f"  ✓ {name} RESTORED AND VERIFIED on {target_dev}!")
    cmd(fd, f"rm -f /tmp/restore_{name}.bin", timeout=2.0)
    return True

def main():
    port = get_port()
    if not port:
        print("ERROR: Device not found on USB serial. Is the Nokia booted?")
        sys.exit(1)
    print(f"Device found on {port}")

    base_dir = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/scratch/efs_backup"

    partitions = [
        ("modemst1", "/dev/disk/by-partlabel/modemst1",
         os.path.join(base_dir, "modemst1.img"),
         "699df61bacf1bba85b0d5b5894b69e29eaedc75d09cb11f626747f77728807a7"),
        ("modemst2", "/dev/disk/by-partlabel/modemst2",
         os.path.join(base_dir, "modemst2.img"),
         "28762518e5750cf09b72f4673e6a6aba13bd7d7c3419c58dd57f81e19fc9b568"),
    ]

    fd = open_serial(port)
    # flush initial prompt
    os.write(fd, b"\n\n\n")
    time.sleep(0.5)
    drain(fd, 0.5)

    # Test connectivity
    out = cmd(fd, "echo DEVICE_OK", timeout=3.0)
    if "DEVICE_OK" not in out:
        print(f"WARNING: Device may not be responding cleanly. Got: {out[:100]}")

    for name, target_dev, local_path, expected_sha in partitions:
        restore_partition(fd, name, target_dev, local_path, expected_sha)

    os.close(fd)
    print("\n" + "="*60)
    print("  modemst1 + modemst2 RESTORED & VERIFIED SUCCESSFULLY")
    print("="*60)
    print("\nNext: Reboot device with 'python3 scratch/push_reboot_tool.py'")

if __name__ == "__main__":
    main()
