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
    attrs = termios.tcgetattr(fd)
    attrs[2] |= termios.CLOCAL | termios.CREAD
    attrs[2] &= ~termios.CSIZE
    attrs[2] |= termios.CS8
    attrs[2] &= ~termios.CSTOPB
    attrs[2] &= ~termios.PARENB
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

def run_cmd(fd, cmd, timeout=4.0):
    drain(fd)
    try:
        os.write(fd, cmd.encode() + b'\n')
    except OSError:
        return b''
    out = b''
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
    return out

def backup_partition(fd, name, path, expected_size, chunk_size, out_dir):
    print(f"\n" + "="*50)
    print(f" Backing up {name} ({path}, {expected_size} B)")
    print("="*50)

    # 1. Get remote sha256
    run_cmd(fd, "busybox stty echo")  # ensure prompt visible for command sync
    sha_out = run_cmd(fd, f"sha256sum {path}").decode(errors='replace')
    remote_sha = None
    for l in sha_out.splitlines():
        p = l.strip().split()
        if len(p) == 2 and len(p[0]) == 64 and path in p[1]:
            remote_sha = p[0]
            break

    if not remote_sha:
        print(f"ERROR: Could not get remote SHA-256 for {name}!")
        print(f"Output: {sha_out}")
        return False, None, None

    print(f"Remote SHA-256: {remote_sha}")

    # Check if already backed up and matches remote SHA-256
    out_file = os.path.join(out_dir, f"{name}.img")
    if os.path.exists(out_file) and os.path.getsize(out_file) == expected_size:
        with open(out_file, "rb") as f:
            existing_sha = hashlib.sha256(f.read()).hexdigest()
        if existing_sha == remote_sha:
            print(f"✓ {name}.img ALREADY EXISTS & VERIFIED BIT-EXACT ({existing_sha})")
            return True, existing_sha, out_file

    # 2. Disable echo for chunk transfers
    run_cmd(fd, "busybox stty -echo")

    num_chunks = (expected_size + chunk_size - 1) // chunk_size
    accum = b''
    t0 = time.time()

    for i in range(num_chunks):
        expected_chunk_len = chunk_size if (i < num_chunks - 1 or expected_size % chunk_size == 0) else (expected_size % chunk_size)
        chunk_success = False
        for attempt in range(3):
            try:
                cmd = f"dd if={path} bs={chunk_size} count=1 skip={i} 2>/dev/null | busybox base64"
                raw_out = run_cmd(fd, cmd, timeout=4.0)
                lines = [l.strip() for l in raw_out.splitlines() if not l.strip().startswith(b'/ #')]
                chunk_data = base64.b64decode(b''.join(lines))
                if len(chunk_data) == expected_chunk_len:
                    accum += chunk_data
                    print(f"  Chunk {i+1:2d}/{num_chunks:2d}: {len(chunk_data):7d} B (accum {len(accum):7d} / {expected_size} B)")
                    chunk_success = True
                    break
                else:
                    print(f"  Chunk {i+1} size mismatch ({len(chunk_data)} vs {expected_chunk_len}), retrying...")
            except Exception as e:
                print(f"  Chunk {i+1} error (attempt {attempt+1}/3): {e}")
                time.sleep(0.3)
        if not chunk_success:
            print(f"ERROR: Failed chunk {i+1} after 3 attempts")
            run_cmd(fd, "busybox stty echo")
            return False, None, None

    # 3. Restore echo
    run_cmd(fd, "busybox stty echo")

    # 4. Verification
    if len(accum) != expected_size:
        print(f"ERROR: Size mismatch for {name}! Expected {expected_size}, got {len(accum)}")
        return False, None, None

    local_sha = hashlib.sha256(accum).hexdigest()
    print(f"Local SHA-256:  {local_sha}")

    if local_sha != remote_sha:
        print(f"ERROR: SHA-256 mismatch for {name}!")
        print(f"Expected: {remote_sha}")
        print(f"Got:      {local_sha}")
        return False, None, None

    with open(out_file, "wb") as f:
        f.write(accum)

    print(f"✓ VERIFIED BIT-EXACT & SAVED TO: {out_file} (elapsed: {time.time()-t0:.2f}s)")
    return True, local_sha, out_file

def main():
    port = get_port()
    out_dir = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/scratch/efs_backup"
    os.makedirs(out_dir, exist_ok=True)

    partitions = [
        ("fsc", "/dev/disk/by-partlabel/fsc", 1024, 1024),
        ("modemst1", "/dev/disk/by-partlabel/modemst1", 2097152, 65536),
        ("modemst2", "/dev/disk/by-partlabel/modemst2", 2097152, 65536),
        ("rf_nv", "/dev/disk/by-partlabel/rf_nv", 2097152, 65536),
        ("nvcust", "/dev/disk/by-partlabel/nvcust", 2097152, 65536),
        ("nvdef_b", "/dev/disk/by-partlabel/nvdef_b", 4194304, 65536),
    ]

    print(f"Connecting to target device on {port}...")
    fd = open_serial(port)

    # Wake up prompt
    os.write(fd, b"\n\n\nbusybox stty sane\n")
    time.sleep(0.3)
    drain(fd)

    manifest = []
    all_ok = True

    for name, path, expected_size, chunk_size in partitions:
        ok, sha, out_path = backup_partition(fd, name, path, expected_size, chunk_size, out_dir)
        if not ok:
            all_ok = False
            print(f"\nCRITICAL FAILURE: Backup of {name} failed! Aborting!")
            break
        manifest.append((name, expected_size, sha, out_path))

    os.close(fd)

    if all_ok:
        manifest_file = os.path.join(out_dir, "manifest.sha256")
        with open(manifest_file, "w") as mf:
            for name, sz, sha, out_path in manifest:
                mf.write(f"{sha}  {name}.img\n")

        print("\n" + "="*75)
        print("         ALL EFS PARTITIONS SAFELY BACKED UP AND BIT-EXACT VERIFIED       ")
        print("="*75)
        for name, sz, sha, out_path in manifest:
            print(f"{name:12s} | {sz:8d} B | {sha} | {out_path}")
        print(f"Manifest: {manifest_file}")
    else:
        sys.exit(1)

if __name__ == "__main__":
    main()
