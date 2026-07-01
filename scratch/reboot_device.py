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

    # Send reset sequence
    print("Sending reset sequence...")
    os.write(fd, b"\n\x03\n\x04\n\n\n")
    time.sleep(0.5)

    # Send reboot bootloader
    print("Sending reboot bootloader...")
    os.write(fd, b"reboot bootloader\n")
    time.sleep(1.0)
    
    os.close(fd)
    print("Done!")

if __name__ == "__main__":
    main()
