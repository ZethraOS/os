import os
import sys
import time
import termios

port = "/dev/tty.usbmodemZETHRA0000011"

def main():
    try:
        fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    except Exception as e:
        print(f"Error: {e}")
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

    # Clean old inputs
    os.write(fd, b"\n\n\n")
    time.sleep(0.5)
    os.set_blocking(fd, False)
    try:
        os.read(fd, 65536)
    except BlockingIOError:
        pass

    # Send grep command
    os.set_blocking(fd, True)
    os.write(fd, b"dmesg | grep -iE 'ramoops|pstore|console-ramoops'\n")
    
    # Wait for execution and output
    response = b""
    no_data_count = 0
    while no_data_count < 10:  # 1.0 second of inactivity timeout
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

    print("=== DEVICE DMESG TAIL ===")
    print(response.decode(errors='replace'))
    print("=========================")
    os.close(fd)

if __name__ == "__main__":
    main()
