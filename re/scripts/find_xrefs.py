#!/usr/bin/env python3
"""Find RIP-relative LEA xrefs to assertion strings in libgraph_server_shared.so"""

import struct

BINARY = "/nix/store/brcaw41j62pjki70ahf7ffd2f3bhaa3d-saleae-logic-2-extracted/resources/linux-x64/libgraph_server_shared.so"

# String VAs to find xrefs for (assertion function signatures)
targets = {
    0x418c1b: "GetDigitalOnlyPerforamnceOptions",
    0x3f5351: "GetAnalogBasicPerformanceOptions",
    0x40abac: "GetAnalogAdvancedPerformanceOptions",
    0x42e346: "SetDigitalOnlyPerformanceOption",
    0x3c4e97: "SetAnalogBasicPerformanceOption",
    0x3829e0: "SetAnalogAdvancedPerformanceOption",
    0x3be274: "SampleRateOption::AnalogPhysicalSampleRateHz",
    0x435094: "SampleRateOption::DigitalSampleRateHz",
    0x4277b6: "DeviceSettings::GetPerformanceInterfaceType",
    0x418be4: "DeviceSettings::GetInterfaceMaximumBitRate",
    0x369cf2: "attempt to read un-set SampleRateOption",
    0x370a86: "invalid sample rate option",
    0x3fbcc8: "SampleRate should never be 0",
    0x39dfab: "HandleSetSampleRate",
    0x3829c0: "analog sample rate is not valid",
    0x3e75d5: "digital sample rate is not valid",
    0x3e7387: "sample rate not found",
    0x3558a3: "set sample rate: {}, {}",
    0x40aada: "virtual DeviceSettings LogicGraduateDevice::GetDeviceSettings()",
    0x390b02: "virtual DeviceSettings LogicProfessionalDevice::GetDeviceSettings()",
}

with open(BINARY, "rb") as f:
    data = f.read()

text_start = 0x1a016c0   # .text VA
text_offset = 0x1a006c0  # .text file offset
text_size = 0x1409b30

text_data = data[text_offset:text_offset + text_size]

print(f"Searching {len(text_data)} bytes of .text section...")
print()

for target_va, name in sorted(targets.items()):
    found = []
    for i in range(len(text_data) - 7):
        b0 = text_data[i]
        b1 = text_data[i + 1]
        b2 = text_data[i + 2]

        # REX.W + LEA + ModRM (mod=00, r/m=101 for RIP-relative)
        if b0 in (0x48, 0x4c) and b1 == 0x8d and (b2 & 0xc7) == 0x05:
            disp = struct.unpack_from('<i', text_data, i + 3)[0]
            inst_va = text_start + i
            next_va = inst_va + 7
            resolved = next_va + disp
            if resolved == target_va:
                found.append(inst_va)
        # Without REX prefix
        elif b0 == 0x8d and (b1 & 0xc7) == 0x05:
            disp = struct.unpack_from('<i', text_data, i + 2)[0]
            inst_va = text_start + i
            next_va = inst_va + 6
            resolved = next_va + disp
            if resolved == target_va:
                found.append(inst_va)

    if found:
        for addr in found:
            print(f"  0x{addr:08x} -> {name}")
    else:
        print(f"  NO XREF -> {name}")

print()
print("Done.")
