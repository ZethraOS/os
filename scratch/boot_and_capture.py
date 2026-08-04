#!/usr/bin/env python3
import os
import sys
import time
import subprocess
import termios
import glob

REPO_ROOT = "/Users/nomad/workstation/work/code/OS/Mobile/zethraos"
BOOT_IMG = os.path.join(REPO_ROOT, "build/out/boot.img")
LOG_FILE = os.path.join(REPO_ROOT, "scratch/boot_capture.log")

print("=== Boot & Capture Tool ===")

# Step 1: Check if device is in fastboot
res = subprocess.run(["fastboot", "devices"], capture_output=True, text=True)
if "fastboot" not in res.stdout:
    print("Device not in fastboot. Attempting reboot to bootloader over serial...")
    subprocess.run(["python3", os.path.join(REPO_ROOT, "scratch/trigger_fastboot_reboot.py")])
    time.sleep(2)
    # Poll for fastboot
    for _ in range(15):
        res = subprocess.run(["fastboot", "devices"], capture_output=True, text=True)
        if "fastboot" in res.stdout:
            print("✓ Device entered fastboot mode.")
            break
        time.sleep(1)

# Step 2: Boot boot.img via fastboot
print(f"Booting {BOOT_IMG} via fastboot...")
boot_proc = subprocess.Popen(["fastboot", "boot", BOOT_IMG], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
out, _ = boot_proc.communicate()
print(f"Fastboot output:\n{out}")

# Step 3: Listen on ACM serial port continuously
print("Listening on ACM serial port (/dev/cu.usbmodem*)...")
with open(LOG_FILE, "w") as log:
    log.write(f"=== Boot capture started at {time.ctime()} ===\n")

start_time = time.time()
port = None

while time.time() - start_time < 60:
    ports = glob.glob("/dev/cu.usbmodem*")
    if ports:
        port = ports[0]
        print(f"✓ Found ACM serial port: {port} at {time.time() - start_time:.1f}s")
        break
    time.sleep(0.5)

if not port:
    print("ERR: Serial port did not appear within 60s!")
    sys.exit(1)

try:
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
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
except Exception as e:
    print(f"Error setting up serial port: {e}")
    sys.exit(1)

print("--- Capturing serial log (press Ctrl+C or wait 30s) ---")
log_fd = open(LOG_FILE, "a")

end_time = time.time() + 30
try:
    while time.time() < end_time:
        try:
            chunk = os.read(fd, 4096)
            if chunk:
                text = chunk.decode(errors="replace")
                sys.stdout.write(text)
                sys.stdout.flush()
                log_fd.write(text)
                log_fd.flush()
        except BlockingIOError:
            time.sleep(0.05)
except KeyboardInterrupt:
    print("\nCapture stopped by user.")

log_fd.close()
os.close(fd)
print(f"\n✓ Serial log saved to {LOG_FILE}")
