#!/usr/bin/env python3
"""
trigger_and_monitor_modem.py — Prepare daemons, launch FIH SKUID emulator,
trigger remoteproc0 recovery, and monitor Hexagon modem bring-up.
"""
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

port = get_port()
if not port:
    print("Error: No serial port found.")
    sys.exit(1)

print(f"[+] Connecting to {port}...")
fd = os.open(port, os.O_RDWR | os.O_NOCTTY)
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

def exec_cmd(cmd, timeout=4.0):
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

print("[1] Verifying OEM FSG symlinks...")
out = exec_cmd("ls -la /dev/disk/by-partlabel/modem_fsg_oem*")
print(out.strip())

print("\n[2] Restarting rmtfs and tqftpserv with clean logs...")
exec_cmd("killall rmtfs 2>/dev/null; killall tqftpserv 2>/dev/null")
time.sleep(0.5)
exec_cmd("rm -f /tmp/rmtfs.log /tmp/tqftpserv.log /tmp/fih_skuid.log")
exec_cmd("/usr/bin/rmtfs -v -s -P -o /dev/disk/by-partlabel > /tmp/rmtfs.log 2>&1 &")
exec_cmd("/usr/bin/tqftpserv -d -v -t /mnt/modem /lib/firmware /var/lib/tqftpserv/fih_rfs /var/lib/tqftpserv/shared /var/lib/tqftpserv/hlos > /tmp/tqftpserv.log 2>&1 &")
time.sleep(1.0)

print("\n[3] Launching fih_skuid_emulator in background...")
exec_cmd("/usr/bin/fih_skuid_emulator > /tmp/fih_skuid.log 2>&1 &")
time.sleep(0.5)

print("\n[4] Clearing dmesg and triggering remoteproc0 recovery...")
exec_cmd("dmesg -c > /dev/null")
exec_cmd("echo recover > /sys/kernel/debug/remoteproc/remoteproc0/recovery")

print("\n[5] Waiting 5 seconds for Hexagon boot sequence...")
time.sleep(5.0)

print("\n=== Remoteproc State ===")
state = exec_cmd("cat /sys/class/remoteproc/remoteproc0/state")
print(state.strip())

print("\n=== QRTR Lookup ===")
qrtr = exec_cmd("qrtr-lookup")
print(qrtr.strip())

print("\n=== RMTFS Log ===")
rmtfs_log = exec_cmd("cat /tmp/rmtfs.log")
print(rmtfs_log.strip())

print("\n=== TQFTPSERV Log ===")
tqftp_log = exec_cmd("cat /tmp/tqftpserv.log")
print(tqftp_log.strip())

print("\n=== FIH SKUID Log ===")
fih_log = exec_cmd("cat /tmp/fih_skuid.log")
print(fih_log.strip())

print("\n=== Dmesg (Remoteproc / Modem) ===")
dmesg = exec_cmd("dmesg | grep -i -E 'remoteproc0|q6v5|fatal|crash|mba|err|glink|qmi' | tail -n 40")
print(dmesg.strip())

os.close(fd)
print("\n[✓] Done!")
