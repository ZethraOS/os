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

def run_cmd(fd, cmd, inactivity_timeout=1.0, max_total_time=15.0):
    os.set_blocking(fd, False)
    while True:
        try:
            if not os.read(fd, 65536): break
        except BlockingIOError:
            break

    os.set_blocking(fd, True)
    os.write(fd, b"\n\n\n")
    time.sleep(0.1)
    os.write(fd, cmd.encode() + b"\n")

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
                continue
        except BlockingIOError:
            pass

        if has_data and (time.time() - last_data_time > inactivity_timeout):
            break
        time.sleep(0.002)

    return response

def restore_partition(fd, name, target_dev, local_path, expected_sha):
    print(f"\n==========================================")
    print(f" Restoring {name} to {target_dev}")
    print(f" Source: {local_path}")
    print(f" Expected SHA-256: {expected_sha}")
    print(f"==========================================")

    with open(local_path, "rb") as f:
        data = f.read()

    local_sha = hashlib.sha256(data).hexdigest()
    if local_sha != expected_sha:
        print(f"ERROR: Local file hash mismatch! Got {local_sha}")
        sys.exit(1)

    chunk_size = 65536
    total_chunks = (len(data) + chunk_size - 1) // chunk_size

    # Clean remote temp file
    run_cmd(fd, f"rm -f /tmp/restore_{name}.bin")

    t0 = time.time()
    for i in range(total_chunks):
        chunk = data[i * chunk_size : (i + 1) * chunk_size]
        b64 = base64.b64encode(chunk).decode('ascii')
        
        # Write b64 chunk
        cmd = f"cat << 'EOF' | busybox base64 -d >> /tmp/restore_{name}.bin\n{b64}\nEOF"
        out = run_cmd(fd, cmd, inactivity_timeout=0.3, max_total_time=5.0)
        sys.stdout.write(f"\rUploaded chunk {i+1}/{total_chunks} ({((i+1)/total_chunks)*100:.1f}%)")
        sys.stdout.flush()

    print(f"\nUpload completed in {time.time()-t0:.2f}s")

    # Verify remote hash of uploaded file
    sha_out = run_cmd(fd, f"sha256sum /tmp/restore_{name}.bin").decode(errors='replace')
    remote_sha = None
    for line in sha_out.splitlines():
        parts = line.strip().split()
        if len(parts) == 2 and len(parts[0]) == 64 and f"restore_{name}.bin" in parts[1]:
            remote_sha = parts[0]
            break

    print(f"Remote staging file SHA-256: {remote_sha}")
    if remote_sha != expected_sha:
        print(f"ERROR: Staged file SHA-256 mismatch!")
        sys.exit(1)

    # Write to target partition
    print(f"Writing staged file to {target_dev}...")
    dd_out = run_cmd(fd, f"dd if=/tmp/restore_{name}.bin of={target_dev} bs=4096 conv=fsync && sync", max_total_time=15.0).decode(errors='replace')
    print(dd_out)

    # Verify remote partition hash directly from eMMC
    print(f"Verifying target partition {target_dev} SHA-256...")
    part_sha_out = run_cmd(fd, f"sha256sum {target_dev}", max_total_time=15.0).decode(errors='replace')
    part_sha = None
    for line in part_sha_out.splitlines():
        parts = line.strip().split()
        if len(parts) == 2 and len(parts[0]) == 64:
            part_sha = parts[0]
            break

    print(f"Target partition SHA-256:    {part_sha}")
    if part_sha != expected_sha:
        print(f"ERROR: Target partition SHA-256 mismatch after write!")
        sys.exit(1)

    print(f"✓ {name} RESTORED BIT-EXACT TO {target_dev} AND VERIFIED!")
    run_cmd(fd, f"rm -f /tmp/restore_{name}.bin")

def main():
    port = get_port()
    base_dir = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/scratch/efs_backup"

    partitions = [
        ("modemst1", "/dev/disk/by-partlabel/modemst1", os.path.join(base_dir, "modemst1.img"), "699df61bacf1bba85b0d5b5894b69e29eaedc75d09cb11f626747f77728807a7"),
        ("modemst2", "/dev/disk/by-partlabel/modemst2", os.path.join(base_dir, "modemst2.img"), "28762518e5750cf09b72f4673e6a6aba13bd7d7c3419c58dd57f81e19fc9b568"),
    ]

    print(f"Opening connection to device on {port}...")
    fd = open_serial(port)

    init_out = run_cmd(fd, "echo DEVICE_READY", inactivity_timeout=0.5)
    if b"DEVICE_READY" not in init_out:
        print("Device not responding. Setting stty sane...")
        run_cmd(fd, "busybox stty sane")

    for name, target_dev, local_path, expected_sha in partitions:
        restore_partition(fd, name, target_dev, local_path, expected_sha)

    os.close(fd)
    print("\n" + "="*70)
    print("      modemst1 & modemst2 SUCCESSFULLY RESTORED AND VERIFIED")
    print("="*70)

if __name__ == "__main__":
    main()
