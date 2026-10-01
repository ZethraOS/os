import os
import sys
import time
import termios

import glob

def get_port():
    cu_ports = glob.glob("/dev/cu.usbmodem*")
    if cu_ports:
        return sorted(cu_ports)[0]
    tty_ports = glob.glob("/dev/tty.usbmodem*")
    return sorted(tty_ports)[0] if tty_ports else None

def run_cmd(cmd):
    port = get_port()
    if not port:
        print("Error: No /dev/cu.usbmodem* or /dev/tty.usbmodem* port found.")
        return None
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
        # Non-blocking read has no pending input; nothing to flush.
        pass

    # Send command
    os.set_blocking(fd, True)
    os.write(fd, b"\n")
    time.sleep(0.15)
    os.set_blocking(fd, False)
    try: os.read(fd, 65536)
    except: pass
    os.set_blocking(fd, True)
    os.write(fd, cmd.encode() + b"\n")
    
    # Read output
    response = b""
    start_time = time.time()
    no_data_count = 0
    timeout = 15.0 if "dmesg" in cmd or "log" in cmd else 6.0
    while time.time() - start_time < timeout and no_data_count < 30:
        time.sleep(0.05)
        os.set_blocking(fd, False)
        try:
            chunk = os.read(fd, 8192)
            if chunk:
                response += chunk
                no_data_count = 0
                if b"/ # " in response[-10:]:
                    break
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
