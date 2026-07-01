import os
import sys
import time
import termios

port = "/dev/tty.usbmodemZETHRA0000011"

def main():
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

    # Reset prompt
    os.write(fd, b"\n\x03\n\x04\n\n\n")
    time.sleep(0.5)
    os.read(fd, 4096)  # Flush

    # Try to mount pstore
    print("Mounting pstore...")
    os.write(fd, b"mount -t pstore pstore /sys/fs/pstore 2>/dev/null || true\n")
    time.sleep(0.2)
    os.read(fd, 4096)  # Clear buffer

    # List pstore contents
    print("Listing pstore files...")
    os.write(fd, b"ls -la /sys/fs/pstore/\n")
    time.sleep(0.5)
    
    os.set_blocking(fd, False)
    files_out = os.read(fd, 4096).decode(errors='ignore')
    print("=== PSTORE FILES ===")
    print(files_out)
    print("====================")

    # If console-ramoops or dmesg-ramoops exists, read them!
    os.set_blocking(fd, True)
    if "console-ramoops" in files_out:
        print("Reading console-ramoops...")
        os.write(fd, b"cat /sys/fs/pstore/console-ramoops\n")
        time.sleep(3.0)
        os.set_blocking(fd, False)
        print("=== CONSOLE RAMOOPS ===")
        print(os.read(fd, 65536).decode(errors='ignore'))
        print("=======================")
        os.set_blocking(fd, True)

    if "dmesg-ramoops-0" in files_out:
        print("Reading dmesg-ramoops-0...")
        os.write(fd, b"cat /sys/fs/pstore/dmesg-ramoops-0\n")
        time.sleep(3.0)
        os.set_blocking(fd, False)
        print("=== DMESG RAMOOPS 0 ===")
        print(os.read(fd, 65536).decode(errors='ignore'))
        print("=======================")
        os.set_blocking(fd, True)

    os.close(fd)

if __name__ == "__main__":
    main()
