#!/usr/bin/env python3
"""
push_reboot_and_flash.py — Upload reboot_bootloader, command device into fastboot,
flash updated boot.img to boot_b, reboot, and verify Hexagon modem bring-up.
"""
import os
import sys
import time
import base64
import termios
import glob
import subprocess

TARGET_SERIAL = "DRGID18100509899"
FORBIDDEN_SERIAL = "ZF6226MKD8"
BOOT_IMG = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/build/out/boot.img"
BINARY_PATH = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/tools/reboot_bootloader/reboot_bootloader_tiny"

def get_serial_port():
    cu_ports = glob.glob("/dev/cu.usbmodem*")
    if cu_ports:
        return sorted(cu_ports)[0]
    tty_ports = glob.glob("/dev/tty.usbmodem*")
    return sorted(tty_ports)[0] if tty_ports else None

def reboot_via_serial():
    port = get_serial_port()
    if not port:
        print("[-] No serial port found.")
        return False

    print(f"[+] Connecting to serial port {port}...")
    if not os.path.exists(BINARY_PATH):
        print(f"[-] Error: {BINARY_PATH} not found")
        return False

    with open(BINARY_PATH, "rb") as f:
        data = f.read()

    raw_b64 = base64.b64encode(data).decode('utf-8')
    lines = [raw_b64[i:i+64] for i in range(0, len(raw_b64), 64)]
    b64_data = "\n".join(lines) + "\n"

    try:
        fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    except Exception as e:
        print(f"[-] Error opening port: {e}")
        return False

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
    try: os.read(fd, 65536)
    except BlockingIOError: pass
    os.set_blocking(fd, True)

    os.write(fd, b"\n\n\n")
    time.sleep(0.2)

    print("[+] Writing reboot helper to /tmp/reboot_bootloader.b64...")
    os.write(fd, b"cat << 'EOF' > /tmp/reboot_bootloader.b64\n")
    time.sleep(0.3)

    lines_per_chunk = 32
    for idx in range(0, len(lines), lines_per_chunk):
        chunk_lines = lines[idx : idx + lines_per_chunk]
        chunk_text = "\n".join(chunk_lines) + "\n"
        os.write(fd, chunk_text.encode('utf-8'))
        time.sleep(0.02)

    os.write(fd, b"EOF\n")
    time.sleep(0.3)

    print("[+] Decoding base64, persisting binary, and setting misc boot-bootloader...")
    os.write(fd, b"busybox base64 -d /tmp/reboot_bootloader.b64 > /tmp/reboot_bootloader\n")
    time.sleep(0.2)
    os.write(fd, b"chmod +x /tmp/reboot_bootloader\n")
    os.write(fd, b"cp -f /tmp/reboot_bootloader /mnt/persist/reboot_bootloader\n")
    os.write(fd, b"printf 'boot-bootloader\\0' | dd of=/dev/disk/by-partlabel/misc bs=32 count=1 conv=notrunc 2>/dev/null\n")
    time.sleep(0.5)

    print("[+] Executing /tmp/reboot_bootloader...")
    os.write(fd, b"/tmp/reboot_bootloader\n")
    time.sleep(1.0)
    os.close(fd)
    return True

print("=== ZethraOS Phase 5K — Flash & Bring-Up Automation ===")
# 1. Trigger reboot to bootloader
if not reboot_via_serial():
    print("[!] Could not reboot via serial; checking if already in fastboot...")

# 2. Wait for fastboot mode
print(f"[+] Waiting for {TARGET_SERIAL} in Fastboot Mode...", flush=True)
device_in_fastboot = False
for i in range(60): # 60 seconds
    try:
        res = subprocess.run(["fastboot", "devices"], capture_output=True, text=True, timeout=1.0)
        out = res.stdout.strip()
        if FORBIDDEN_SERIAL in out:
            print(f"[-] FATAL: Forbidden device {FORBIDDEN_SERIAL} detected! Aborting!", flush=True)
            sys.exit(1)
        if TARGET_SERIAL in out or (out and "fastboot" in out):
            print(f"[✓] Target device detected in Fastboot Mode:\n{out}", flush=True)
            device_in_fastboot = True
            break
    except Exception:
        pass
    time.sleep(1)

if not device_in_fastboot:
    print(f"[-] Device did not enter fastboot mode within 60s. Please hold Volume Down manually.", flush=True)
    sys.exit(1)

# 3. Clear misc boot-bootloader flag in fastboot (so next reboot is normal)
print("[+] Clearing misc boot-bootloader command...")
subprocess.run(["fastboot", "-s", TARGET_SERIAL, "erase", "misc"], capture_output=True, text=True)

# 4. Check slot
slot_proc = subprocess.run(["fastboot", "-s", TARGET_SERIAL, "getvar", "current-slot"], capture_output=True, text=True)
print(f"[+] Current slot: {(slot_proc.stderr + slot_proc.stdout).strip()}")

# 5. Flash boot_b
print(f"[+] Flashing {BOOT_IMG} (size: {os.path.getsize(BOOT_IMG)} bytes) to boot_b...")
flash_proc = subprocess.run(["fastboot", "-s", TARGET_SERIAL, "flash", "boot_b", BOOT_IMG], capture_output=True, text=True)
print(flash_proc.stdout.strip())
print(flash_proc.stderr.strip())

if flash_proc.returncode != 0:
    print("[-] Flash failed!")
    sys.exit(1)

print("[✓] boot_b successfully flashed! Rebooting device into Linux...")
reboot_proc = subprocess.run(["fastboot", "-s", TARGET_SERIAL, "reboot"], capture_output=True, text=True)
print(reboot_proc.stdout.strip())

# 6. Wait for device to boot into Linux serial console
print("[+] Waiting for Linux Serial Gadget (/dev/cu.usbmodem*)...", flush=True)
port = None
for _ in range(60):
    port = get_serial_port()
    if port:
        break
    time.sleep(1)

if not port:
    print("[-] Serial gadget did not appear within 60s. Check physical screen.", flush=True)
    sys.exit(0)

print(f"[✓] Linux Serial console active at {port}! Waiting 3s for services to settle...", flush=True)
time.sleep(3.0)

# Connect to serial console and verify
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

print("\n--- Remoteproc State ---")
print(exec_cmd("cat /sys/class/remoteproc/remoteproc0/state").strip())

print("\n--- QRTR Services ---")
print(exec_cmd("qrtr-lookup").strip())

print("\n--- RMTFS Log ---")
print(exec_cmd("cat /tmp/rmtfs.log").strip())

print("\n--- FIH SKUID Log ---")
print(exec_cmd("cat /tmp/fih_skuid.log").strip())

print("\n--- Dmesg Modem Logs ---")
print(exec_cmd("dmesg | grep -i -E 'remoteproc0|q6v5|fatal|crash|mba|diag|smem' | tail -n 80").strip())

os.close(fd)
print("\n[✓] Complete!")
