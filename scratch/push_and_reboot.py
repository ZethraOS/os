import os
import sys
import time
import termios

port = "/dev/tty.usbmodemZETHRA0000011"
binary_path = "build/out/reboot_bootloader"

def main():
    if not os.path.exists(binary_path):
        print(f"Error: {binary_path} does not exist")
        return

    with open(binary_path, "rb") as f:
        data = f.read()
    
    print(f"Loaded binary: {len(data)} bytes")

    try:
        fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    except Exception as e:
        print(f"Error opening port: {e}")
        return

    # Configure termios for raw serial transmission
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

    # Reset command prompt on device
    os.write(fd, b"\n\n\x03\n")
    time.sleep(0.2)

    # Clear previous file on device
    os.write(fd, b"rm -f /tmp/reboot_bootloader_dyn\n")
    time.sleep(0.1)

    chunk_size = 800
    print(f"Pushed: 0/{len(data)} bytes...")
    for i in range(0, len(data), chunk_size):
        chunk = data[i:i+chunk_size]
        hex_str = "".join(f"\\x{b:02x}" for b in chunk)
        cmd = f"printf '{hex_str}' >> /tmp/reboot_bootloader_dyn\n"
        os.write(fd, cmd.encode())
        # Brief pause to let busybox process the input line
        time.sleep(0.15)
        print(f"Pushed: {min(i+chunk_size, len(data))}/{len(data)} bytes...")

    # Mark executable and run
    print("Making executable and running...")
    os.write(fd, b"chmod +x /tmp/reboot_bootloader_dyn && /tmp/reboot_bootloader_dyn\n")
    time.sleep(0.5)
    
    os.close(fd)
    print("Done!")

if __name__ == "__main__":
    main()
