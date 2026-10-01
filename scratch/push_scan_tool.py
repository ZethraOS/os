import os
import sys
import time
import base64
import termios
import glob

ports = glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/tty.usbmodem*")
port = sorted(ports)[0] if ports else "/dev/cu.usbmodem2101"
binary_path = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/scratch/scan_diag_v4"

def push_and_run():
    if not os.path.exists(binary_path):
        print(f"Error: {binary_path} not found")
        return

    with open(binary_path, "rb") as f:
        data = f.read()

    raw_b64 = base64.b64encode(data).decode('utf-8')
    lines = [raw_b64[i:i+64] for i in range(0, len(raw_b64), 64)]
    print(f"Binary size: {len(data)} bytes. Base64 lines: {len(lines)}")

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
    os.write(fd, b"\n\n\n")
    time.sleep(0.2)

    # Start writing base64 file
    print("Writing base64 encoded binary to /tmp/scan_diag_v4.b64...")
    os.write(fd, b"cat << 'EOF' > /tmp/scan_diag_v4.b64\n")
    for line in lines:
        os.write(fd, (line + "\n").encode())
        time.sleep(0.01)
    os.write(fd, b"EOF\n")
    time.sleep(0.2)

    print("Decoding binary and setting executable permissions...")
    os.write(fd, b"busybox base64 -d /tmp/scan_diag_v4.b64 > /tmp/scan_diag_v4 && chmod +x /tmp/scan_diag_v4\n")
    time.sleep(0.3)

    print("Executing /tmp/scan_diag_v4...")
    os.write(fd, b"/tmp/scan_diag_v4\n")

    # Read output
    response = b""
    no_data_count = 0
    while no_data_count < 40:  # 4 second timeout of inactivity
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

if __name__ == "__main__":
    push_and_run()
