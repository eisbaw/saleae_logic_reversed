#!/usr/bin/env python3
"""Extract global data referenced by sample rate functions."""

import struct

BINARY = "/nix/store/brcaw41j62pjki70ahf7ffd2f3bhaa3d-saleae-logic-2-extracted/resources/linux-x64/libgraph_server_shared.so"

with open(BINARY, "rb") as f:
    data = f.read()

# ELF sections (from readelf -S):
# .rodata: VA 0x34a580, offset 0x34a580, size 0x13e44a0
# .data.rel.ro: VA 0x2e2ea60, offset 0x2e2ca60, size 0xba1a8
# .bss would be after .data in memory but not in file

# For this binary, VA == file offset for sections before .text
# But .text: VA 0x1a016c0, offset 0x1a006c0 (diff = 0x1000)
# .data.rel.ro: VA 0x2e2ea60, offset 0x2e2ca60 (diff = 0x2000)

def va_to_offset(va):
    """Convert VA to file offset based on section."""
    if va < 0x1a016c0:  # before .text
        return va  # .rodata etc, VA == offset
    elif va < 0x2e2ea60:  # .text section
        return va - 0x1000
    else:  # .data.rel.ro and beyond
        return va - 0x2000

def read_u32(va):
    off = va_to_offset(va)
    return struct.unpack_from('<I', data, off)[0]

def read_u64(va):
    off = va_to_offset(va)
    return struct.unpack_from('<Q', data, off)[0]

def read_double(va):
    off = va_to_offset(va)
    return struct.unpack_from('<d', data, off)[0]

def read_bytes(va, n):
    off = va_to_offset(va)
    return data[off:off+n]

print("=" * 80)
print("GetPerformanceInterfaceType lookup table at DAT_005748cc")
print("=" * 80)
# uint32 array indexed by device type (0-8)
for i in range(9):
    val = read_u32(0x5748cc + i * 4)
    print(f"  [{i}] = {val}")

print()
print("=" * 80)
print("USB max bit rate per speed at DAT_005748f0")
print("=" * 80)
# uint64 array indexed by USB speed (0=?, 1=USB2, 2=USB3?)
for i in range(4):
    val = read_u64(0x5748f0 + i * 8)
    print(f"  [{i}] = {val} ({val:,})")

print()
print("=" * 80)
print("LogicGraduateDevice divisor constants")
print("=" * 80)
# DAT_0044e3b8 - used as: DAT_0044e3b8 / digital_rate
val = read_double(0x44e3b8)
print(f"  DAT_0044e3b8 (double) = {val} ({int(val):,})")
# DAT_0044e278 - used as: DAT_0044e278 / analog_rate
val = read_double(0x44e278)
print(f"  DAT_0044e278 (double) = {val} ({int(val):,})")

print()
print("=" * 80)
print("LogicProfessionalDevice divisor constants")
print("=" * 80)
val = read_double(0x44d9d0)
print(f"  DAT_0044d9d0 (double) = {val} ({int(val):,})")
val = read_double(0x44e2e8)
print(f"  DAT_0044e2e8 (double) = {val} ({int(val):,})")

print()
print("=" * 80)
print("Channel config constants")
print("=" * 80)
val = read_u32(0x44d4cc)
print(f"  DAT_0044d4cc = 0x{val:08x} = {val}")
val = read_u32(0x44d4dc)
print(f"  DAT_0044d4dc = 0x{val:08x} = {val}")
val = read_u32(0x44d4d4)
print(f"  DAT_0044d4d4 = 0x{val:08x} = {val}")

print()
print("=" * 80)
print("LogicStudentDevice builder constants")
print("=" * 80)
# DAT_00b71b00 - first arg to FUN_01d47bd0 (digital sample rate?)
val = read_u32(0xb71b00)
print(f"  DAT_00b71b00 = 0x{val:08x} = {val:,}")
# DAT_005b8d80 - second arg (analog sample rate?)
val = read_u32(0x5b8d80)
print(f"  DAT_005b8d80 = 0x{val:08x} = {val:,}")
# Also check as pointers/u64
val64 = read_u64(0xb71b00)
print(f"  DAT_00b71b00 (u64) = 0x{val64:016x} = {val64:,}")
val64 = read_u64(0x5b8d80)
print(f"  DAT_005b8d80 (u64) = 0x{val64:016x} = {val64:,}")

