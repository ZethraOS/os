#!/usr/bin/env python3
import sys
import re

stock_dts_path = "build/out/stock_drg_dvt.dts"
custom_dts_path = "linux-6.9/arch/arm64/boot/dts/qcom/sdm636-nokia-frt.dts"

with open(stock_dts_path, 'r') as f:
    stock_content = f.read()

with open(custom_dts_path, 'r') as f:
    custom_content = f.read()

# Extract reserved-memory from stock
stock_rm_match = re.search(r'\treserved-memory \{(.*?)\n\t\};', stock_content, re.DOTALL)
if not stock_rm_match:
    print("Could not find reserved-memory in stock DTS")
    sys.exit(1)

stock_rm = stock_rm_match.group(1)

# Clean up phandles
stock_rm = re.sub(r'\t\t\tlinux,phandle = <0x[0-9a-f]+>;\n', '', stock_rm)
stock_rm = re.sub(r'\t\t\tphandle = <0x[0-9a-f]+>;\n', '', stock_rm)

# Ensure ramoops is correctly configured (stock uses 0x200000 at 0xacb00000)
# Let's replace the stock ramoops node with our properly sized one
ramoops_node = """
		ramoops@acb00000 {
			compatible = "ramoops";
			reg = <0x00 0xacb00000 0x00 0x200000>;
			record-size = <0x40000>;
			console-size = <0x40000>;
			ftrace-size = <0x0>;
			pmsg-size = <0x40000>;
		};
"""
# Remove existing ramoops if it exists
stock_rm = re.sub(r'\t\tramoops@[0-9a-f]+ \{.*?^\t\t\};', '', stock_rm, flags=re.DOTALL | re.MULTILINE)
stock_rm += ramoops_node

new_rm_block = "\treserved-memory {" + stock_rm + "\t};\n"

# Replace in custom DTS
new_custom_content = re.sub(r'\treserved-memory \{.*?^\t\};', new_rm_block, custom_content, flags=re.DOTALL | re.MULTILINE)

# Also remove the simple-framebuffer from chosen
chosen_clean = """	chosen {
		stdout-path = "serial0:115200n8";
	};"""
new_custom_content = re.sub(r'\tchosen \{.*?^\t\};', chosen_clean, new_custom_content, flags=re.DOTALL | re.MULTILINE)

with open(custom_dts_path, 'w') as f:
    f.write(new_custom_content)

print("Successfully replaced reserved-memory and cleaned up chosen.")
