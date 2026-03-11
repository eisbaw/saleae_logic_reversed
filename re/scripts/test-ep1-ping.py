#!/usr/bin/env python3
"""Test EP1 ping against a Saleae Logic Pro 8 that booted from SPI flash.

This replicates the ReseedDevice + Ping handshake from the Rust driver
to verify EP1 command channel functionality on real hardware.

Requirements: pyusb (pip install pyusb)
"""
import sys
import struct
import os
import usb.core
import usb.util

VID = 0x21a9
PID = 0x1005
EP1_OUT = 0x01
EP1_IN = 0x81
TIMEOUT_MS = 5000

# LFSR / encryption constants
CONTROL_MASK = 0xD7
RESEED_FLAG = 0x20
PING_OPCODE = 0x7D
PING_TEST_BYTE = 0x55
PING_EXPECTED = 0xAA


def lfsr_step(state):
    """One step of the 32-bit LFSR (taps at bits 0, 1, 21, 31)."""
    feedback = (state ^ (state >> 1) ^ (state >> 21) ^ (state >> 31)) & 1
    return ((state >> 1) | (feedback << 31)) & 0xFFFFFFFF


def advance_seed(seed):
    """Advance LFSR by (seed & 0x1F) + 0x23 steps."""
    n = (seed & 0x1F) + 0x23
    for _ in range(n):
        seed = lfsr_step(seed)
    return seed


def seed_byte(seed, i):
    """Extract byte i%4 from seed (little-endian)."""
    return (seed >> ((i % 4) * 8)) & 0xFF


def encrypt_reseed(initial_seed, seed_bytes):
    """Encrypt a ReseedDevice packet using the initial seed."""
    key = initial_seed
    control = ((seed_bytes[0] & CONTROL_MASK) | RESEED_FLAG) ^ (seed_byte(key, 0) & CONTROL_MASK)
    out = [control]
    for i, b in enumerate(seed_bytes):
        out.append(b ^ seed_byte(key, i + 1))
    return bytes(out)


def encrypt_command(seed, opcode, payload):
    """Encrypt a command. Returns (wire_bytes, new_seed)."""
    control = (opcode & CONTROL_MASK) ^ (seed_byte(seed, 0) & CONTROL_MASK)
    out = [control]
    out.append(opcode ^ seed_byte(seed, 1))
    for i, b in enumerate(payload):
        out.append(b ^ seed_byte(seed, i + 2))
    new_seed = advance_seed(seed)
    return bytes(out), new_seed


def decrypt_response(seed, data):
    """Decrypt a response. Returns (decrypted_bytes, new_seed)."""
    out = bytearray(len(data))
    for i, b in enumerate(data):
        out[i] = b ^ seed_byte(seed, i)
    new_seed = advance_seed(seed)
    return bytes(out), new_seed


