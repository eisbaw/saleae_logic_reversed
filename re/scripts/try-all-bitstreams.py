#!/usr/bin/env python3
"""Try all SLX9 bitstreams to find the correct one for this device.

Requires device to already have firmware running with a live EP1 channel.
Does ResetFpga between each attempt. Stops on first 0xAA readback.

Usage: sudo python3 try-all-bitstreams.py
"""
import struct
import os
import sys
import glob
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


def main():
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if not dev:
        print("No device")
        sys.exit(1)
    print(f"bcdDevice={dev.bcdDevice:#06x}")

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

    # Ping
    resp, seed = send_cmd(dev, seed, 0x7D, [0x55])
    assert resp[0] == 0xAA, f"Ping failed: {resp[0]:#04x}"
    print("Ping OK")

    candidates = sorted(glob.glob('/tmp/bitstream-spartan6-slx9-*.bin'))
    print(f"Candidates: {[os.path.basename(c) for c in candidates]}")

    for bs_path in candidates:
        name = os.path.basename(bs_path)
        with open(bs_path, 'rb') as f:
            bitstream = f.read()

        # ResetFpga
        resp, seed = send_cmd(dev, seed, 0x7E, [])
        if resp[0] != 0:
            print(f"  {name}: ResetFpga failed ({resp[0]:#04x})")
            continue

        # Upload bitstream
        chunk_size = 1020
        offset = 0
        chunks = 0
        ok = True
        while offset < len(bitstream):
            end = min(offset + chunk_size, len(bitstream))
            chunk = bitstream[offset:end]
            clen = len(chunk)
            payload = [clen & 0xFF, (clen >> 8) & 0xFF] + list(chunk)
            resp, seed = send_cmd(dev, seed, 0x7F, payload)
            if resp[0] != 0:
                print(f"  {name}: chunk {chunks} bad ack ({resp[0]:#04x})")
                ok = False
                break
            chunks += 1
            offset = end
        if not ok:
            continue

        # Write 0xAA to r0x7F, read back
        resp, seed = send_cmd(dev, seed, 0x80, [1, 0x7F, 0xAA])
        resp, seed = send_cmd(dev, seed, 0x81, [1, 0x7F])
        readback = resp[0]

        if readback == 0xAA:
            print(f"  {name}: FPGA RESPONSIVE (readback=0xAA) *** MATCH ***")
            # Also try reading r0x00 and r0x40
            resp, seed = send_cmd(dev, seed, 0x81, [1, 0x00])
            print(f"    r0x00 = {resp[0]:#04x}")
            resp, seed = send_cmd(dev, seed, 0x81, [1, 0x40])
            print(f"    r0x40 = {resp[0]:#04x}")
            break
        else:
            print(f"  {name}: readback={readback:#04x} (wrong)")

    usb.util.release_interface(dev, 0)
    print("Done.")


if __name__ == '__main__':
    main()
