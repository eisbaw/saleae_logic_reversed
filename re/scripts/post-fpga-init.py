#!/usr/bin/env python3
"""Run post-FPGA init sequence (Phase 3) on a device with FPGA already programmed.

Assumes EP1 is alive and FPGA is already loaded. Does reseed to sync state.

Usage: sudo python3 post-fpga-init.py
"""
import struct
import os
import sys
import time
import usb.core
import usb.util

VID, PID = 0x21a9, 0x1005
EP1_OUT, EP1_IN = 0x01, 0x81
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
    """WriteRegisters (0x80): list of (addr, val) pairs."""
    payload = [len(pairs)]
    for a, v in pairs:
        payload.extend([a, v])
    return send_cmd(dev, seed, 0x80, payload)

def read_regs(dev, seed, addrs):
    """ReadRegisters (0x81): list of addresses."""
    payload = [len(addrs)] + list(addrs)
    return send_cmd(dev, seed, 0x81, payload)


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

    # Reseed to sync state
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

    # Verify FPGA is still responsive
    resp, seed = read_regs(dev, seed, [0x7F])
    print(f"r0x7F = {resp[0]:#04x} (expect 0xAA)")
    if resp[0] != 0xAA:
        print("FPGA not responsive, need to reprogram")
        sys.exit(1)

    # ======================================================================
    # Phase 3b: ADC Init (from pcap Phase 3b, frames 1385-1468)
    # ======================================================================
    print("\n=== ADC Init ===")

    # ADC writes via proxy registers:
    # r0x03 = register address, r0x16 = bank (always 0x00), r0x04 = value, r0x05 = commit (0x00)
    adc_sequence = [
        # Bank 0x08: reset sequence (multiple values)
        (0x08, 0x03),
        (0x08, 0x00),
        (0x08, 0x01),
        (0x08, 0x00),
        # Individual registers
        (0x0D, 0x00),
        (0x15, 0x21),
        (0x21, 0xC2),
        (0x05, 0x0F),
        (0x04, 0x0F),
        (0x10, 0x00),
        (0x18, 0x04),
    ]

    for reg_addr, value in adc_sequence:
        resp, seed = write_regs(dev, seed, [
            (0x03, reg_addr),
            (0x16, 0x00),
            (0x04, value),
            (0x05, 0x00),
        ])
        print(f"  ADC[{reg_addr:#04x}] = {value:#04x}  ack={resp.hex()}")

    # r0x15 = 0x00 (bank power off)
    resp, seed = write_regs(dev, seed, [(0x15, 0x00)])
    print(f"  r0x15=0x00 (power off)  ack={resp.hex()}")

    # Bank 0x08 = 0x01 (enable)
    resp, seed = write_regs(dev, seed, [
        (0x03, 0x08),
        (0x16, 0x00),
        (0x04, 0x01),
        (0x05, 0x00),
    ])
    print(f"  ADC[0x08]=0x01 (enable)  ack={resp.hex()}")

    # ======================================================================
    # Phase 3c: JESD Link (from pcap, frames 1429-1508)
    # ======================================================================
    print("\n=== JESD Link Establishment ===")

    # Power-down/up sequence: r0x00 transitions
    for val in [0x1E, 0x1C, 0x18, 0x08, 0x00]:
        resp, seed = write_regs(dev, seed, [(0x00, val)])
        print(f"  r0x00={val:#04x}  ack={resp.hex()}")

    time.sleep(0.05)

    # Poll JESD status
    resp, seed = read_regs(dev, seed, [0x40])
    jesd_status = resp[0]
    print(f"  r0x40 = {jesd_status:#04x} (JESD status, want 0xE0)")

    resp, seed = read_regs(dev, seed, [0x4A])
    print(f"  r0x4A = {resp[0]:#04x}")

    if jesd_status != 0xE0:
        print("  Retrying JESD power cycle...")
        time.sleep(0.05)
        for val in [0x1E, 0x1C, 0x18, 0x08, 0x00]:
            resp, seed = write_regs(dev, seed, [(0x00, val)])
        time.sleep(0.05)

        resp, seed = read_regs(dev, seed, [0x40])
        jesd_status = resp[0]
        print(f"  r0x40 = {jesd_status:#04x} (after retry)")

        resp, seed = read_regs(dev, seed, [0x4A])
        print(f"  r0x4A = {resp[0]:#04x}")

    # Additional status reads from pcap
    resp, seed = read_regs(dev, seed, [0x19])
    print(f"  r0x19 = {resp[0]:#04x}")

    resp, seed = read_regs(dev, seed, [0x40])
    print(f"  r0x40 = {resp[0]:#04x} (confirm)")

    # ======================================================================
    # Phase 3d: Calibration coefficients
    # ======================================================================
    print("\n=== Calibration ===")

    # Read 16 cal bytes from EEPROM offset 0x10
    resp, seed = send_cmd(dev, seed, 0x07, [0x33, 0x81, 0x10, 0x00, 0x10, 0x00])
    cal = resp[:16]
    print(f"  EEPROM cal: {cal.hex()}")

    # Write to FPGA r0x17
    for b in cal:
        resp, seed = write_regs(dev, seed, [(0x17, b)])
    print(f"  Wrote {len(cal)} cal coefficients to r0x17")

    # ======================================================================
    # Phase 4: Post-init status (from pcap, frames 1509+)
    # ======================================================================
    print("\n=== Post-Init Register Dump ===")
    for reg in range(0x00, 0x20):
        try:
            resp, seed = read_regs(dev, seed, [reg])
            val = resp[0]
            if val != 0x00:  # Only show non-zero
                print(f"  r{reg:#04x} = {val:#04x}")
        except Exception as e:
            print(f"  r{reg:#04x}: FAILED")

    for reg in [0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x4A, 0x7F]:
        try:
            resp, seed = read_regs(dev, seed, [reg])
            print(f"  r{reg:#04x} = {resp[0]:#04x}")
        except Exception as e:
            print(f"  r{reg:#04x}: FAILED")

    # ======================================================================
    # Test: Start/Stop capture
    # ======================================================================
    print("\n=== StartRead / StopStream ===")
    resp, seed = send_cmd(dev, seed, 0x01, [])
    print(f"  StartRead: {resp.hex()}")

    time.sleep(0.5)

    resp, seed = send_cmd(dev, seed, 0x02, [])
    print(f"  StopStream: {resp.hex()}")

    # Final ping to verify seed still in sync
    resp, seed = send_cmd(dev, seed, 0x7D, [0x42])
    expected = (~0x42) & 0xFF
    if resp[0] == expected:
        print(f"\nSeed still in sync. All commands successful.")
    else:
        print(f"\nSeed DESYNC: got {resp[0]:#04x}, expected {expected:#04x}")

    usb.util.release_interface(dev, 0)
    print("Done.")


if __name__ == '__main__':
    main()