def main():
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if dev is None:
        print(f"ERROR: Device {VID:04x}:{PID:04x} not found")
        sys.exit(1)

    print(f"Found: {dev.manufacturer} {dev.product} (serial={dev.serial_number})")
    print(f"  bcdDevice: {dev.bcdDevice:#06x}")
    print(f"  USB speed: {'SuperSpeed' if dev.speed == 5 else f'speed={dev.speed}'}")

    # Detach kernel driver if needed
    try:
        if dev.is_kernel_driver_active(0):
            dev.detach_kernel_driver(0)
            print("  Detached kernel driver from interface 0")
    except Exception as e:
        print(f"  (kernel driver check: {e})")

    # SET_CONFIGURATION(1) - triggers FX3 SETCONF handler
    try:
        dev.set_configuration(1)
        print("  SET_CONFIGURATION(1) OK")
    except Exception as e:
        print(f"  SET_CONFIGURATION(1): {e}")

    # Claim interface
    usb.util.claim_interface(dev, 0)
    print("  Claimed interface 0")

    # Clear halt on EP1 endpoints (as pcap shows)
    try:
        usb.util.clear_halt(dev, EP1_OUT)
        print("  CLEAR_HALT(EP1 OUT) OK")
    except Exception as e:
        print(f"  CLEAR_HALT(EP1 OUT): {e}")

    try:
        usb.util.clear_halt(dev, EP1_IN)
        print("  CLEAR_HALT(EP1 IN) OK")
    except Exception as e:
        print(f"  CLEAR_HALT(EP1 IN): {e}")

    # Test 1: Try a vendor control transfer (should work on SPI-booted device)
    print("\n--- Test: Vendor control transfer ---")
    try:
        # Read device type (bRequest=0x80, vendor, device-to-host)
        data = dev.ctrl_transfer(0xC0, 0x80, 0, 0, 64, timeout=TIMEOUT_MS)
        print(f"  Vendor IN (bRequest=0x80): {data.tobytes().hex()} ({len(data)} bytes)")
    except Exception as e:
        print(f"  Vendor IN (bRequest=0x80): FAILED - {e}")

    # Test 2: EP1 ReseedDevice + Ping
    print("\n--- Test: EP1 ReseedDevice + Ping ---")

    # Initial encryption seed (the pre-reseed state)
    # When the device boots fresh, the initial seed is 0x354b248e
    # (built into the firmware). We start with this.
    initial_seed = 0x354b248e

    # Generate a random seed for the reseed command
    new_seed_bytes = list(os.urandom(4))
    # Avoid zero seed
    if all(b == 0 for b in new_seed_bytes):
        new_seed_bytes[0] = 1
    new_seed = struct.unpack('<I', bytes(new_seed_bytes))[0]
    print(f"  New seed: {new_seed:#010x} (bytes: {bytes(new_seed_bytes).hex()})")

    # Encrypt and send ReseedDevice
    reseed_wire = encrypt_reseed(initial_seed, new_seed_bytes)
    print(f"  ReseedDevice wire: {reseed_wire.hex()}")
    try:
        written = dev.write(EP1_OUT, reseed_wire, timeout=TIMEOUT_MS)
        print(f"  EP1 OUT (reseed): wrote {written} bytes OK")
    except Exception as e:
        print(f"  EP1 OUT (reseed): FAILED - {e}")
        sys.exit(1)

    # Now the rolling seed is the new seed
    seed = new_seed

    # Encrypt and send Ping
    ping_wire, seed = encrypt_command(seed, PING_OPCODE, [PING_TEST_BYTE])
    print(f"  Ping wire: {ping_wire.hex()}")
    try:
        written = dev.write(EP1_OUT, ping_wire, timeout=TIMEOUT_MS)
        print(f"  EP1 OUT (ping): wrote {written} bytes OK")
    except Exception as e:
        print(f"  EP1 OUT (ping): FAILED - {e}")
        sys.exit(1)

    # Read ping response
    try:
        resp_raw = dev.read(EP1_IN, 64, timeout=TIMEOUT_MS)
        resp_bytes = bytes(resp_raw)
        print(f"  EP1 IN (raw): {resp_bytes.hex()} ({len(resp_bytes)} bytes)")

        # Decrypt
        decrypted, seed = decrypt_response(seed, resp_bytes)
        print(f"  EP1 IN (decrypted): {decrypted.hex()}")

        if len(decrypted) >= 1 and decrypted[0] == PING_EXPECTED:
            print(f"  PING SUCCESS: got 0x{decrypted[0]:02x} (expected 0x{PING_EXPECTED:02x})")
        else:
            print(f"  PING MISMATCH: got 0x{decrypted[0]:02x}, expected 0x{PING_EXPECTED:02x}")
            print(f"  (This could mean wrong initial_seed or LFSR bug)")
    except Exception as e:
        print(f"  EP1 IN (ping response): FAILED - {e}")
        print("  The firmware may not be processing EP1 commands.")

    # Test 3: Try reading device info (opcode 0x8B = ReadChipId or similar)
    print("\n--- Test: ReadDeviceString (opcode 0x8B) ---")
    try:
        cmd_wire, seed = encrypt_command(seed, 0x8B, [])
        print(f"  Command wire: {cmd_wire.hex()}")
        written = dev.write(EP1_OUT, cmd_wire, timeout=TIMEOUT_MS)
        print(f"  EP1 OUT: wrote {written} bytes")

        resp_raw = dev.read(EP1_IN, 256, timeout=TIMEOUT_MS)
        resp_bytes = bytes(resp_raw)
        print(f"  EP1 IN (raw): {resp_bytes[:32].hex()}... ({len(resp_bytes)} bytes)")

        decrypted, seed = decrypt_response(seed, resp_bytes)
        # Try to decode as ASCII
        try:
            text = decrypted.decode('ascii', errors='replace')
            print(f"  Decrypted: {text[:64]}")
        except:
            print(f"  Decrypted (hex): {decrypted[:32].hex()}")
    except Exception as e:
        print(f"  ReadDeviceString: FAILED - {e}")

    usb.util.release_interface(dev, 0)
    print("\nDone.")


if __name__ == '__main__':
    main()
