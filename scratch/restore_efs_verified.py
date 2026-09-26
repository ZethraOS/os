#!/usr/bin/env python3
"""
Verified EFS restore tool for Nokia 6.1 Plus.
Wraps base64 at 76-chars to respect Linux TTY line discipline (MAX_CANON 4096).
Uses per-chunk explicit ACK tokens for guaranteed sync and fast throughput.
"""
import os, sys, time, glob, base64, hashlib, termios

CHUNK_SIZE = 32768  # 32 KB per chunk (64 chunks = 2MB)

def get_port():
    ports = glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/tty.usbmodem*")
    return ports[0] if ports else None

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

def drain(fd):
    while True:
        try:
            if not os.read(fd, 65536): break
        except (BlockingIOError, OSError):
            break

def run_cmd(fd, cmd, timeout=8.0):
    drain(fd)
    os.write(fd, cmd.encode() + b'\n')
    out = b""
    t0 = time.time()
    while time.time() - t0 < timeout:
        time.sleep(0.01)
        try:
            c = os.read(fd, 65536)
            if c:
                out += c
                if b'/ # ' in out[-10:]: break
        except BlockingIOError:
            if b'/ # ' in out[-10:]: break
        except OSError:
            break
    return out.decode(errors='replace')

def restore_partition(fd, name, target_dev, local_path, expected_sha):
    print(f"\n{'='*65}")
    print(f" Restoring {name} -> {target_dev}")
    print(f"{'='*65}")

    with open(local_path, "rb") as f:
        data = f.read()

    local_sha = hashlib.sha256(data).hexdigest()
    if local_sha != expected_sha:
        print(f"ERROR: Local SHA-256 mismatch! Got {local_sha}, expected {expected_sha}")
        return False
    print(f"  Local file: {len(data)} bytes ({len(data)//1024} KB), SHA-256 verified.")

    # Remove any stale remote file
    run_cmd(fd, f"rm -f /tmp/{name}.bin", timeout=2.0)

    total_chunks = (len(data) + CHUNK_SIZE - 1) // CHUNK_SIZE
    print(f"  Streaming {total_chunks} chunks ({CHUNK_SIZE//1024} KB each) with line wrapping & ACK...")

    t0 = time.time()
    for i in range(total_chunks):
        chunk = data[i * CHUNK_SIZE : (i + 1) * CHUNK_SIZE]
        raw_b64 = base64.b64encode(chunk).decode('ascii')
        # Wrap into 76-character lines to avoid TTY line discipline buffer truncation
        lines = [raw_b64[j : j + 76] for j in range(0, len(raw_b64), 76)]
        b64_wrapped = "\n".join(lines) + "\n"

        drain(fd)
        ack_tag = f"ACK_{name}_{i}"
        payload = (
            f"cat << 'EOF_CHUNK' | busybox base64 -d >> /tmp/{name}.bin\n"
            f"{b64_wrapped}"
            f"EOF_CHUNK\n"
            f"echo {ack_tag}\n"
        )
        os.write(fd, payload.encode())

        # Wait specifically for ack_tag
        buf = b""
        t_wait = time.time()
        ack_found = False
        while time.time() - t_wait < 5.0:
            try:
                c = os.read(fd, 65536)
                if c:
                    buf += c
                    if ack_tag.encode() in buf:
                        ack_found = True
                        break
            except BlockingIOError:
                time.sleep(0.01)
            except OSError:
                break

        if not ack_found:
            print(f"\n  ERROR: Timeout waiting for {ack_tag}! Last buffer: {buf[-100:]}")
            return False

        pct = ((i + 1) / total_chunks) * 100
        elapsed = time.time() - t0
        speed = ((i + 1) * CHUNK_SIZE / 1024) / max(elapsed, 0.001)
        sys.stdout.write(f"\r  [{pct:5.1f}%] Chunk {i+1:2d}/{total_chunks} ({speed:5.1f} KB/s)   ")
        sys.stdout.flush()

    total_time = time.time() - t0
    print(f"\n  Upload completed in {total_time:.1f}s ({len(data)/1024/total_time:.1f} KB/s average)")

    # 1. Verify size
    out = run_cmd(fd, f"wc -c /tmp/{name}.bin", timeout=3.0)
    print(f"  Remote size: {out.strip()}")

    # 2. Verify SHA-256 of uploaded file
    print("  Verifying uploaded file SHA-256...")
    out = run_cmd(fd, f"sha256sum /tmp/{name}.bin", timeout=10.0)
    remote_sha = None
    for line in out.splitlines():
        parts = line.strip().split()
        if parts and len(parts[0]) == 64:
            remote_sha = parts[0]
            break
    print(f"  Uploaded SHA-256: {remote_sha}")
    if remote_sha != expected_sha:
        print(f"  ERROR: Checksum mismatch! Expected {expected_sha}")
        return False
    print("  ✓ Uploaded file checksum matches 100%!")

    # 3. Write to eMMC partition
    print(f"  Writing to {target_dev} via dd...")
    out = run_cmd(fd, f"dd if=/tmp/{name}.bin of={target_dev} bs=65536 conv=fsync 2>&1 && sync", timeout=15.0)
    print(f"  dd: {out.strip()}")

    # 4. Verify partition SHA-256 on eMMC
    print(f"  Verifying {target_dev} on eMMC...")
    out = run_cmd(fd, f"sha256sum {target_dev}", timeout=10.0)
    part_sha = None
    for line in out.splitlines():
        parts = line.strip().split()
        if parts and len(parts[0]) == 64:
            part_sha = parts[0]
            break
    print(f"  eMMC SHA-256:     {part_sha}")
    if part_sha != expected_sha:
        print(f"  ERROR: eMMC partition checksum mismatch!")
        return False

    # 5. Hexdump superblock magic
    out = run_cmd(fd, f"hexdump -C {target_dev} | head -n 4", timeout=3.0)
    print(f"  Superblock check:\n{out}")

    print(f"  ✓✓ {name} RESTORED & VERIFIED 100% BIT-EXACT ON {target_dev}!")
    run_cmd(fd, f"rm -f /tmp/{name}.bin", timeout=2.0)
    return True

def main():
    port = get_port()
    if not port:
        print("ERROR: Device not found on /dev/cu.usbmodem*")
        sys.exit(1)
    print(f"Using serial port: {port}")

    fd = open_serial(port)
    os.write(fd, b"\n\n\n")
    time.sleep(0.2)
    drain(fd)

    partitions = [
        ("modemst1", "/dev/disk/by-partlabel/modemst1",
         "scratch/efs_backup/modemst1.img",
         "699df61bacf1bba85b0d5b5894b69e29eaedc75d09cb11f626747f77728807a7"),
        ("modemst2", "/dev/disk/by-partlabel/modemst2",
         "scratch/efs_backup/modemst2.img",
         "28762518e5750cf09b72f4673e6a6aba13bd7d7c3419c58dd57f81e19fc9b568"),
    ]

    for name, dev, path, sha in partitions:
        ok = restore_partition(fd, name, dev, path, sha)
        if not ok:
            print(f"\nFATAL: Restore failed for {name}!")
            os.close(fd)
            sys.exit(1)

    os.close(fd)
    print("\n" + "="*65)
    print(" SUCCESS: modemst1 and modemst2 restored and verified on eMMC!")
    print("="*65)

if __name__ == "__main__":
    main()
