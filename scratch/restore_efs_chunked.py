#!/usr/bin/env python3
"""
Reliable, chunked EFS partition restore tool for Nokia 6.1 Plus.
Sends data in small, verified 32KB chunks with round-trip shell ACK.
"""
import os, sys, time, glob, base64, hashlib, termios

CHUNK_SIZE = 32768  # 32 KB binary per chunk

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

def run_cmd(fd, command, timeout=6.0):
    drain(fd)
    try:
        os.write(fd, command.encode() + b'\n')
    except OSError as e:
        print(f"Write error: {e}")
        return ""
    out = b""
    t0 = time.time()
    while time.time() - t0 < timeout:
        time.sleep(0.01)
        try:
            c = os.read(fd, 65536)
            if c:
                out += c
                if b'/ # ' in out[-10:]:
                    break
        except BlockingIOError:
            if b'/ # ' in out[-10:]:
                break
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
    print(f"  Local file verified: {len(data)} bytes, SHA-256: {local_sha}")

    # Remove stale temp file
    run_cmd(fd, f"rm -f /tmp/{name}.bin", timeout=2.0)

    # Split data into 32KB chunks
    total_chunks = (len(data) + CHUNK_SIZE - 1) // CHUNK_SIZE
    print(f"  Uploading in {total_chunks} chunk(s) ({CHUNK_SIZE // 1024} KB each)...")

    t_start = time.time()
    for i in range(total_chunks):
        chunk = data[i * CHUNK_SIZE : (i + 1) * CHUNK_SIZE]
        b64_chunk = base64.b64encode(chunk).decode('ascii')
        
        # Write chunk heredoc
        drain(fd)
        cmd_head = f"cat << 'EOF_C' | busybox base64 -d >> /tmp/{name}.bin\n"
        os.write(fd, cmd_head.encode() + b64_chunk.encode() + b"\nEOF_C\n")
        
        # Wait for shell ACK
        out = b""
        t0 = time.time()
        while time.time() - t0 < 4.0:
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
        
        pct = ((i + 1) / total_chunks) * 100
        elapsed = time.time() - t_start
        speed = ((i + 1) * CHUNK_SIZE / 1024) / max(elapsed, 0.001)
        sys.stdout.write(f"\r  [{pct:5.1f}%] Chunk {i+1:2d}/{total_chunks} ({speed:5.1f} KB/s)   ")
        sys.stdout.flush()

    print(f"\n  Upload complete in {time.time()-t_start:.1f}s")

    # 1. Verify size on device
    out = run_cmd(fd, f"wc -c /tmp/{name}.bin", timeout=3.0)
    print(f"  Remote size check: {out.strip()}")

    # 2. Verify SHA-256 of uploaded file
    print("  Verifying uploaded file hash...")
    out = run_cmd(fd, f"sha256sum /tmp/{name}.bin", timeout=10.0)
    remote_sha = None
    for line in out.splitlines():
        parts = line.strip().split()
        if parts and len(parts[0]) == 64:
            remote_sha = parts[0]
            break
    print(f"  Remote file SHA-256: {remote_sha}")
    if remote_sha != expected_sha:
        print(f"  ERROR: Uploaded file checksum mismatch!")
        return False
    print("  ✓ Uploaded file hash matches perfectly!")

    # 3. Write to target partition
    print(f"  Writing to {target_dev}...")
    out = run_cmd(fd, f"dd if=/tmp/{name}.bin of={target_dev} bs=65536 conv=fsync 2>&1 && sync", timeout=15.0)
    print(f"  dd output: {out.strip()}")

    # 4. Verify partition SHA-256 on eMMC
    print(f"  Verifying {target_dev} partition hash on eMMC...")
    out = run_cmd(fd, f"sha256sum {target_dev}", timeout=10.0)
    part_sha = None
    for line in out.splitlines():
        parts = line.strip().split()
        if parts and len(parts[0]) == 64:
            part_sha = parts[0]
            break
    print(f"  Partition SHA-256:   {part_sha}")
    if part_sha != expected_sha:
        print(f"  ERROR: eMMC partition checksum mismatch!")
        return False

    print(f"  ✓✓ {name} RESTORED & VERIFIED 100% BIT-EXACT ON {target_dev}!")
    run_cmd(fd, f"rm -f /tmp/{name}.bin", timeout=2.0)
    return True

def main():
    port = get_port()
    if not port:
        print("ERROR: Device not found on /dev/cu.usbmodem*")
        sys.exit(1)
    print(f"Connected to device on: {port}")

    fd = open_serial(port)
    os.write(fd, b"\n\n\n")
    time.sleep(0.2)
    drain(fd)

    # Quick ping
    ping = run_cmd(fd, "echo DEVICE_READY", timeout=2.0)
    if "DEVICE_READY" not in ping:
        print("Warning: Initial ping did not return cleanly, retrying...")
        os.write(fd, b"\n\n")
        time.sleep(0.5)

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
            print(f"\nFATAL: Failed to restore {name}!")
            os.close(fd)
            sys.exit(1)

    os.close(fd)
    print("\n" + "="*65)
    print(" ALL PARTITIONS RESTORED AND VERIFIED SUCCESSFULLY!")
    print("="*65)

if __name__ == "__main__":
    main()
