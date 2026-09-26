#!/usr/bin/env python3
"""
reboot_and_capture_modem.py — Reboot device and capture fresh modem telemetry.
"""
import os
import sys
import time
import termios
import glob

def get_serial_port():
    cu_ports = glob.glob("/dev/cu.usbmodem*")
    if cu_ports:
        return sorted(cu_ports)[0]
    tty_ports = glob.glob("/dev/tty.usbmodem*")
    return sorted(tty_ports)[0] if tty_ports else None

port = get_serial_port()
if not port:
    print("[-] No serial port found.")
    sys.exit(1)

print(f"[+] Connecting to {port} to trigger reboot...")
try:
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    os.set_blocking(fd, True)
    os.write(fd, b"\n\nsync; echo 1 > /proc/sys/kernel/sysrq; echo b > /proc/sysrq-trigger\n")
    time.sleep(0.5)
    os.close(fd)
    print("[+] Sent hardware reset command via sysrq-trigger.")
except Exception as e:
    print(f"[-] Error: {e}")

print("[+] Waiting for device to reboot and USB serial to re-enumerate (up to 45s)...")
new_port = None
for i in range(45):
    time.sleep(1)
    new_port = get_serial_port()
    if new_port:
        print(f"[✓] Detected serial port at {new_port} after {i+1}s!")
        break

if not new_port:
    print("[-] Serial port did not appear within 45s.")
    sys.exit(1)

print("[+] Waiting 3 seconds for init scripts and modem to settle...")
time.sleep(3.0)

fd = os.open(new_port, os.O_RDWR | os.O_NOCTTY)
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

def exec_cmd(cmd, timeout=3.5):
    os.set_blocking(fd, False)
    try: os.read(fd, 65536)
    except: pass
    os.set_blocking(fd, True)
    os.write(fd, b"\n\x03\n")
    time.sleep(0.1)
    os.write(fd, cmd.encode() + b"\n")
    resp = b""
    start = time.time()
    while time.time() - start < timeout:
        time.sleep(0.05)
        os.set_blocking(fd, False)
        try:
            chunk = os.read(fd, 4096)
            if chunk:
                resp += chunk
                if b"/ # " in resp[len(cmd):]:
                    break
        except BlockingIOError:
            pass
    return resp.decode(errors='replace')

print("\n=== Remoteproc State ===")
print(exec_cmd("cat /sys/class/remoteproc/remoteproc0/state").strip())

print("\n=== QRTR Lookup (Active Services) ===")
print(exec_cmd("qrtr-lookup").strip())

print("\n=== RMTFS Log ===")
print(exec_cmd("cat /tmp/rmtfs.log").strip())

print("\n=== FIH SKUID Emulator Log ===")
print(exec_cmd("cat /tmp/fih_skuid.log").strip())

print("\n=== Shared Memory Superblock (0x85e00000) ===")
print(exec_cmd("od -A x -t x1 -N 64 /dev/qcom_rmtfs_mem1").strip())

print("\n=== Dmesg Modem Telemetry ===")
print(exec_cmd("dmesg | grep -i -E 'remoteproc0|q6v5|fatal|crash|mba|handover|ready|err' | tail -n 35").strip())

os.close(fd)
print("\n[✓] Capture complete!")