print()
print("=" * 80)
print("Runtime pointer globals (.bss/.data - may need relocation)")
print("=" * 80)
# These are pointers to vectors in .data.rel.ro, populated at load time
# LogicGraduateDevice rate arrays:
# DAT_0307aa30/38 -> digital rate array (u32 elements, 4 bytes each)
# DAT_0307aa48/50 -> analog rate array (u64 elements, 8 bytes each)
# LogicProfessionalDevice rate arrays:
# DAT_0307aa60/68 -> digital rate array
# DAT_0307aa78/80 -> analog rate array
# DAT_0307aa90/98 -> threshold strings

# These are in .bss (not in file). Let me look for them in .data.rel.ro
# The actual data would be pointed to by these runtime pointers
# Let me search for known rate values in .data.rel.ro

print()
print("=" * 80)
print("Searching .data.rel.ro for sample rate arrays")
print("=" * 80)
drelro_va = 0x2e2ea60
drelro_off = 0x2e2ca60
drelro_size = 0xba1a8
drelro = data[drelro_off:drelro_off + drelro_size]

# Search for 500000000 (0x1DCD6500) as u32
target_u32 = 500000000
for i in range(0, len(drelro) - 4, 4):
    val = struct.unpack_from('<I', drelro, i)[0]
    if val == target_u32:
        va = drelro_va + i
        # Print surrounding context
        context_start = max(0, i - 32)
        context_end = min(len(drelro), i + 64)
        vals = []
        for j in range(context_start, context_end, 4):
            v = struct.unpack_from('<I', drelro, j)[0]
            vals.append(v)
        print(f"  Found 500M at VA 0x{va:08x} (offset 0x{i:x} in .data.rel.ro)")
        print(f"    Context (u32): {vals}")

# Search for 200000000 (0x0BEBC200) as u32
target_u32 = 200000000
for i in range(0, len(drelro) - 4, 4):
    val = struct.unpack_from('<I', drelro, i)[0]
    if val == target_u32:
        va = drelro_va + i
        context_start = max(0, i - 16)
        context_end = min(len(drelro), i + 48)
        vals = []
        for j in range(context_start, context_end, 4):
            v = struct.unpack_from('<I', drelro, j)[0]
            vals.append(v)
        print(f"  Found 200M at VA 0x{va:08x} (offset 0x{i:x} in .data.rel.ro)")
        print(f"    Context (u32): {vals}")

# Search in .rodata too
rodata_va = 0x34a580
rodata_off = 0x34a580
rodata_size = 0x13e44a0
rodata = data[rodata_off:rodata_off + rodata_size]

print()
print("Searching .rodata for sample rate arrays...")
for target, name in [(500000000, "500M"), (200000000, "200M"), (100000000, "100M"), (50000000, "50M")]:
    found = []
    for i in range(0, min(len(rodata), rodata_size) - 4, 4):
        val = struct.unpack_from('<I', rodata, i)[0]
        if val == target:
            va = rodata_va + i
            found.append(va)
    if found:
        print(f"  {name} found at: {', '.join(f'0x{x:08x}' for x in found[:10])}")
        # Show context for first occurrence
        for va in found[:3]:
            off = va - rodata_va
            context_start = max(0, off - 16)
            context_end = min(len(rodata), off + 64)
            vals = []
            for j in range(context_start, context_end, 4):
                v = struct.unpack_from('<I', rodata, j)[0]
                vals.append(v)
            print(f"    Context at 0x{va:08x}: {vals}")

print()
print("=" * 80)
print("DAT_000f4240 and 3000000 constants (LogicStudentDevice digital-only)")
print("=" * 80)
# 0xf4240 = 1,000,000
# 3000000 = 0x2DC6C0
print(f"  0x000f4240 = {0xf4240:,}")
print(f"  3000000 = {3000000:,}")

print()
print("=" * 80)
print("DAT_0044aa30/38 - bitrate multiplier for second entry")
print("=" * 80)
val = read_u64(0x44aa30)
print(f"  DAT_0044aa30 = 0x{val:016x} = {val}")
val = read_u64(0x44aa38)
print(f"  DAT_0044aa38 = 0x{val:016x} = {val}")

print()
print("Done.")
