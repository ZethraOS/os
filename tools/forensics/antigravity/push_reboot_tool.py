import os
import sys
import time
import base64
import termios

port = "/dev/tty.usbmodemZETHRA0000011"
binary_path = "build/out/reboot_bootloader"

def push_and_run():
    if not os.path.exists(binary_path):
        print(f"Error: {binary_path} not found")
        return

    with open(binary_path, "rb") as f:
        data = f.read()

    raw_b64 = base64.b64encode(data).decode('utf-8')
    # Insert newlines every 64 characters
    lines = [raw_b64[i:i+64] for i in range(0, len(raw_b64), 64)]
    b64_data = "\n".join(lines) + "\n"
    
    print(f"Binary size: {len(data)} bytes. Base64 size: {len(b64_data)} bytes. Lines: {len(lines)}")

    try:
        fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    except Exception as e:
        print(f"Error opening port: {e}")
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
    print("Preparing device for file transfer...")
    os.write(fd, b"cat << 'EOF' > /tmp/reboot_bootloader.b64\n")
    time.sleep(0.5)

    print("Sending base64 data line by line...")
    # Send in chunks of 32 lines (approx 2KB) to keep USB packets efficient
    lines_per_chunk = 32
    for idx in range(0, len(lines), lines_per_chunk):
        chunk_lines = lines[idx : idx + lines_per_chunk]
        chunk_text = "\n".join(chunk_lines) + "\n"
        os.write(fd, chunk_text.encode('utf-8'))
        time.sleep(0.02)  # pause to let shell write it
        if (idx // lines_per_chunk) % 10 == 0 or idx + lines_per_chunk >= len(lines):
            print(f"Sent lines {idx + len(chunk_lines)}/{len(lines)}")

    os.write(fd, b"EOF\n")
    time.sleep(0.5)

    # Verify size
    print("Verifying transferred size...")
    os.write(fd, b"ls -la /tmp/reboot_bootloader.b64\n")
    time.sleep(0.2)
    os.set_blocking(fd, False)
    try:
        print(os.read(fd, 4096).decode(errors='ignore'))
    except BlockingIOError:
        pass
    os.set_blocking(fd, True)

    print("Decoding base64 and running reboot...")
    os.write(fd, b"busybox base64 -d /tmp/reboot_bootloader.b64 > /tmp/reboot_bootloader\n")
    time.sleep(0.5)
    os.write(fd, b"chmod +x /tmp/reboot_bootloader\n")
    time.sleep(0.2)
    os.write(fd, b"/tmp/reboot_bootloader\n")
    time.sleep(1.0)

    print("Pushed and executed reboot_bootloader!")
    os.close(fd)

if __name__ == "__main__":
    push_and_run()
