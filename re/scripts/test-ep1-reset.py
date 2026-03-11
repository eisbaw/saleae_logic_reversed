#!/usr/bin/env python3
"""Try USB reset + EP1 ping on Saleae Logic Pro 8.

Tries USB device reset to force re-enumeration, then tests EP1.
"""
import sys
import struct
import os
import time
import usb.core
import usb.util

VID = 0x21a9
PID = 0x1005
BOOT_VID = 0x04b4
BOOT_PID = 0x00f3
EP1_OUT = 0x01
EP1_IN = 0x81
TIMEOUT_MS = 5000

CONTROL_MASK = 0xD7
RESEED_FLAG = 0x20
PING_OPCODE = 0x7D
PING_TEST_BYTE = 0x55
PING_EXPECTED = 0xAA


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


def find_and_test():
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if dev is None:
        # Check for bootloader
        boot = usb.core.find(idVendor=BOOT_VID, idProduct=BOOT_PID)
        if boot:
            print(f"Device is in bootloader mode ({BOOT_VID:04x}:{BOOT_PID:04x})")
            print("Need firmware upload first. Cannot test EP1.")
        else:
            print("No Saleae device found at all.")
        sys.exit(1)

    print(f"Found: {dev.manufacturer} {dev.product}")
    print(f"  bcdDevice={dev.bcdDevice:#06x} speed={dev.speed}")
    print(f"  bus={dev.bus} address={dev.address}")

    # Step 1: USB reset
    print("\n--- USB Reset ---")
    try:
        dev.reset()
        print("  USB reset OK")
        time.sleep(2)
    except Exception as e:
        print(f"  USB reset: {e}")
        time.sleep(2)

    # Re-find device after reset
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if dev is None:
        # Maybe it went back to bootloader?
        boot = usb.core.find(idVendor=BOOT_VID, idProduct=BOOT_PID)
        if boot:
            print("  Device reverted to bootloader after reset!")
            print(f"  bcdDevice={boot.bcdDevice:#06x}")
            sys.exit(0)
        print("  Device disappeared after reset!")
        sys.exit(1)

    print(f"  After reset: bcdDevice={dev.bcdDevice:#06x} speed={dev.speed}")

    # Detach kernel driver
    try:
        if dev.is_kernel_driver_active(0):
            dev.detach_kernel_driver(0)
    except:
        pass

    # Try WITHOUT SET_CONFIGURATION first (device may already be configured from SPI boot)
    print("\n--- Test: Skip SET_CONFIGURATION ---")
    try:
        usb.util.claim_interface(dev, 0)
        print("  Claimed interface 0 (no SET_CONFIG)")
    except Exception as e:
        print(f"  Claim failed: {e}")
        # Try with SET_CONFIGURATION
        try:
            dev.set_configuration(1)
            usb.util.claim_interface(dev, 0)
            print("  SET_CONFIGURATION(1) + claim OK")
        except Exception as e2:
            print(f"  Failed: {e2}")
            sys.exit(1)

    # Clear halt via raw control transfer (CLEAR_FEATURE)
    for ep in [EP1_OUT, EP1_IN]:
        try:
            dev.ctrl_transfer(0x02, 0x01, 0x00, ep, timeout=1000)
            print(f"  CLEAR_FEATURE(ENDPOINT_HALT) ep={ep:#04x} OK")
        except Exception as e:
            print(f"  CLEAR_FEATURE ep={ep:#04x}: {e}")

    # Test vendor control
    print("\n--- Test: Vendor control ---")
    try:
        data = dev.ctrl_transfer(0xC0, 0x80, 0, 0, 64, timeout=TIMEOUT_MS)
        print(f"  Vendor IN 0x80: {data.tobytes().hex()} ({len(data)} bytes)")
    except Exception as e:
        print(f"  Vendor IN 0x80: FAILED - {e}")

    # EP1 ping
    print("\n--- Test: EP1 Ping ---")
    initial_seed = 0x354b248e
    new_seed_bytes = list(os.urandom(4))
    if all(b == 0 for b in new_seed_bytes):
        new_seed_bytes[0] = 1
    new_seed = struct.unpack('<I', bytes(new_seed_bytes))[0]
    print(f"  Seed: {new_seed:#010x}")

    reseed_wire = encrypt_reseed(initial_seed, new_seed_bytes)
    try:
        written = dev.write(EP1_OUT, reseed_wire, timeout=TIMEOUT_MS)
        print(f"  Reseed: wrote {written} bytes")
    except Exception as e:
        print(f"  Reseed OUT: FAILED - {e}")
        usb.util.release_interface(dev, 0)
        sys.exit(1)

    seed = new_seed
    ping_wire, seed = encrypt_command(seed, PING_OPCODE, [PING_TEST_BYTE])
    try:
        written = dev.write(EP1_OUT, ping_wire, timeout=TIMEOUT_MS)
        print(f"  Ping OUT: wrote {written} bytes")
    except Exception as e:
        print(f"  Ping OUT: FAILED - {e}")
        usb.util.release_interface(dev, 0)
        sys.exit(1)

    try:
        resp = dev.read(EP1_IN, 64, timeout=TIMEOUT_MS)
        resp_bytes = bytes(resp)
        print(f"  Ping IN (raw): {resp_bytes.hex()}")
        decrypted, seed = decrypt_response(seed, resp_bytes)
        print(f"  Ping IN (dec): {decrypted.hex()}")
        if decrypted[0] == PING_EXPECTED:
            print(f"  >>> PING SUCCESS! <<<")
        else:
            print(f"  Ping mismatch: got {decrypted[0]:#04x}, expected {PING_EXPECTED:#04x}")
    except Exception as e:
        print(f"  Ping IN: FAILED - {e}")

    usb.util.release_interface(dev, 0)


if __name__ == '__main__':
    find_and_test()
