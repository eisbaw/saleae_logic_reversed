#!/usr/bin/env python3
"""Attempt a capture: reprogram FPGA, configure, start, read EP2.

Follows pcap Phase 7 capture parameters exactly.
Skips JESD (not applicable to Spartan-6) but does ADC init and capture config.

Requires live EP1 channel (device has firmware loaded).

Usage: sudo python3 capture-test.py
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
    assert resp[0] == 0xAA, f"Ping failed"
    print("Ping OK")

    # ======================================================================
    # Step 1: Reprogram FPGA (clean slate)
    # ======================================================================
    print("\n=== Reprogram FPGA ===")
    resp, seed = send_cmd(dev, seed, 0x7E, [])
    assert resp[0] == 0x00, "ResetFpga failed"

    bs_path = "/tmp/bitstream-spartan6-slx9-02.bin"
    with open(bs_path, 'rb') as f:
        bitstream = f.read()

    chunk_size = 1020
    offset = 0
    chunks = 0
    while offset < len(bitstream):
        end = min(offset + chunk_size, len(bitstream))
        chunk = bitstream[offset:end]
        clen = len(chunk)
        payload = [clen & 0xFF, (clen >> 8) & 0xFF] + list(chunk)
        resp, seed = send_cmd(dev, seed, 0x7F, payload)
        assert resp[0] == 0x00, f"Chunk {chunks} failed"
        chunks += 1
        offset = end
    print(f"Uploaded {len(bitstream)} bytes in {chunks} chunks")

    # Verify
    resp, seed = write_regs(dev, seed, [(0x7F, 0xAA)])
    resp, seed = read_regs(dev, seed, [0x7F])
    assert resp[0] == 0xAA, f"FPGA not responsive: {resp[0]:#04x}"
    print("FPGA verified (r0x7F=0xAA)")

    # Check initial r0x40 state
    resp, seed = read_regs(dev, seed, [0x40])
    print(f"r0x40 after program: {resp[0]:#04x}")

    # ======================================================================
    # Step 2: ADC Init (pcap Phase 3b - the indirect register writes)
    # ======================================================================
    print("\n=== ADC Init ===")
    adc_sequence = [
        (0x08, 0x03), (0x08, 0x00), (0x08, 0x01), (0x08, 0x00),
        (0x0D, 0x00), (0x15, 0x21), (0x21, 0xC2), (0x05, 0x0F),
        (0x04, 0x0F), (0x10, 0x00), (0x18, 0x04),
    ]
    for reg_addr, value in adc_sequence:
        resp, seed = write_regs(dev, seed, [
            (0x03, reg_addr), (0x16, 0x00), (0x04, value), (0x05, 0x00),
        ])
    print("ADC init done (11 writes)")

    # r0x15=0x00 then ADC[0x08]=0x01
    resp, seed = write_regs(dev, seed, [(0x15, 0x00)])
    resp, seed = write_regs(dev, seed, [
        (0x03, 0x08), (0x16, 0x00), (0x04, 0x01), (0x05, 0x00),
    ])
    print("ADC enable done")

    # ======================================================================
    # Step 3: Post-FPGA (pcap Phase 3d - skip JESD, do calibration)
    # ======================================================================
    print("\n=== Calibration ===")
    # Write r0x00=0x00 then r0x00=0x40 (from pcap Phase 3d)
    resp, seed = write_regs(dev, seed, [(0x00, 0x00)])
    resp, seed = write_regs(dev, seed, [(0x00, 0x40)])
    print("r0x00: 0x00 -> 0x40")

    # Load calibration from EEPROM
    resp, seed = send_cmd(dev, seed, 0x07, [0x33, 0x81, 0x10, 0x00, 0x10, 0x00])
    cal = resp[:16]
    for b in cal:
        resp, seed = write_regs(dev, seed, [(0x17, b)])
    print(f"Loaded 16 cal coefficients: {cal.hex()}")

    # ======================================================================
    # Step 4: Capture Parameters (pcap Phase 7)
    # ======================================================================
    print("\n=== Capture Config ===")

    # From pcap Phase 7: ADC re-init
    resp, seed = write_regs(dev, seed, [(0x15, 0x03)])  # bank power on
    resp, seed = write_regs(dev, seed, [
        (0x03, 0x08), (0x16, 0x00), (0x04, 0x00), (0x05, 0x00),
    ])
    resp, seed = write_regs(dev, seed, [
        (0x03, 0x08), (0x16, 0x00), (0x04, 0x03), (0x05, 0x00),
    ])
    resp, seed = write_regs(dev, seed, [
        (0x03, 0x08), (0x16, 0x00), (0x04, 0x00), (0x05, 0x00),
    ])
    print("ADC re-init for capture")

    # Skip JESD power cycle (not applicable to Spartan-6)
    # But do write r0x12=0x04 (from pcap Phase 3f, may be needed)
    resp, seed = write_regs(dev, seed, [(0x12, 0x04)])
    print("r0x12=0x04")

    # Capture parameters (pcap Phase 7, 12-register write)
    capture_regs = [
        (0x00, 0x00),   # reset/stop state
        (0x08, 0xFF),   # sample config?
        (0x06, 0xFF),   # channel mask? (all channels)
        (0x0A, 0x20),   # buffer/transfer size?
        (0x0B, 0x10),   # additional config
        (0x0C, 0x00),   # config
        (0x1A, 0x00),   # config
        (0x0D, 0x18),   # sample rate divider?
        (0x0E, 0x08),   # additional rate config
        (0x12, 0x02),   # config
        (0x13, 0x00),   # config
        (0x14, 0x10),   # config
    ]
    resp, seed = write_regs(dev, seed, capture_regs)
    print(f"Capture params written (12 regs): ack={resp.hex()}")

    # Verify capture params
    addrs = [p[0] for p in capture_regs]
    resp, seed = read_regs(dev, seed, addrs)
    print("Readback: " + " ".join(f"r{a:#04x}={v:#04x}" for a, v in zip(addrs, resp)))

    # ======================================================================
    # Step 5: Start Capture
    # ======================================================================
    print("\n=== Start Capture ===")

    # StartRead command
    resp, seed = send_cmd(dev, seed, 0x01, [])
    print(f"StartRead: {resp.hex()}")

    # r0x00=0x01 triggers the capture
    resp, seed = write_regs(dev, seed, [(0x00, 0x01)])
    print(f"r0x00=0x01 (trigger): ack={resp.hex()}")

    # ======================================================================
    # Step 6: Read EP2
    # ======================================================================
    print("\n=== EP2 Data ===")
    ep2_total = 0
    ep2_data = bytearray()
    for i in range(20):
        try:
            data = dev.read(EP2_IN, 16384, timeout=2000)
            chunk = bytes(data)
            ep2_total += len(chunk)
            if len(ep2_data) < 1024:
                ep2_data.extend(chunk[:1024 - len(ep2_data)])
            if i < 5 or i % 5 == 0:
                print(f"  EP2 #{i}: {len(chunk)} bytes (total: {ep2_total})")
        except Exception as e:
            print(f"  EP2 #{i}: {e}")
            break

    # ======================================================================
    # Step 7: Stop Capture
    # ======================================================================
    print("\n=== Stop Capture ===")
    resp, seed = write_regs(dev, seed, [(0x00, 0x00)])
    print(f"r0x00=0x00 (stop): ack={resp.hex()}")

    resp, seed = send_cmd(dev, seed, 0x02, [])
    print(f"StopStream: {resp.hex()}")

    # Drain EP2
    drained = 0
    for _ in range(10):
        try:
            data = dev.read(EP2_IN, 16384, timeout=500)
            drained += len(data)
        except:
            break
    if drained:
        print(f"Drained {drained} bytes from EP2")

    # ======================================================================
    # Results
    # ======================================================================
    if ep2_total > 0:
        print(f"\nTotal EP2 data: {ep2_total} bytes")
        print("First 256 bytes:")
        hexdump(ep2_data[:256])

        with open("/tmp/saleae-capture.bin", 'wb') as f:
            f.write(ep2_data)
        print(f"Saved {len(ep2_data)} bytes to /tmp/saleae-capture.bin")

        # Basic analysis
        if len(ep2_data) >= 16:
            # Check if data looks like ADC samples (should be varying)
            unique = len(set(ep2_data[:256]))
            print(f"Unique byte values in first 256: {unique}/256")
            if unique < 5:
                print("Data looks like constant/stuck (ADC not running?)")
            else:
                print("Data has variation (ADC may be producing samples)")
    else:
        print("\nNo EP2 data received.")
        print("Possible causes:")
        print("  - ADC not clocked (Spartan-6 needs different init than ECP5)")
        print("  - GPIF data path not configured")
        print("  - Capture parameters wrong for this device revision")

    # Final register dump
    print("\n=== Post-Capture Register Dump ===")
    for reg in [0x00, 0x06, 0x08, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
                0x12, 0x13, 0x14, 0x15, 0x19, 0x1A, 0x40, 0x4A, 0x7F]:
        resp, seed = read_regs(dev, seed, [reg])
        print(f"  r{reg:#04x} = {resp[0]:#04x}")

    # Final ping
    resp, seed = send_cmd(dev, seed, 0x7D, [0x42])
    expected = (~0x42) & 0xFF
    print(f"\nFinal ping: {'OK' if resp[0] == expected else 'FAIL'}")

    usb.util.release_interface(dev, 0)
    print("Done.")


if __name__ == '__main__':
    main()
