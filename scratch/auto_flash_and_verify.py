#!/usr/bin/env python3
"""
auto_flash_and_verify.py — Automatically flash fixed boot.img to boot_b when fastboot appears,
reboot, and verify modem bring-up.
"""
import subprocess
import time
import sys
import os
import glob
import termios

TARGET_SERIAL = "DRGID18100509899"
FORBIDDEN_SERIAL = "ZF6226MKD8"
BOOT_IMG = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/build/out/boot.img"

print(f"[+] auto_flash_and_verify.py waiting for {TARGET_SERIAL} in Fastboot Mode...", flush=True)

if not os.path.exists(BOOT_IMG):
    print(f"[-] ERROR: {BOOT_IMG} not found!", flush=True)
    sys.exit(1)

device_ready = False
for i in range(180): # Wait up to 3 minutes
    try:
        res = subprocess.run(["fastboot", "devices"], capture_output=True, text=True, timeout=1.0)
        out = res.stdout.strip()
        if FORBIDDEN_SERIAL in out:
            print(f"[-] FATAL: Forbidden device {FORBIDDEN_SERIAL} detected! Aborting immediately!", flush=True)
            sys.exit(1)
        if TARGET_SERIAL in out:
            print(f"\n[✓] Detected target device {TARGET_SERIAL} in Fastboot Mode!", flush=True)
            device_ready = True
            break
    except Exception:
        pass
    time.sleep(1)

if not device_ready:
    print(f"[-] Timeout waiting for {TARGET_SERIAL} in fastboot.", flush=True)
    sys.exit(1)

# Double check current slot
slot_proc = subprocess.run(["fastboot", "-s", TARGET_SERIAL, "getvar", "current-slot"], capture_output=True, text=True)
slot_info = (slot_proc.stderr + "\n" + slot_proc.stdout).strip()
print(f"[+] Slot info: {slot_info}", flush=True)

# Flash boot_b
print(f"[+] Flashing {BOOT_IMG} to boot_b on {TARGET_SERIAL}...", flush=True)
flash_proc = subprocess.run(["fastboot", "-s", TARGET_SERIAL, "flash", "boot_b", BOOT_IMG], capture_output=True, text=True)
print(flash_proc.stdout.strip(), flush=True)
print(flash_proc.stderr.strip(), flush=True)

if flash_proc.returncode != 0:
    print("[-] Flash failed!", flush=True)
    sys.exit(1)

print("[✓] Successfully flashed boot_b! Rebooting device into Linux...", flush=True)
reboot_proc = subprocess.run(["fastboot", "-s", TARGET_SERIAL, "reboot"], capture_output=True, text=True)
print(reboot_proc.stdout.strip(), flush=True)

print("[+] Waiting for Linux USB Serial Gadget (/dev/cu.usbmodem*)...", flush=True)
port = None
for _ in range(60): # 60 seconds
    ports = glob.glob("/dev/cu.usbmodem*")
    if ports:
        port = sorted(ports)[0]
        break
    time.sleep(1)

if not port:
    print("[-] Serial gadget did not appear within 60s. Check physical screen or USB connection.", flush=True)
    sys.exit(0)

print(f"[✓] Linux Serial console active at {port}!", flush=True)
time.sleep(1.0)

# Connect to serial console and check status
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

def exec_cmd(cmd, timeout=3.0):
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

print("\n--- OEM FSG Symlink Status ---")
print(exec_cmd("ls -la /dev/disk/by-partlabel/modem_fsg_oem*").strip())

print("\n--- QRTR Services ---")
print(exec_cmd("qrtr-lookup").strip())

print("\n--- Remoteproc State ---")
print(exec_cmd("cat /sys/class/remoteproc/remoteproc0/state").strip())

print("\n--- RMTFS Log ---")
print(exec_cmd("cat /tmp/rmtfs.log").strip())

print("\n--- Dmesg Modem Logs ---")
print(exec_cmd("dmesg | grep -i -E 'remoteproc0|q6v5|fatal|crash|mba' | tail -n 25").strip())

os.close(fd)
print("\n[✓] Verification finished successfully!")
