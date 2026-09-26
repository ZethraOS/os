#!/usr/bin/env python3
"""
Patch modemst1.img to align SKUID with nvcust/rf_nv partitions
Part of ZethraOS / Nokia 6.1 Plus (DRGID18100509899) Bring-up
"""
import sys
import hashlib
import re

def patch_skuid_profile(input_img, correct_skuid, output_img):
    with open(input_img, 'rb') as f:
        data = bytearray(f.read())
    
    # Search for /skuid/profile path
    pos = data.find(b'/skuid/profile')
    if pos == -1:
        print("ERROR: /skuid/profile not found in modemst1")
        return False
    
    # SKUID value typically follows the path string
    skuid_start = pos + len(b'/skuid/profile') + 1  # Skip null byte
    skuid_end = data.find(b'\x00', skuid_start)
    
    if skuid_end == -1:
        print("ERROR: Could not find SKUID string boundary")
        return False
    
    current_skuid = bytes(data[skuid_start:skuid_end])
    print(f"Current SKUID at /skuid/profile: {current_skuid}")
    print(f"Patching to: {correct_skuid}")
    
    # Pad or truncate new SKUID to fit
    new_skuid = correct_skuid.encode() if isinstance(correct_skuid, str) else correct_skuid
    if len(new_skuid) < len(current_skuid):
        new_skuid = new_skuid + b'\x00' * (len(current_skuid) - len(new_skuid))
    elif len(new_skuid) > len(current_skuid):
        print(f"WARNING: New SKUID longer than existing, truncating")
        new_skuid = new_skuid[:len(current_skuid)]
    
    data[skuid_start:skuid_end] = new_skuid
    new_hash = hashlib.sha256(data).hexdigest()
    
    with open(output_img, 'wb') as f:
        f.write(data)
    
    print(f"Patched {output_img} created")
    print(f"SHA-256: {new_hash}")
    return True

if __name__ == '__main__':
    if len(sys.argv) != 4:
        print("Usage: patch_skuid.py <input_modemst1.img> <correct_skuid> <output_patched.img>")
        print("Example: patch_skuid.py modemst1.img 600WW modemst1_patched.img")
        sys.exit(1)
    
    patch_skuid_profile(sys.argv[1], sys.argv[2], sys.argv[3])
