#!/usr/bin/env python3
"""Probe EP2 capture data and scan FPGA register space.

Assumes device has firmware + FPGA loaded with live EP1 channel.

Usage: sudo python3 probe-ep2-and-regs.py
"""
import struct
import os
import sys
import time
import usb.core
import usb.util

VID, PID = 0x21a9, 0x1005
EP1_OUT, EP1_IN = 0x01, 0x81
EP2_IN = 0x82
TIMEOUT_MS = 5000
CONTROL_MASK, RESEED_FLAG = 0xD7, 0x20
INITIAL_SEED = 0x354b248e


def lfsr_step(s):
    fb = (s ^ (s >> 1) ^ (s >> 21) ^ (s >> 31)) & 1
    return ((s >> 1) | (fb << 31)) & 0xFFFFFFFF

def advance_seed(seed):
    n = (seed & 0x1F) + 0x23
    for _ in range(n):
        seed = lfsr_step(seed)
    return seed

def seed_byte(seed, i):
    return (seed >> ((i % 4) * 8)) & 0xFF

def encrypt_reseed(initial_seed, seed_bytes):
    key = initial_seed
    ctrl = ((seed_bytes[0] & CONTROL_MASK) | RESEED_FLAG) ^ (seed_byte(key, 0) & CONTROL_MASK)
    out = [ctrl]
    for i, b in enumerate(seed_bytes):
        out.append(b ^ seed_byte(key, i + 1))
    return bytes(out)

def encrypt_command(seed, opcode, payload):
    ctrl = (opcode & CONTROL_MASK) ^ (seed_byte(seed, 0) & CONTROL_MASK)
    out = [ctrl]
    out.append(opcode ^ seed_byte(seed, 1))
    for i, b in enumerate(payload):
        out.append(b ^ seed_byte(seed, i + 2))
    return bytes(out), advance_seed(seed)

def decrypt_response(seed, data):
    out = bytearray(len(data))
    for i, b in enumerate(data):
        out[i] = b ^ seed_byte(seed, i)
    return bytes(out), advance_seed(seed)

def send_cmd(dev, seed, opcode, payload, resp_size=512):
    cmd_wire, seed = encrypt_command(seed, opcode, payload)
    dev.write(EP1_OUT, cmd_wire, timeout=TIMEOUT_MS)
    resp = dev.read(EP1_IN, resp_size, timeout=TIMEOUT_MS)
    dec, seed = decrypt_response(seed, bytes(resp))
    return dec, seed

def write_regs(dev, seed, pairs):
    payload = [len(pairs)]
    for a, v in pairs:
        payload.extend([a, v])
    return send_cmd(dev, seed, 0x80, payload)

def read_regs(dev, seed, addrs):
    payload = [len(addrs)] + list(addrs)
    return send_cmd(dev, seed, 0x81, payload)

def hexdump(data, prefix="  ", max_rows=None):
    total = (len(data) + 15) // 16
    rows = total if max_rows is None else min(total, max_rows)
    for row in range(rows):
        off = row * 16
        hex_part = ' '.join(f'{b:02x}' for b in data[off:off+16])
        ascii_part = ''.join(chr(b) if 32 <= b < 127 else '.' for b in data[off:off+16])
        print(f"{prefix}{off:04x}: {hex_part:<48s} {ascii_part}")
    if max_rows and total > max_rows:
        print(f"{prefix}... ({total - max_rows} more rows)")


