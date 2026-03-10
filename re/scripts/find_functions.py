#!/usr/bin/env python3
"""Identify function boundaries around sample rate xrefs by looking for function prologues."""

import struct

BINARY = "/nix/store/brcaw41j62pjki70ahf7ffd2f3bhaa3d-saleae-logic-2-extracted/resources/linux-x64/libgraph_server_shared.so"

# Key xref addresses (even addresses, filtering out the off-by-one REX prefix hits)
xref_addrs = {
    # GetDeviceSettings
    0x01c3e490: "LogicGraduateDevice::GetDeviceSettings xref",
    0x01c41d22: "LogicProfessionalDevice::GetDeviceSettings xref",

    # Get*PerformanceOptions
    0x01c4a07a: "GetDigitalOnlyPerforamnceOptions xref",
    0x01c49b94: "GetAnalogBasicPerformanceOptions xref 1",
    0x01c4a1ce: "GetAnalogBasicPerformanceOptions xref 2",
    0x01c4ae5c: "GetAnalogBasicPerformanceOptions xref 3",
    0x01c4aa70: "GetAnalogAdvancedPerformanceOptions xref 1",
    0x01c4ab1c: "GetAnalogAdvancedPerformanceOptions xref 2",

    # Set*PerformanceOption
    0x01c4b042: "SetDigitalOnlyPerformanceOption xref",
    0x01c4ada6: "SetAnalogBasicPerformanceOption xref",
    0x01c4b2bc: "SetAnalogAdvancedPerformanceOption xref",

    # SampleRateOption accessors
    0x01c479d6: "attempt to read un-set SampleRateOption xref 1",
    0x01c47a26: "attempt to read un-set SampleRateOption xref 2",
    0x01c47a02: "SampleRateOption::DigitalSampleRateHz xref 1",
    0x01c47a52: "SampleRateOption::AnalogPhysicalSampleRateHz xref 1",

    # GetPerformanceInterfaceType
    0x01c49cd4: "GetPerformanceInterfaceType xref 1",

    # HandleSetSampleRate
    0x01be4fd6: "HandleSetSampleRate xref",

    # Builder function (from device-settings.md, VA 0x1c47bd0)
    0x01c47bd0: "SampleRateOption builder (known from device-settings.md)",
}

with open(BINARY, "rb") as f:
    data = f.read()

text_start = 0x1a016c0
text_offset = 0x1a006c0
text_size = 0x1409b30
text_data = data[text_offset:text_offset + text_size]

def va_to_file(va):
    return text_offset + (va - text_start)

# For each xref, search backwards for function prologue
# Common x86-64 prologues:
# push rbp; mov rbp, rsp  = 55 48 89 e5
# push rbp; push r15; push r14; ...  = 55 41 57 41 56 ...
# push rbp alone = 55
# Also look for: endbr64 (f3 0f 1e fa) followed by push

def find_func_start(va):
    """Search backwards from va for a likely function start."""
    file_pos = va_to_file(va)
    # Search backwards up to 0x2000 bytes
    search_start = max(text_offset, file_pos - 0x2000)

    # Look for common function prologues
    # Align candidates on 16-byte boundaries or after 0xCC (int3) / 0x90 (nop) padding
    best = None
    for pos in range(file_pos - 1, search_start, -1):
        b = data[pos]
        # Check if this looks like padding before a function
        if b in (0xcc, 0x90, 0x00):
            # Next byte could be function start
            candidate_pos = pos + 1
            candidate_va = text_start + (candidate_pos - text_offset)
            cb = data[candidate_pos]
            # Check for push rbp (0x55) or endbr64 (f3 0f 1e fa)
            if cb == 0x55:
                return candidate_va
            if (cb == 0xf3 and candidate_pos + 3 < len(data) and
                data[candidate_pos+1] == 0x0f and
                data[candidate_pos+2] == 0x1e and
                data[candidate_pos+3] == 0xfa):
                return candidate_va
            # Also check for push r12-r15 (41 54-57) or sub rsp (48 83 ec / 48 81 ec)
            if cb == 0x41 and candidate_pos + 1 < len(data) and data[candidate_pos+1] in (0x54, 0x55, 0x56, 0x57):
                return candidate_va
            if cb == 0x48 and candidate_pos + 1 < len(data) and data[candidate_pos+1] == 0x83:
                return candidate_va

    return None

print("Function starts for key xref addresses:")
print("=" * 80)

# Group xrefs by their containing function
func_starts = {}
for va, name in sorted(xref_addrs.items()):
    func_start = find_func_start(va)
    if func_start:
        if func_start not in func_starts:
            func_starts[func_start] = []
        func_starts[func_start].append((va, name))
    else:
        print(f"  0x{va:08x} {name} -> NO FUNC START FOUND")

for func_va in sorted(func_starts.keys()):
    xrefs = func_starts[func_va]
    ghidra_va = func_va + 0x100000
    print(f"\nFunction @ ELF 0x{func_va:08x} (Ghidra 0x{ghidra_va:08x}):")
    for va, name in xrefs:
        print(f"  xref 0x{va:08x}: {name}")
