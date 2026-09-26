#!/usr/bin/env python3
"""
Create minimal FIH RFS configuration files with valid headers
Part of ZethraOS / Nokia 6.1 Plus (DRGID18100509899) Bring-up
"""
import os
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.abspath(os.path.join(SCRIPT_DIR, "../.."))
STAGING_DIR = os.path.join(REPO_ROOT, 'scratch/tqftpserv_staging')

# NV item file format (Qualcomm NV binary format)
def create_nv_file(path, nv_items=None):
    """Create a minimal NV file with header and optional items"""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'wb') as f:
        # NV file header: magic 'NV' + version + item count
        f.write(b'NV')           # Magic
        f.write(b'\x01\x00')     # Version 1.0
        if nv_items:
            f.write(len(nv_items).to_bytes(2, 'little'))  # Item count
            for nv_id, data in nv_items.items():
                f.write(nv_id.to_bytes(2, 'little'))  # NV ID
                f.write(len(data).to_bytes(2, 'little'))  # Data length
                f.write(data)  # Data
        else:
            f.write(b'\x00\x00')  # Zero items
    print(f"Created NV file: {path}")

# CFG file format (simple key=value text)
def create_cfg_file(path, config_dict=None):
    """Create a minimal CFG file with optional key-value pairs"""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        f.write("# FIH RFS Configuration File\n")
        f.write("# Auto-generated for Phase 5K\n")
        if config_dict:
            for key, value in config_dict.items():
                f.write(f"{key}={value}\n")
    print(f"Created CFG file: {path}")

# DAT file format (binary blob with header)
def create_dat_file(path, data=None):
    """Create a minimal DAT file with header"""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'wb') as f:
        f.write(b'DATA')  # Magic
        f.write(b'\x01\x00\x00\x00')  # Version
        if data:
            f.write(len(data).to_bytes(4, 'little'))  # Data length
            f.write(data)
        else:
            f.write(b'\x00\x00\x00\x00')  # Zero length
    print(f"Created DAT file: {path}")

def main():
    print(f"Provisioning FIH RFS stubs into: {STAGING_DIR}")
    os.makedirs(STAGING_DIR, exist_ok=True)

    # 1. Standard FIH RFS / Vendor stubs
    create_cfg_file(f'{STAGING_DIR}/fih_rfs/data/vendor/nv_items.cfg', {
        'rf_calibrated': '1',
        'sim_initialized': '1',
        'vendor': 'FIH'
    })

    create_dat_file(f'{STAGING_DIR}/fih_rfs/data/vendor/rf_config.dat')

    create_nv_file(f'{STAGING_DIR}/fih_rfs/data/vendor/radio/calibration.nv', {
        2000: b'\x00' * 16,
        2001: b'\x00' * 16,
    })

    create_nv_file(f'{STAGING_DIR}/fih_rfs/data/vendor/sim/sim_config.nv', {
        1: b'\x00' * 32,
    })

    create_cfg_file(f'{STAGING_DIR}/shared/modem/modem.cfg', {
        'modem_type': 'SDM636',
        'rfs_enabled': '1',
        'init_done': '1'
    })

    # 2. Add TFTP server test file (/readwrite/server_check.txt)
    rw_dir = f'{STAGING_DIR}/readwrite'
    os.makedirs(rw_dir, exist_ok=True)
    with open(f'{rw_dir}/server_check.txt', 'w') as f:
        f.write("hello\n")
    print(f"Created TFTP test probe file: {rw_dir}/server_check.txt")

    # 3. Add FIH OEM specific lists discovered in coredump
    fih_oem_dir = f'{STAGING_DIR}/fih_rfs/data/vendor/FIHOEM'
    os.makedirs(fih_oem_dir, exist_ok=True)
    with open(f'{fih_oem_dir}/SKUID1', 'w') as f:
        f.write("600WW\n")
    with open(f'{fih_oem_dir}/MDM_SIM', 'w') as f:
        f.write("2\n")
    with open(f'{fih_oem_dir}/fih_cust_list.txt', 'w') as f:
        f.write("600WW\n")
    with open(f'{fih_oem_dir}/fih_ims_list.txt', 'w') as f:
        f.write("\n")
    print(f"Created FIHOEM metadata files in {fih_oem_dir}")

    # 4. Generate manifest
    manifest_path = f'{STAGING_DIR}/MANIFEST.txt'
    lines = []
    for root, _, files in os.walk(STAGING_DIR):
        for file in sorted(files):
            if file != 'MANIFEST.txt':
                rel = os.path.relpath(os.path.join(root, file), STAGING_DIR)
                lines.append(rel)
    lines.sort()
    with open(manifest_path, 'w') as f:
        f.write('\n'.join(lines) + '\n')
    print(f"Manifest written: {manifest_path} ({len(lines)} files)")

if __name__ == '__main__':
    main()
