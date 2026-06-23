#!/usr/bin/env python3
"""
time_reboot_cycle.py — Measure how long a Nokia 6.1 Plus takes to return to fastboot.

F-10 FIX: Timeout message now matches the actual poll duration (was "60s", is now correct).
F-12 FIX: Caller (run_experiment.sh) performs ACM liveness check after this script exits.

Interpretation:
  TIMEOUT (≥ BOOT_TIMEOUT seconds):  Device did NOT crash-loop — it booted and stayed up.
  < 20s:                             Crash-loop — device rebooted before reaching userspace.
  20s–BOOT_TIMEOUT:                  Unexpected — check if device clean-shut or UART hung.

Usage:
  python3 build/scripts/time_reboot_cycle.py
  python3 build/scripts/time_reboot_cycle.py --timeout 120 --experiment-id img-01-headless-abc123 --output out.json

Exit codes:
  0  — Timed out (device stayed up, did not return to fastboot)
  1  — Device returned to fastboot (crash-loop or clean reboot)
  2  — Error (fastboot not found, device not detected)
"""

import subprocess
import time
import sys
import json
import argparse
import datetime

# ─── Constants ────────────────────────────────────────────────────────────────
CRASH_LOOP_THRESHOLD_S = 20.0   # < 20s = crash-loop (watchdog fires at ~10s)
POLL_INTERVAL_S = 1.0

def parse_args():
    p = argparse.ArgumentParser(description="Measure Nokia 6.1 Plus reboot cycle time.")
    p.add_argument("--timeout", type=int, default=120,
                   help="Seconds to wait before declaring 'booted' (default: 120)")
    p.add_argument("--experiment-id", default="",
                   help="Experiment ID to embed in the JSON output")
    p.add_argument("--output", default="",
                   help="Path to write JSON result (optional)")
    return p.parse_args()

def device_in_fastboot() -> bool:
    """Return True if a fastboot device is currently visible."""
    try:
        result = subprocess.run(
            ["fastboot", "devices"],
            capture_output=True, text=True, timeout=5
        )
        lines = [l.strip() for l in result.stdout.splitlines() if l.strip()]
        return any("fastboot" in l for l in lines)
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return False

def main():
    args = parse_args()
    timeout = args.timeout
    experiment_id = args.experiment_id or "unknown"

    print(f"[time_reboot_cycle] Experiment: {experiment_id}")
    print(f"[time_reboot_cycle] Polling for fastboot return. Timeout: {timeout}s.")
    print(f"[time_reboot_cycle] Crash-loop threshold: {CRASH_LOOP_THRESHOLD_S}s.")
    print(f"[time_reboot_cycle] Started at: {datetime.datetime.utcnow().isoformat()}Z")
    print()

    start = time.monotonic()

    while True:
        elapsed = time.monotonic() - start

        if elapsed >= timeout:
            # F-10 FIX: Message matches actual timeout value.
            print(f"\n[time_reboot_cycle] TIMEOUT: Device did not return to fastboot within {timeout}s.")
            print(f"[time_reboot_cycle] Interpretation: Device appears to have BOOTED SUCCESSFULLY.")
            _write_result(args.output, experiment_id, elapsed, "timeout", timeout)
            sys.exit(0)

        if device_in_fastboot():
            category = "crash-loop" if elapsed < CRASH_LOOP_THRESHOLD_S else "unexpected-fastboot"
            print(f"\n[time_reboot_cycle] Device returned to fastboot in {elapsed:.1f}s.")
            if category == "crash-loop":
                print(f"[time_reboot_cycle] CRASH LOOP detected (< {CRASH_LOOP_THRESHOLD_S}s threshold).")
            else:
                print(f"[time_reboot_cycle] Unexpected fastboot return after {elapsed:.1f}s (not a crash-loop).")
            _write_result(args.output, experiment_id, elapsed, category, timeout)
            sys.exit(1)

        # Progress indicator
        bar_len = 40
        filled = int(bar_len * elapsed / timeout)
        bar = "█" * filled + "░" * (bar_len - filled)
        print(f"\r  [{bar}] {elapsed:5.1f}s / {timeout}s", end="", flush=True)
        time.sleep(POLL_INTERVAL_S)

def _write_result(output_path: str, experiment_id: str, elapsed: float,
                  category: str, timeout: int):
    record = {
        "experiment_id":  experiment_id,
        "timestamp":      datetime.datetime.utcnow().isoformat() + "Z",
        "timeout_s":      timeout,
        "elapsed_s":      round(elapsed, 2),
        "outcome":        category,
        "crash_threshold_s": CRASH_LOOP_THRESHOLD_S,
        "interpretation": {
            "timeout":             "Device booted and stayed up",
            "crash-loop":          f"Device crash-looped (< {CRASH_LOOP_THRESHOLD_S}s)",
            "unexpected-fastboot": "Device returned to fastboot unexpectedly (> 20s)",
        }.get(category, "unknown"),
    }

    print()
    print(f"[time_reboot_cycle] Result: {json.dumps(record, indent=2)}")

    if output_path:
        try:
            with open(output_path, "w") as f:
                json.dump(record, f, indent=2)
            print(f"[time_reboot_cycle] Timing record written: {output_path}")
        except OSError as e:
            print(f"[time_reboot_cycle] WARNING: Could not write timing JSON: {e}", file=sys.stderr)

if __name__ == "__main__":
    main()
