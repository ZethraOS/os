#!/usr/bin/env python3
"""
push_fsc_gen2.py — Upload FSC Gen 2 (modemst2 match) and test modem bringup.
Part of ZethraOS / Nokia 6.1 Plus Bring-up.
"""
import os
import sys
import time
import glob
import base64
import hashlib
import termios

FSC_IMG = "/Users/nomad/.gemini/antigravity/brain/f8ba667f-b0fd-4be5-9bdb-eb546c4f2763/scratch/fsc_real_gen2.img"
EXPECTED_HASH = "ed5e79500befeb884ee458c0c8e8e8480af6b9dda11fb48c0ba3ad69baa85406"

def get_port():
    ports = glob.glob("/dev/cu.usbmodem*")
    return sorted(ports)[0] if ports else None

def open_serial(port):
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
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
    return fd

def exec_cmd(fd, cmd, timeout=5.0):
    os.set_blocking(fd, False)
    try:
        os.read(fd, 65536)
    except Exception:
        pass
    os.set_blocking(fd, True)
    os.write(fd, b"\n\x03\n")
    time.sleep(0.1)
    os.write(fd, cmd.encode() + b"\n")
    response = b""
    start = time.time()
    while time.time() - start < timeout:
        time.sleep(0.05)
        os.set_blocking(fd, False)
        try:
            chunk = os.read(fd, 4096)
            if chunk:
                response += chunk
                if b"/ # " in response[len(cmd):]:
                    break
        except BlockingIOError:
            pass
    return response.decode(errors="replace")

def main():
    if not os.path.exists(FSC_IMG):
        print(f"Error: {FSC_IMG} not found")
        sys.exit(1)

    with open(FSC_IMG, "rb") as f:
        data = f.read()

    actual_hash = hashlib.sha256(data).hexdigest()
    if actual_hash != EXPECTED_HASH:
        print(f"Error: Hash mismatch {actual_hash} != {EXPECTED_HASH}")
        sys.exit(1)
    print(f"[+] FSC Gen 2 image verified: {len(data)} bytes, sha256={actual_hash}")

    port = get_port()
    if not port:
        print("[-] Target not connected on /dev/cu.usbmodem*")
        sys.exit(2)

    print(f"[+] Found serial port: {port}")
    fd = open_serial(port)

    # Test shell responsiveness
    out = exec_cmd(fd, "echo PING")
    if "PING" not in out:
        print("[-] Target shell not responding to PING")
        sys.exit(3)
    print("[+] Target shell responsive")

    # Upload FSC in chunks of 512 bytes
    print("[*] Uploading FSC Gen 2 to /tmp/fsc.img...")
    exec_cmd(fd, "rm -f /tmp/fsc.img")
    chunk_size = 512
    for i in range(0, len(data), chunk_size):
        chunk = data[i:i+chunk_size]
        b64 = base64.b64encode(chunk).decode("ascii")
        exec_cmd(fd, f"echo -n '{b64}' | base64 -d >> /tmp/fsc.img")

    # Verify upload hash on target
    out = exec_cmd(fd, "sha256sum /tmp/fsc.img")
    print(f"[+] Target /tmp/fsc.img: {out.strip()}")
    if EXPECTED_HASH not in out:
        print("[-] Uploaded hash verification failed!")
        sys.exit(4)

    # Flash to partition
    print("[*] Writing /tmp/fsc.img to /dev/disk/by-partlabel/fsc...")
    exec_cmd(fd, "dd if=/tmp/fsc.img of=/dev/disk/by-partlabel/fsc bs=1024 count=1 conv=fsync && sync")
    out = exec_cmd(fd, "sha256sum /dev/disk/by-partlabel/fsc")
    print(f"[+] Target partition hash: {out.strip()}")
    if EXPECTED_HASH not in out:
        print("[-] Partition hash verification failed!")
        sys.exit(5)

    # Ensure OEM symlinks and multi-instance tqftpserv
    print("[*] Setting clean OEM FSG symlinks...")
    exec_cmd(fd, "ln -sf /tmp/fsg_clean.img /dev/disk/by-partlabel/modem_fsg_oem_1; ln -sf /tmp/fsg_clean.img /dev/disk/by-partlabel/modem_fsg_oem_2")

    print("[*] Ensuring 4-instance tqftpserv running...")
    exec_cmd(fd, "cp -f /mnt/persist/tqftpserv /usr/bin/tqftpserv; killall tqftpserv 2>/dev/null; /usr/bin/tqftpserv -d -v -t /mnt/modem /lib/firmware /var/lib/tqftpserv/fih_rfs /var/lib/tqftpserv/shared /var/lib/tqftpserv/hlos > /tmp/tqftpserv.log 2>&1 &")

    print("[*] Restarting rmtfs...")
    exec_cmd(fd, "killall rmtfs 2>/dev/null; /usr/bin/rmtfs -v -s -P -o /dev/disk/by-partlabel > /tmp/rmtfs.log 2>&1 &")

    time.sleep(1)
    out = exec_cmd(fd, "qrtr-lookup")
    print(f"[+] Services:\n{out.strip()}")

    # Trigger recovery
    print("[*] Triggering remoteproc recovery...")
    exec_cmd(fd, "> /tmp/rmtfs.log; > /tmp/tqftpserv.log; dmesg -c > /tmp/dmesg_pre.txt")
    exec_cmd(fd, "echo recover > /sys/kernel/debug/remoteproc/remoteproc0/recovery")
    time.sleep(3)

    out = exec_cmd(fd, "cat /sys/class/remoteproc/remoteproc0/state")
    print(f"[+] Remoteproc state: {out.strip()}")

    out = exec_cmd(fd, "cat /tmp/rmtfs.log")
    print(f"[+] RMTFS log:\n{out.strip()}")

    out = exec_cmd(fd, "dmesg | grep -i -E 'remoteproc0|q6v5|fatal|crash' | tail -n 25")
    print(f"[+] Dmesg:\n{out.strip()}")

    print("[+] Done!")

if __name__ == "__main__":
    main()
