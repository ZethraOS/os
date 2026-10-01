#!/usr/bin/env python3
"""
auto_fix_and_recover.py — Automatically link clean OEM FSG and trigger modem recovery
as soon as the serial console appears.
"""
import os
import sys
import time
import glob
import termios

print("[+] auto_fix_and_recover.py started. Watching for /dev/cu.usbmodem*...")
sys.stdout.flush()

port = None
for _ in range(240): # poll for up to 2 minutes
    ports = glob.glob("/dev/cu.usbmodem*")
    if ports:
        port = sorted(ports)[0]
        break
    time.sleep(0.5)

if not port:
    print("[-] Timeout: No serial port found.")
    sys.exit(1)

print(f"[✓] Detected serial port: {port}")
sys.stdout.flush()

# Give TTY driver a moment to settle
time.sleep(0.5)

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

# 1. Ping
print("[*] Testing shell...")
out = exec_cmd("echo PING")
print(out.strip())

# 2. Fix OEM FSG symlinks
print("[*] Linking clean OEM FSG...")
out = exec_cmd("ln -sf /tmp/fsg_clean.img /dev/disk/by-partlabel/modem_fsg_oem_1; ln -sf /tmp/fsg_clean.img /dev/disk/by-partlabel/modem_fsg_oem_2; ls -la /dev/disk/by-partlabel/modem_fsg_oem*")
print(out.strip())

# 3. Ensure multi-instance tqftpserv
print("[*] Restarting 4-instance tqftpserv...")
exec_cmd("cp -f /mnt/persist/tqftpserv /usr/bin/tqftpserv; killall tqftpserv 2>/dev/null; /usr/bin/tqftpserv -d -v -t /mnt/modem /lib/firmware /var/lib/tqftpserv/fih_rfs /var/lib/tqftpserv/shared /var/lib/tqftpserv/hlos > /tmp/tqftpserv.log 2>&1 &")
time.sleep(1)

# 4. Restart rmtfs
print("[*] Restarting rmtfs...")
exec_cmd("killall rmtfs 2>/dev/null; /usr/bin/rmtfs -v -s -P -o /dev/disk/by-partlabel > /tmp/rmtfs.log 2>&1 &")
time.sleep(1)

out = exec_cmd("qrtr-lookup")
print(f"[+] Active services:\n{out.strip()}")

# 5. Trigger remoteproc recovery
print("[*] Triggering remoteproc recovery...")
exec_cmd("> /tmp/rmtfs.log; > /tmp/tqftpserv.log; dmesg -c > /dev/null")
exec_cmd("echo recover > /sys/kernel/debug/remoteproc/remoteproc0/recovery")

time.sleep(3)

state = exec_cmd("cat /sys/class/remoteproc/remoteproc0/state")
print(f"\n[+] Remoteproc state: {state.strip()}")

rmtfs_log = exec_cmd("cat /tmp/rmtfs.log")
print(f"\n[+] RMTFS log:\n{rmtfs_log.strip()}")

services = exec_cmd("qrtr-lookup")
print(f"\n[+] QRTR services post-recovery:\n{services.strip()}")

dmesg_tail = exec_cmd("dmesg | grep -i -E 'remoteproc0|q6v5|fatal|crash' | tail -n 25")
print(f"\n[+] Dmesg:\n{dmesg_tail.strip()}")

os.close(fd)
print("\n[+] Done!")
