import time
import subprocess
import sys

def poll_fastboot():
    try:
        res = subprocess.run(["fastboot", "devices"], capture_output=True, text=True)
        return "DRGID18100509899" in res.stdout
    except Exception:
        return False

def main():
    print("Rebooting device...")
    subprocess.run(["fastboot", "reboot"])
    
    start_time = time.time()
    print("Device rebooted. Waiting for it to return to fastboot (polling)...")
    
    # Wait first 2 seconds before polling
    time.sleep(2)
    
    found = False
    timeout = 120 # 120 seconds max timeout
    while time.time() - start_time < timeout:
        if poll_fastboot():
            found = True
            break
        time.sleep(0.5)
        
    elapsed = time.time() - start_time
    if found:
        print(f"=== TEST RESULT ===")
        print(f"Device returned to fastboot in {elapsed:.2f} seconds.")
        if elapsed > 20.0:
            print("Status: STALL DETECTED (Checkpoint reached!).")
        else:
            print("Status: NO STALL (Rebooted instantly, checkpoint not reached).")
        print(f"===================")
    else:
        print("Timeout: Device did not return to fastboot within 60 seconds.")

if __name__ == "__main__":
    main()
