import os
import sys
import time
import termios

port = "/dev/tty.usbmodemZETHRA0000011"

def run_cmd(cmd):
    try:
        fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    except Exception as e:
        print(f"Error opening port: {e}")
        return None

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

    # Send command
    os.set_blocking(fd, True)
    os.write(fd, b"\n\n\n")
    time.sleep(0.2)
    os.write(fd, cmd.encode() + b"\n")
    
    # Read output
    response = b""
    no_data_count = 0
    while no_data_count < 10:  # 1.0 second timeout of inactivity
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
    return response.decode(errors='replace')

if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "cat /proc/cmdline"
    print(f"Running command on device: {cmd}")
    res = run_cmd(cmd)
    if res:
        print("=== DEVICE OUTPUT ===")
        print(res)
        print("=====================")