def main():
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if not dev:
        print("No device")
        sys.exit(1)

    try:
        if dev.is_kernel_driver_active(0):
            dev.detach_kernel_driver(0)
    except:
        pass

    dev.set_configuration(1)
    usb.util.claim_interface(dev, 0)
    try:
        dev.ctrl_transfer(0x02, 0x01, 0x00, EP1_IN, timeout=1000)
    except:
        pass

    # Reseed
    new_bytes = list(os.urandom(4))
    if all(b == 0 for b in new_bytes):
        new_bytes[0] = 1
    new_seed = struct.unpack('<I', bytes(new_bytes))[0]
    reseed_wire = encrypt_reseed(INITIAL_SEED, new_bytes)
    dev.write(EP1_OUT, reseed_wire, timeout=TIMEOUT_MS)
    seed = new_seed

    resp, seed = send_cmd(dev, seed, 0x7D, [0x55])
    assert resp[0] == 0xAA, f"Ping failed: {resp[0]:#04x}"
    print("Ping OK")

    # Verify FPGA
    resp, seed = read_regs(dev, seed, [0x7F])
    print(f"r0x7F = {resp[0]:#04x} (expect 0xAA)")
    if resp[0] != 0xAA:
        print("FPGA not responsive")
        sys.exit(1)

    # ======================================================================
    # Part 1: Full FPGA register scan (0x00-0xFF)
    # ======================================================================
    print("\n=== FPGA Register Scan (0x00-0xFF) ===")
    reg_map = {}
    for reg in range(0x00, 0x100):
        try:
            resp, seed = read_regs(dev, seed, [reg])
            val = resp[0]
            reg_map[reg] = val
            if val != 0x00:
                print(f"  r{reg:#04x} = {val:#04x}")
        except Exception as e:
            print(f"  r{reg:#04x}: FAILED - {e}")
            break

    print(f"\nNon-zero registers: {sum(1 for v in reg_map.values() if v != 0)}/{len(reg_map)}")

    # ======================================================================
    # Part 2: Test register read/write
    # ======================================================================
    print("\n=== Register Read/Write Tests ===")

    # Test which registers are writable by writing and reading back
    # Use a non-destructive approach: read current, write test, read back, restore
    test_regs = [0x01, 0x02, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
                 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14]
    for reg in test_regs:
        orig = reg_map.get(reg, 0)
        try:
            resp, seed = write_regs(dev, seed, [(reg, 0x55)])
            resp, seed = read_regs(dev, seed, [reg])
            after_write = resp[0]
            # Restore
            resp, seed = write_regs(dev, seed, [(reg, orig)])
            if after_write == 0x55:
                print(f"  r{reg:#04x}: WRITABLE (wrote 0x55, read 0x55)")
            elif after_write == orig:
                print(f"  r{reg:#04x}: READ-ONLY (wrote 0x55, still {orig:#04x})")
            else:
                print(f"  r{reg:#04x}: PARTIAL (wrote 0x55, got {after_write:#04x}, was {orig:#04x})")
        except Exception as e:
            print(f"  r{reg:#04x}: ERROR - {e}")

    # ======================================================================
    # Part 3: EP2 capture data test
    # ======================================================================
    print("\n=== EP2 Capture Data Test ===")

    # First try reading EP2 without StartRead - should timeout
    print("  Reading EP2 before StartRead...")
    try:
        data = dev.read(EP2_IN, 16384, timeout=1000)
        print(f"  Got {len(data)} bytes (unexpected!)")
        hexdump(bytes(data), max_rows=4)
    except Exception as e:
        print(f"  Expected timeout: {e}")

    # Send StartRead
    resp, seed = send_cmd(dev, seed, 0x01, [])
    print(f"  StartRead: {resp.hex()}")

    # Try reading EP2 after StartRead
    print("  Reading EP2 after StartRead...")
    ep2_total = 0
    ep2_samples = bytearray()
    for i in range(10):
        try:
            data = dev.read(EP2_IN, 16384, timeout=2000)
            chunk = bytes(data)
            ep2_total += len(chunk)
            if i == 0:
                ep2_samples.extend(chunk[:256])  # Save first 256 bytes
            print(f"  EP2 read #{i}: {len(chunk)} bytes")
        except Exception as e:
            print(f"  EP2 read #{i}: {e}")
            break

    if ep2_total > 0:
        print(f"\n  Total EP2 data: {ep2_total} bytes")
        print(f"  First 256 bytes of capture data:")
        hexdump(ep2_samples, max_rows=16)

        # Save raw capture
        with open("/tmp/saleae-ep2-raw.bin", 'wb') as f:
            f.write(ep2_samples)
        print(f"  Saved to /tmp/saleae-ep2-raw.bin")

    # StopStream
    resp, seed = send_cmd(dev, seed, 0x02, [])
    print(f"  StopStream: {resp.hex()}")

    # Drain any remaining EP2 data
    drained = 0
    for _ in range(5):
        try:
            data = dev.read(EP2_IN, 16384, timeout=500)
            drained += len(data)
        except:
            break
    if drained:
        print(f"  Drained {drained} remaining EP2 bytes")

    # Final ping
    resp, seed = send_cmd(dev, seed, 0x7D, [0x33])
    expected = (~0x33) & 0xFF
    print(f"\nFinal ping: {'OK' if resp[0] == expected else 'FAIL'}")

    usb.util.release_interface(dev, 0)
    print("Done.")


if __name__ == '__main__':
    main()
