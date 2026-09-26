import os
import sys
import time
import base64
import hashlib
import glob

def main():
    ports = glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/tty.usbmodem*")
    if not ports:
        print("Error: No usbmodem serial port found")
        sys.exit(1)
    port = ports[0]

    out_dir = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/scratch/efs_backup"
    os.makedirs(out_dir, exist_ok=True)

    partitions = [
        ("modemst1", "/dev/disk/by-partlabel/modemst1", 2097152),
        ("modemst2", "/dev/disk/by-partlabel/modemst2", 2097152),
        ("fsc", "/dev/disk/by-partlabel/fsc", 1024),
        ("nvdef_b", "/dev/disk/by-partlabel/nvdef_b", 4194304),
        ("rf_nv", "/dev/disk/by-partlabel/rf_nv", 2097152),
        ("nvcust", "/dev/disk/by-partlabel/nvcust", 2097152),
    ]

    print(f"Connecting to device on {port}...")
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY)
    
    # Configure device shell: disable echo and set raw mode
    os.write(fd, b"\n\nbusybox stty -echo raw 2>/dev/null || true\n")
    time.sleep(0.3)
    
    # Drain any initial output
    os.set_blocking(fd, False)
    while True:
        try:
            if not os.read(fd, 65536): break
        except BlockingIOError:
            break
    os.set_blocking(fd, True)

    results = []

    for name, path, expected_size in partitions:
        print(f"\n--- Backing up {name} ({path}, {expected_size} bytes) ---")
        
        # 1. Get remote sha256
        # Drain buffer first
        os.set_blocking(fd, False)
        while True:
            try:
                if not os.read(fd, 65536): break
            except BlockingIOError:
                break
        os.set_blocking(fd, True)

        os.write(fd, f"sha256sum {path}; echo '___SHA_DONE___'\n".encode())
        resp = b""
        t0 = time.time()
        while time.time() - t0 < 5.0:
            time.sleep(0.05)
            try:
                os.set_blocking(fd, False)
                chunk = os.read(fd, 4096)
                if chunk:
                    resp += chunk
                    if b"___SHA_DONE___" in resp:
                        break
            except BlockingIOError:
                pass
        os.set_blocking(fd, True)
        
        remote_sha = None
        for line in resp.decode(errors='replace').splitlines():
            parts = line.strip().split()
            if len(parts) == 2 and len(parts[0]) == 64 and path in parts[1]:
                remote_sha = parts[0]
                break
        print(f"Remote SHA-256: {remote_sha}")

        # 2. Dump base64 to /tmp/cur_b64.txt
        os.write(fd, f"busybox base64 {path} > /tmp/cur_b64.txt && echo '___GEN_DONE___'\n".encode())
        
        t0 = time.time()
        ready = False
        resp_gen = b""
        while time.time() - t0 < 15.0:
            time.sleep(0.1)
            try:
                os.set_blocking(fd, False)
                chk = os.read(fd, 4096)
                if chk:
                    resp_gen += chk
                    if b"___GEN_DONE___" in resp_gen:
                        ready = True
                        break
            except BlockingIOError:
                pass
        os.set_blocking(fd, True)
        
        if not ready:
            print(f"Timeout creating base64 file for {name}")
            continue

        # 3. Stream /tmp/cur_b64.txt with EOF marker
        # Base64 never contains '_' so '___STREAM_EOF___' is unique
        print(f"Streaming /tmp/cur_b64.txt for {name}...")
        os.write(fd, b"cat /tmp/cur_b64.txt; echo '___STREAM_EOF___'\n")
        
        b64_accum = b""
        t0 = time.time()
        no_data_time = time.time()
        
        os.set_blocking(fd, False)
        while True:
            try:
                chunk = os.read(fd, 65536)
                if chunk:
                    no_data_time = time.time()
                    b64_accum += chunk
                    if b"___STREAM_EOF___" in b64_accum:
                        break
                else:
                    if time.time() - no_data_time > 8.0:
                        print("Timeout waiting for stream data")
                        break
                    time.sleep(0.01)
            except BlockingIOError:
                if time.time() - no_data_time > 8.0:
                    print("Timeout waiting for stream data")
                    break
                time.sleep(0.01)
        os.set_blocking(fd, True)

        eof_idx = b64_accum.find(b"___STREAM_EOF___")
        if eof_idx == -1:
            print(f"Error: EOF marker not found for {name}")
            continue

        raw_b64_bytes = b64_accum[:eof_idx].strip()
        # Remove any leading command echoes or whitespace
        lines = [line.strip() for line in raw_b64_bytes.splitlines() if line.strip()]
        # Filter only valid base64 lines (no shell prompts)
        valid_b64 = b"".join(line for line in lines if not line.startswith(b"/ #") and not line.startswith(b"cat "))
        
        binary_data = base64.b64decode(valid_b64)
        local_sha = hashlib.sha256(binary_data).hexdigest()
        print(f"Local SHA-256:  {local_sha}")
        print(f"Received {len(binary_data)} bytes in {time.time()-t0:.2f}s")

        if len(binary_data) != expected_size:
            print(f"ERROR: Size mismatch for {name}! Expected {expected_size}, got {len(binary_data)}")
            sys.exit(1)

        if remote_sha and local_sha != remote_sha:
            print(f"ERROR: SHA-256 MISMATCH for {name}!")
            print(f"Expected: {remote_sha}")
            print(f"Got:      {local_sha}")
            sys.exit(1)

        out_path = os.path.join(out_dir, f"{name}.img")
        with open(out_path, "wb") as f:
            f.write(binary_data)
        print(f"✓ Saved to {out_path} (VERIFIED BIT-EXACT)")
        results.append((name, len(binary_data), local_sha, out_path))

        # Cleanup tmp
        os.write(fd, b"rm -f /tmp/cur_b64.txt\n")
        time.sleep(0.1)

    os.write(fd, b"busybox stty echo -raw 2>/dev/null || true\n")
    os.close(fd)

    print("\n==========================================")
    print("      EFS BACKUP SUMMARY (ALL VERIFIED)   ")
    print("==========================================")
    for name, sz, sha, path in results:
        print(f"{name:12s} | {sz:8d} B | {sha} | {path}")

if __name__ == "__main__":
    main()
