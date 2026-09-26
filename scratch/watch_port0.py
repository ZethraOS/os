#!/usr/bin/env python3
import time, glob, subprocess, sys

print("[+] Listening for phone boot on Port 0 / Serial / Fastboot...")
sys.stdout.flush()

for i in range(180): # 3 minutes
    # 1. Check serial
    ports = glob.glob("/dev/cu.usbmodem*")
    if ports:
        print(f"\n[✓] Serial port detected: {ports[0]}")
        sys.stdout.flush()
        # Immediately run the fix
        cmd = [sys.executable, "/Users/nomad/workstation/work/code/OS/Mobile/zethraos/scratch/auto_fix_and_recover.py"]
        subprocess.run(cmd)
        sys.exit(0)

    # 2. Check fastboot
    try:
        fb = subprocess.run(["fastboot", "devices"], capture_output=True, text=True, timeout=0.5)
        if fb.stdout.strip():
            print(f"\n[✓] Fastboot device detected: {fb.stdout.strip()}")
            sys.stdout.flush()
            sys.exit(0)
    except:
        pass

    time.sleep(1)

print("\n[-] Watcher timed out.")
sys.exit(1)
