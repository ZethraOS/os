#!/usr/bin/env python3
"""
Patch RF calibration NV items (2000-4000) into modemst1.img
Offset: 0x80000, Size: 0x80000 (512 KB)
"""
import sys
import hashlib

def patch_rf_calibration(input_img, rf_cal_bin, output_img):
    # Load modemst1 backup
    with open(input_img, 'rb') as f:
        data = bytearray(f.read())
    
    # Load donor RF calibration
    with open(rf_cal_bin, 'rb') as f:
        rf_cal = f.read()
    
    if len(rf_cal) > 0x80000:
        print(f"Warning: RF calibration file too large ({len(rf_cal)} bytes, expected <= 524288)")
        rf_cal = rf_cal[:0x80000]
    
    # Patch at offset 0x80000
    data[0x80000:0x80000+len(rf_cal)] = rf_cal
    
    # Calculate new hash
    new_hash = hashlib.sha256(data).hexdigest()
    
    # Write patched image
    with open(output_img, 'wb') as f:
        f.write(data)
    
    print(f"Patched {output_img} created")
    print(f"SHA-256: {new_hash}")
    print(f"RF calibration injected: {len(rf_cal)} bytes at offset 0x80000")

if __name__ == '__main__':
    if len(sys.argv) != 4:
        print("Usage: patch_rf_cal.py <input_modemst1.img> <rf_calibration.bin> <output_patched.img>")
        sys.exit(1)
    
    patch_rf_calibration(sys.argv[1], sys.argv[2], sys.argv[3])
