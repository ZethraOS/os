#!/usr/bin/env python3
import glob
import subprocess
import time
import sys
import os

SERIAL_HELPER = "/Users/nomad/.gemini/antigravity/brain/f8ba667f-b0fd-4be5-9bdb-eb546c4f2763/scratch/serial_helper.py"

print("[Auto-Fastboot Watcher started. Monitoring for USB connection...]")
for i in range(60):
    # 1. Check if already in Fastboot
    res = subprocess.run(["fastboot", "devices"], capture_output=True, text=True)
    if "DRGID18100509899" in res.stdout or (res.stdout.strip() and "fastboot" in res.stdout):
        print(f"\n✓ DEVICE ALREADY IN FASTBOOT MODE:\n{res.stdout.strip()}")
        sys.exit(0)

    # 2. Check if booted into Linux (serial gadget)
    ports = glob.glob("/dev/cu.usbmodem*")
    if ports:
        port = sorted(ports)[0]
        print(f"\n[!] Device detected in Linux mode on {port}")
        print("[!] Executing direct reboot to bootloader/fastboot...")
        sub_res = subprocess.run([sys.executable, SERIAL_HELPER, "reboot_fastboot"], capture_output=True, text=True)
        print(sub_res.stdout)
        print("[!] Waiting for fastboot re-enumeration...")
        time.sleep(3)
        for _ in range(15):
            fb_check = subprocess.run(["fastboot", "devices"], capture_output=True, text=True)
            if fb_check.stdout.strip():
                print(f"\n✓ SUCCESS: DEVICE IS NOW IN FASTBOOT MODE:\n{fb_check.stdout.strip()}")
                sys.exit(0)
            time.sleep(1)

    time.sleep(1)

print("\n[-] Timeout: No device detected after 60 seconds.")
sys.exit(1)
