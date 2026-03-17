#!/usr/bin/env python3
"""
ZethraOS Kernel Config Security Checker
Used in CI to catch dangerous kernel configuration choices.
"""

import sys
import re
from dataclasses import dataclass
from typing import List, Optional

@dataclass
class Rule:
    option: str
    required_value: Optional[str]   # None = must be absent
    severity: str                    # error | warning
    message: str

RULES: List[Rule] = [
    # Must-have security options
    Rule("CONFIG_RANDOMIZE_BASE",           "y",  "error",   "KASLR must be enabled"),
    Rule("CONFIG_STRICT_KERNEL_RWX",        "y",  "error",   "Strict kernel RWX must be enabled"),
    Rule("CONFIG_STACKPROTECTOR_STRONG",    "y",  "error",   "Stack protector must be enabled"),
    Rule("CONFIG_SECURITY_SELINUX",         "y",  "error",   "SELinux must be enabled"),
    Rule("CONFIG_PAGE_TABLE_ISOLATION",     "y",  "error",   "PTI (Meltdown mitigation) must be on"),
    Rule("CONFIG_INIT_ON_ALLOC_DEFAULT_ON", "y",  "warning", "Memory init on alloc recommended"),
    Rule("CONFIG_INIT_ON_FREE_DEFAULT_ON",  "y",  "warning", "Memory init on free recommended"),
    Rule("CONFIG_FORTIFY_SOURCE",           "y",  "error",   "FORTIFY_SOURCE must be enabled"),

    # Must-NOT-have options
    Rule("CONFIG_KGDB",                     None, "error",   "KGDB must not be in production builds"),
    Rule("CONFIG_DEVMEM",                   None, "error",   "/dev/mem access must be disabled"),
    Rule("CONFIG_DEVKMEM",                  None, "error",   "/dev/kmem must be disabled"),
    Rule("CONFIG_ACPI_CUSTOM_METHOD",       None, "warning", "ACPI custom method is a rootkit vector"),
    Rule("CONFIG_HIBERNATION",              None, "warning", "Hibernation bypasses dm-crypt"),
    Rule("CONFIG_COMPAT_BRK",              None, "warning", "Compat BRK weakens ASLR"),
    Rule("CONFIG_X86_PTDUMP",             None, "error",   "Page table dumping must not be in prod"),
    Rule("CONFIG_DEBUG_KERNEL",            None, "warning", "Kernel debug options should not be in prod"),
]

def parse_config(path: str) -> dict:
    """Parse a Kconfig file into a dict of {option: value}."""
    config = {}
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line.startswith("#") or not line:
                continue
            m = re.match(r"^(CONFIG_\w+)=(.*)", line)
            if m:
                val = m.group(2).strip('"')
                # Strip inline comments (e.g. "y         # KASLR" → "y")
                val = val.split('#')[0].strip().strip('"')
                config[m.group(1)] = val
            # Handle "# CONFIG_X is not set"
            m2 = re.match(r"^# (CONFIG_\w+) is not set", line)
            if m2:
                config[m2.group(1)] = "__not_set__"
    return config

def audit_config(config_path):
    config = parse_config(config_path)
    errors = 0
    warnings = 0

    print(f"\nZethraOS kernel config security audit: {config_path}\n{'─'*60}")

    for rule in RULES:
        value = config.get(rule.option)

        if rule.required_value is not None:
            # Must be set to required_value
            if value != rule.required_value:
                sym = "✗" if rule.severity == "error" else "⚠"
                print(f"  {sym} [{rule.severity.upper()}] {rule.option} = {value or 'unset'}")
                print(f"         {rule.message}")
                if rule.severity == "error": errors += 1
                else: warnings += 1
        else:
            # Must be absent or not set
            if value and value != "__not_set__":
                sym = "✗" if rule.severity == "error" else "⚠"
                print(f"  {sym} [{rule.severity.upper()}] {rule.option} is set (should be absent)")
                print(f"         {rule.message}")
                if rule.severity == "error": errors += 1
                else: warnings += 1

    print(f"\n{'─'*60}")
    print(f"Results: {errors} errors, {warnings} warnings")

    if errors == 0 and warnings == 0:
        print("✓ All security checks passed\n")
    elif errors == 0:
        print("⚠ Passed with warnings — review before release\n")
    else:
        print("✗ FAILED — fix errors before building\n")

    return 1 if errors > 0 else 0

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: check_kernel_config.py <path/to/defconfig>")
        sys.exit(1)
    sys.exit(audit_config(sys.argv[1]))
