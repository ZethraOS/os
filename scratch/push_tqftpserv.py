import os
import sys
import time
import base64
import gzip
import termios
import glob
import hashlib

ports = glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/tty.usbmodem*")
port = sorted(ports)[0] if ports else "/dev/cu.usbmodem1101"
binary_path = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/tools/tqftpserv/tqftpserv"

def push_tqftpserv():
    if not os.path.exists(binary_path):
        print(f"Error: {binary_path} not found")
        return

    with open(binary_path, "rb") as f:
        data = f.read()

    host_hash = hashlib.sha256(data).hexdigest()
    print(f"Original binary size: {len(data)} bytes, sha256: {host_hash}")

    compressed = gzip.compress(data)
    print(f"Gzipped size: {len(compressed)} bytes")

    b64_str = base64.b64encode(compressed).decode('utf-8')
    chunk_size = 512
    lines = [b64_str[i:i+chunk_size] for i in range(0, len(b64_str), chunk_size)]
    print(f"Base64 chunks: {len(lines)}")

    try:
        fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    except Exception as e:
        print(f"Error opening port {port}: {e}")
        return

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

    # Flush input buffer
    os.set_blocking(fd, False)
    try:
        os.read(fd, 65536)
    except BlockingIOError:
        pass
    os.set_blocking(fd, True)

    # Reset shell prompt
    os.write(fd, b"\n\x03\n\x03\n")
    time.sleep(0.3)

    # Remove old staging file
    os.write(fd, b"rm -f /tmp/tq.gz.b64 /mnt/persist/tqftpserv\n")
    time.sleep(0.2)

    print("Uploading chunks...")
    # Write chunks
    os.write(fd, b"cat << 'EOF' > /tmp/tq.gz.b64\n")
    for i, line in enumerate(lines):
        os.write(fd, (line + "\n").encode())
        if (i + 1) % 50 == 0:
            time.sleep(0.05)
            print(f"  Sent {i+1}/{len(lines)} chunks...")
    os.write(fd, b"EOF\n")
    time.sleep(0.5)

    print("Decompressing on target...")
    cmd = "busybox base64 -d /tmp/tq.gz.b64 | busybox gunzip -c > /mnt/persist/tqftpserv && chmod +x /mnt/persist/tqftpserv && rm -f /tmp/tq.gz.b64 && sha256sum /mnt/persist/tqftpserv && killall -9 tqftpserv 2>/dev/null; sleep 1; cp -f /mnt/persist/tqftpserv /usr/bin/tqftpserv && /usr/bin/tqftpserv -d -v -t /mnt/modem /lib/firmware /var/lib/tqftpserv/fih_rfs /var/lib/tqftpserv/shared /var/lib/tqftpserv/hlos > /tmp/tqftpserv.log 2>&1 & sleep 1; qrtr-lookup\n"
    os.write(fd, cmd.encode())

    # Read output
    response = b""
    no_data_count = 0
    while no_data_count < 60:
        time.sleep(0.1)
        os.set_blocking(fd, False)
        try:
            chunk = os.read(fd, 4096)
            if chunk:
                response += chunk
                no_data_count = 0
            else:
                no_data_count += 1
        except BlockingIOError:
            no_data_count += 1

    os.close(fd)
    out = response.decode(errors='replace')
    print("=== DEVICE OUTPUT ===")
    print(out)
    print("=====================")
    if host_hash in out:
        print("✓ SUCCESS: SHA-256 matches bit-exact on target!")
    else:
        print("⚠ WARNING: Hash verification failed!")

if __name__ == "__main__":
    push_tqftpserv()
