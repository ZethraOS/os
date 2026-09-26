#!/usr/bin/env python3
import glob
import subprocess
import time
import sys
import os

print("[+] Device Watcher Started — polling for Fastboot, Serial, or USB attachment...")
sys.stdout.flush()

for i in range(120): # poll for up to 2 minutes
    # 1. Check Fastboot
    try:
        res = subprocess.run(["fastboot", "devices"], capture_output=True, text=True, timeout=1)
        if "DRGID18100509899" in res.stdout or (res.stdout.strip() and "fastboot" in res.stdout):
            print(f"\n[✓] DETECTED IN FASTBOOT MODE:\n{res.stdout.strip()}")
            sys.stdout.flush()
            sys.exit(0)
    except Exception:
        pass

    # 2. Check Linux Serial Gadget
    ports = glob.glob("/dev/cu.usbmodem*")
    if ports:
        port = sorted(ports)[0]
        print(f"\n[✓] DETECTED IN LINUX SERIAL MODE: {port}")
        sys.stdout.flush()
        sys.exit(0)

    # 3. Check USB Probe
    try:
        probe = subprocess.run(["/Users/nomad/workstation/work/code/OS/Mobile/zethraos/scratch/mac_usb_probe"], capture_output=True, text=True, timeout=1)
        if "IOUSBHostDevice instances: 1" in probe.stdout or "IOUSBHostDevice instances: 2" in probe.stdout:
            print(f"\n[✓] DETECTED USB DEVICE:\n{probe.stdout.strip()}")
            sys.stdout.flush()
            sys.exit(0)
    except Exception:
        pass

    time.sleep(1)

print("\n[-] Timeout: No device detected after 120 seconds.")
sys.exit(1)
