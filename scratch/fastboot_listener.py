#!/usr/bin/env python3
import subprocess
import time
import sys

print("[+] Fastboot Listener started. Waiting for DRGID18100509899...", flush=True)

for i in range(120): # 2 minutes
    try:
        res = subprocess.run(["fastboot", "devices"], capture_output=True, text=True, timeout=0.5)
        out = res.stdout.strip()
        if "DRGID18100509899" in out or "fastboot" in out:
            print(f"\n[✓] FASTBOOT DEVICE CONNECTED:\n{out}", flush=True)
            # Immediately query slot and variables
            vars_out = subprocess.run(["fastboot", "getvar", "current-slot"], capture_output=True, text=True)
            print(vars_out.stderr.strip() or vars_out.stdout.strip(), flush=True)
            sys.exit(0)
    except Exception:
        pass

    # Also check if it booted Linux instead
    try:
        probe = subprocess.run(["/Users/nomad/workstation/work/code/OS/Mobile/zethraos/scratch/mac_usb_probe"], capture_output=True, text=True, timeout=0.5)
        if "Gadget Serial" in probe.stdout:
            print(f"\n[✓] LINUX GADGET SERIAL CONNECTED", flush=True)
            sys.exit(0)
    except Exception:
        pass

    time.sleep(1)

print("\n[-] Timeout: No device detected after 120s.", flush=True)
sys.exit(1)
