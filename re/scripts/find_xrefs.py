#!/usr/bin/env python3
"""Find RIP-relative LEA references to the ComputationUtilities.cpp string."""

import struct
import subprocess

BINARY = "/nix/store/brcaw41j62pjki70ahf7ffd2f3bhaa3d-saleae-logic-2-extracted/resources/linux-x64/libgraph_server_shared.so"

def main():
    target_va = 0x3e7583  # VA of the ComputationUtilities.cpp full path string

    # .text: VA=0x1a016c0, foff=0x1a006c0, size=0x1409b30
    text_va = 0x1a016c0
    text_foff = 0x1a006c0
    text_size = 0x1409b30
    va_offset = text_va - text_foff  # 0x1000

    with open(BINARY, "rb") as f:
        data = f.read()

    results = []
    scan_start = text_foff
    scan_end = text_foff + text_size - 7

    for foff in range(scan_start, scan_end):
        b0 = data[foff]
        b1 = data[foff + 1]
        if b1 != 0x8d:
            continue
        if b0 not in (0x48, 0x4c):
            continue
        b2 = data[foff + 2]
        if (b2 & 0xC7) != 0x05:
            continue
        disp = struct.unpack_from("<i", data, foff + 3)[0]
        rip_va = (foff + 7) + va_offset
        effective = rip_va + disp
        if effective == target_va:
            instr_va = foff + va_offset
            results.append((foff, instr_va))

    print(f"Found {len(results)} LEA references to VA 0x{target_va:x}")
    for foff, va in results:
        ctx = data[foff:foff+20].hex()
        print(f"  ELF VA 0x{va:x} (foff 0x{foff:x})  bytes: {ctx}")

    # For each reference, find surrounding string references to identify assertion context
    for foff, va in results:
        print(f"\n--- Context around VA 0x{va:x} ---")
        window_start = max(text_foff, foff - 200)
        window_end = min(text_foff + text_size, foff + 200)
        for woff in range(window_start, window_end - 7):
            wb0 = data[woff]
            wb1 = data[woff + 1]
            if wb1 != 0x8d:
                continue
            if wb0 not in (0x48, 0x4c):
                continue
            wb2 = data[woff + 2]
            if (wb2 & 0xC7) != 0x05:
                continue
            wdisp = struct.unpack_from("<i", data, woff + 3)[0]
            wrip = (woff + 7) + va_offset
            weff = wrip + wdisp
            if 0x34a580 <= weff < 0x64a580:
                str_start = weff
                str_end = str_start
                while str_end < len(data) and data[str_end] != 0 and str_end - str_start < 300:
                    str_end += 1
                try:
                    s = data[str_start:str_end].decode("ascii")
                    if len(s) > 3 and s.isprintable():
                        wva = woff + va_offset
                        print(f"  VA 0x{wva:x}: -> string at 0x{weff:x}: {s!r}")
                except:
                    pass

if __name__ == "__main__":
    main()
