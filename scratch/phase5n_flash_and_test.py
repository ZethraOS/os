#!/usr/bin/env python3
"""
phase5n_flash_and_test.py — Phase 5N Automated Flashing and Telemetry Capture
Part of ZethraOS / Nokia 6.1 Plus Bring-up.
"""

import os
import sys
import time
import subprocess
import glob

SERIAL = "DRGID18100509899"
REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
BOOT_IMG = os.path.join(REPO_ROOT, "build/out/boot.img")

def log(msg):
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)

def run_fb(cmd_args):
    full_cmd = ["fastboot", "-s", SERIAL] + cmd_args
    log(f"Running: {' '.join(full_cmd)}")
    res = subprocess.run(full_cmd, capture_output=True, text=True)
    out = res.stdout.strip()
    err = res.stderr.strip()
    if out: log(f"  stdout: {out}")
    if err: log(f"  stderr: {err}")
    if res.returncode != 0:
        raise RuntimeError(f"Command failed with code {res.returncode}: {err or out}")
    return err or out

def main():
    log("=== Phase 5N Flashing & Telemetry Capture ===")
    
    # 1. Preflight check: battery voltage
    bv_out = run_fb(["getvar", "battery-voltage"])
    mv = None
    for line in bv_out.splitlines():
        if "battery-voltage:" in line:
            mv = int(line.split(":")[-1].strip())
            break
    
    if mv is None or mv < 3500:
        raise RuntimeError(f"Safety check failed: battery voltage is {mv} mV (must be >= 3500 mV)")
    
    log(f"✓ Battery safety verified: {mv} mV >= 3500 mV")
    
    # 2. Flash boot_b
    log(f"Flashing {BOOT_IMG} to boot_b...")
    run_fb(["flash", "boot_b", BOOT_IMG])
    
    # 3. Set active slot b
    log("Setting active slot to b...")
    run_fb(["set_active", "b"])
    
    # 4. Reboot device
    log("Rebooting device...")
    run_fb(["reboot"])
    
    # 5. Wait for serial port to enumerate
    log("Waiting for USB serial device (/dev/cu.usbmodem*) to enumerate...")
    serial_port = None
    start = time.time()
    while time.time() - start < 90.0:
        ports = glob.glob("/dev/cu.usbmodem*")
        if ports:
            serial_port = sorted(ports)[0]
            log(f"✓ USB serial enumerated: {serial_port} (after {time.time() - start:.1f}s)")
            break
        time.sleep(1.0)
    
    if not serial_port:
        raise TimeoutError("USB serial device did not enumerate within 90s")
    
    # Settle window (allow initramfs and modem to reach steady state)
    log("Allowing 20s for initramfs daemons and remoteproc to initialize...")
    time.sleep(20.0)
    
    # 6. Capture telemetry using run_command_on_device.py
    sys.path.insert(0, os.path.dirname(__file__))
    import run_command_on_device as rcod
    
    def exec_and_save(cmd, outfile=None):
        log(f"Executing on device: {cmd}")
        output = rcod.run_cmd(cmd)
        if output is None:
            output = "[ERROR: No output or failed]"
        if outfile:
            outpath = os.path.join(REPO_ROOT, outfile)
            with open(outpath, "w") as f:
                f.write(output)
            log(f"  Saved to {outfile} ({len(output)} bytes)")
        return output

    remoteproc_state = exec_and_save("cat /sys/class/remoteproc/remoteproc0/state", "scratch/phase5n_remoteproc_state.txt")
    log(f"=== REMOTEPROC STATE: {remoteproc_state.strip()} ===")

    rmtfs_log = exec_and_save("cat /tmp/rmtfs.log", "scratch/phase5n_rmtfs.log")
    log(f"=== RMTFS LOG: ===\n{rmtfs_log}")

    fsc_hexdump = exec_and_save("hexdump -C /dev/disk/by-partlabel/fsc | head -n 35", "scratch/phase5n_fsc_hexdump.txt")
    log(f"=== FSC HEXDUMP: ===\n{fsc_hexdump}")

    qrtr_services = exec_and_save("qrtr-lookup", "scratch/phase5n_qrtr.txt")
    log(f"=== QRTR SERVICES: ===\n{qrtr_services}")

    dmesg_out = exec_and_save("dmesg", "scratch/phase5n_dmesg.txt")
    log(f"=== DMESG CAPTURED: {len(dmesg_out)} bytes ===")

    log("✓ Phase 5N flashing and telemetry capture complete!")

if __name__ == "__main__":
    main()
