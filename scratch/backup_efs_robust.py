import os
import sys
import time
import termios
import glob
import base64
import hashlib

def get_port():
    ports = glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/tty.usbmodem*")
    return ports[0] if ports else "/dev/cu.usbmodem2101"

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

def run_cmd(fd, cmd, inactivity_timeout=1.5, max_total_time=60.0):
    # Flush input buffer
    os.set_blocking(fd, False)
    while True:
        try:
            if not os.read(fd, 65536): break
        except BlockingIOError:
            break

    # Send command
    os.set_blocking(fd, True)
    os.write(fd, b"\n\n\n")
    time.sleep(0.2)
    os.write(fd, cmd.encode() + b"\n")

    # Read output with non-blocking tight loop for max throughput
    os.set_blocking(fd, False)
    response = b""
    t0 = time.time()
    last_data_time = time.time()
    has_data = False

    while (time.time() - t0) < max_total_time:
        try:
            chunk = os.read(fd, 65536)
            if chunk:
                response += chunk
                last_data_time = time.time()
                has_data = True
                continue  # Read immediately while data is streaming!
        except BlockingIOError:
            pass

        # Check for inactivity only after data has started arriving
        if has_data and (time.time() - last_data_time > inactivity_timeout):
            break

        time.sleep(0.002)

    return response

def main():
    port = get_port()
    out_dir = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/scratch/efs_backup"
    os.makedirs(out_dir, exist_ok=True)

    partitions = [
        ("fsc", "/dev/disk/by-partlabel/fsc", 1024),
        ("modemst1", "/dev/disk/by-partlabel/modemst1", 2097152),
        ("modemst2", "/dev/disk/by-partlabel/modemst2", 2097152),
        ("rf_nv", "/dev/disk/by-partlabel/rf_nv", 2097152),
        ("nvcust", "/dev/disk/by-partlabel/nvcust", 2097152),
        ("nvdef_b", "/dev/disk/by-partlabel/nvdef_b", 4194304),
    ]

    print(f"Connecting to device on {port}...")
    fd = open_serial(port)

    # Initial check
    init_out = run_cmd(fd, "echo DEVICE_READY", inactivity_timeout=1.0)
    if b"DEVICE_READY" not in init_out:
        print("Failed to get initial prompt. Trying stty sane...")
        run_cmd(fd, "busybox stty sane", inactivity_timeout=1.0)

    results = []

    for name, path, expected_size in partitions:
        print(f"\n==========================================")
        print(f" Backing up: {name} ({path}, {expected_size} bytes)")
        print(f"==========================================")

        # 1. Remote SHA-256
        sha_out = run_cmd(fd, f"sha256sum {path}", inactivity_timeout=1.5).decode(errors='replace')
        remote_sha = None
        for line in sha_out.splitlines():
            parts = line.strip().split()
            if len(parts) == 2 and len(parts[0]) == 64 and path in parts[1]:
                remote_sha = parts[0]
                break
        
        if not remote_sha:
            print(f"ERROR: Could not get SHA-256 for {name}!")
            print(f"Output was: {sha_out}")
            sys.exit(1)
        print(f"Remote SHA-256: {remote_sha}")

        # 2. Base64 dump
        t0 = time.time()
        timeout = 2.0 if expected_size <= 1024 else 3.0
        max_time = 30.0 if expected_size <= 2097152 else 60.0
        
        b64_out = run_cmd(fd, f"busybox base64 {path}", inactivity_timeout=timeout, max_total_time=max_time)
        print(f"Transferred {len(b64_out)} bytes over serial in {time.time()-t0:.2f}s")

        # 3. Parse base64 lines
        b64_lines = []
        for line in b64_out.decode(errors='replace').splitlines():
            l = line.strip()
            # Valid base64 characters only
            if l and len(l) >= 4 and all(c in 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=' for c in l):
                b64_lines.append(l)

        raw_b64 = ''.join(b64_lines)
        binary_data = base64.b64decode(raw_b64)
        local_sha = hashlib.sha256(binary_data).hexdigest()
        print(f"Decoded size:   {len(binary_data)} bytes (expected {expected_size})")
        print(f"Local SHA-256:  {local_sha}")

        if len(binary_data) != expected_size:
            print(f"ERROR: Size mismatch for {name}! Expected {expected_size}, got {len(binary_data)}")
            sys.exit(1)

        if local_sha != remote_sha:
            print(f"ERROR: SHA-256 mismatch for {name}!")
            print(f"Remote: {remote_sha}")
            print(f"Local:  {local_sha}")
            sys.exit(1)

        out_path = os.path.join(out_dir, f"{name}.img")
        with open(out_path, "wb") as f:
            f.write(binary_data)
        print(f"✓ VERIFIED BIT-EXACT & SAVED TO: {out_path}")
        results.append((name, len(binary_data), local_sha, out_path))

    os.close(fd)

    print("\n" + "="*70)
    print("           ALL EFS PARTITIONS SUCCESSFULLY BACKED UP & VERIFIED")
    print("="*70)
    for name, sz, sha, path in results:
        print(f"{name:12s} | {sz:8d} B | {sha} | {path}")

if __name__ == "__main__":
    main()
