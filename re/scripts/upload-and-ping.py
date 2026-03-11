#!/usr/bin/env python3
"""Upload FX3 firmware and immediately test EP1 ping.

Tests the hypothesis that the firmware has a watchdog timer that kills
the command processing thread if no host communication occurs within
a short window after enumeration.

Usage: python3 upload-and-ping.py <firmware.img>
"""
import sys
import struct
import os
import time
import usb.core
import usb.util

SALEAE_VID = 0x21a9
SALEAE_PID = 0x1005
BOOT_VID_CYPRESS = 0x04b4
BOOT_PID_FX3 = 0x00f3
EP1_OUT = 0x01
EP1_IN = 0x81
TIMEOUT_MS = 5000

CONTROL_MASK = 0xD7
RESEED_FLAG = 0x20
PING_OPCODE = 0x7D
PING_TEST_BYTE = 0x55
PING_EXPECTED = 0xAA

# Default initial seed (hardcoded in both host app and firmware)
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


def parse_fx3_image(data):
    """Parse Cypress FX3 .img file format."""
    if len(data) < 4 or data[0:2] != b'CY':
        raise ValueError("Not a valid FX3 image (missing CY magic)")

    image_ctl = data[2]
    image_type = data[3]
    print(f"  FX3 image: ctl={image_ctl:#04x} type={image_type:#04x}")

    sections = []
    offset = 4
    while offset + 8 <= len(data):
        length = struct.unpack_from('<I', data, offset)[0]
        address = struct.unpack_from('<I', data, offset + 4)[0]

        if length == 0:
            # Entry point record
            entry_address = address
            print(f"  Entry point: {entry_address:#010x}")
            break

        section_data = data[offset + 8: offset + 8 + length * 4]
        sections.append((address, section_data))
        print(f"  Section: addr={address:#010x} len={len(section_data)} bytes")
        offset += 8 + length * 4
    else:
        raise ValueError("No entry point found in FX3 image")

    return sections, entry_address


def upload_firmware(dev, fw_path):
    """Upload FX3 firmware via vendor control transfers."""
    with open(fw_path, 'rb') as f:
        fw_data = f.read()

    sections, entry_address = parse_fx3_image(fw_data)

    chunk_size = 4096
    for addr, section_data in sections:
        for i in range(0, len(section_data), chunk_size):
            chunk = section_data[i:i + chunk_size]
            chunk_addr = addr + i
            w_value = chunk_addr & 0xFFFF
            w_index = (chunk_addr >> 16) & 0xFFFF
            dev.ctrl_transfer(0x40, 0xA0, w_value, w_index, chunk, timeout=5000)

    # Trigger execution with zero-length transfer to entry address
    w_value = entry_address & 0xFFFF
    w_index = (entry_address >> 16) & 0xFFFF
    dev.ctrl_transfer(0x40, 0xA0, w_value, w_index, [], timeout=5000)
    print(f"  Firmware execution triggered at {entry_address:#010x}")


def wait_for_device(vid, pid, timeout_s=10):
    """Wait for a USB device to appear."""
    start = time.time()
    while time.time() - start < timeout_s:
        dev = usb.core.find(idVendor=vid, idProduct=pid)
        if dev is not None:
            return dev
        time.sleep(0.1)
    return None


def test_ep1_ping(dev, initial_seed):
    """Send reseed + ping and check response."""
    try:
        if dev.is_kernel_driver_active(0):
            dev.detach_kernel_driver(0)
    except:
        pass

    # SET_CONFIGURATION(1)
    dev.set_configuration(1)
    usb.util.claim_interface(dev, 0)

    # CLEAR_FEATURE(ENDPOINT_HALT) on EP1 IN only (matching pcap)
    try:
        dev.ctrl_transfer(0x02, 0x01, 0x00, EP1_IN, timeout=1000)
    except:
        pass

    # Generate random seed
    new_seed_bytes = list(os.urandom(4))
    if all(b == 0 for b in new_seed_bytes):
        new_seed_bytes[0] = 1
    new_seed = struct.unpack('<I', bytes(new_seed_bytes))[0]
    print(f"  New seed: {new_seed:#010x}")

    # Encrypt and send reseed
    reseed_wire = encrypt_reseed(initial_seed, new_seed_bytes)
    written = dev.write(EP1_OUT, reseed_wire, timeout=TIMEOUT_MS)
    print(f"  Reseed sent: {written} bytes ({reseed_wire.hex()})")

    # Now rolling seed = new_seed
    seed = new_seed

    # Encrypt and send ping
    ping_wire, seed = encrypt_command(seed, PING_OPCODE, [PING_TEST_BYTE])
    written = dev.write(EP1_OUT, ping_wire, timeout=TIMEOUT_MS)
    print(f"  Ping sent: {written} bytes ({ping_wire.hex()})")

    # Read response
    resp = dev.read(EP1_IN, 64, timeout=TIMEOUT_MS)
    resp_bytes = bytes(resp)
    print(f"  Response (raw): {resp_bytes.hex()}")

    decrypted, seed = decrypt_response(seed, resp_bytes)
    print(f"  Response (dec): {decrypted.hex()}")

    if len(decrypted) >= 1 and decrypted[0] == PING_EXPECTED:
        print(f"  >>> PING SUCCESS! <<<")
        return True, seed
    else:
        print(f"  Ping MISMATCH: got {decrypted[0]:#04x}, expected {PING_EXPECTED:#04x}")
        return False, seed


def main():
    if len(sys.argv) < 2:
        # Try default path
        fw_path = "firmware/fw-fx3-pro8.img"
        if not os.path.exists(fw_path):
            print(f"Usage: {sys.argv[0]} <firmware.img>")
            print(f"  Default path {fw_path} not found")
            sys.exit(1)
    else:
        fw_path = sys.argv[1]

    print(f"Firmware: {fw_path} ({os.path.getsize(fw_path)} bytes)")

    # Step 1: Find device
    # Check for Saleae device with bcdDevice=1.00 (WestBridge/bootloader)
    dev = usb.core.find(idVendor=SALEAE_VID, idProduct=SALEAE_PID)
    if dev is None:
        dev = usb.core.find(idVendor=BOOT_VID_CYPRESS, idProduct=BOOT_PID_FX3)
        if dev is None:
            print("ERROR: No Saleae or Cypress bootloader device found")
            sys.exit(1)
        print(f"Found Cypress bootloader: {BOOT_VID_CYPRESS:04x}:{BOOT_PID_FX3:04x}")
    else:
        print(f"Found Saleae device: bcdDevice={dev.bcdDevice:#06x}")
        if dev.bcdDevice == 0x0100:
            print("  Device is in bootloader mode (WestBridge)")
        elif dev.bcdDevice == 0x0000:
            print("  Device already has firmware loaded")
            print("  Skipping firmware upload, testing EP1 directly...")
            elapsed = 0
            result, _ = test_ep1_ping(dev, INITIAL_SEED)
            if not result:
                # Try with seed=0
                print("\n  Retrying with initial_seed=0...")
                usb.util.release_interface(dev, 0)
                dev.reset()
                time.sleep(1)
                dev = usb.core.find(idVendor=SALEAE_VID, idProduct=SALEAE_PID)
                if dev:
                    result, _ = test_ep1_ping(dev, 0)
            usb.util.release_interface(dev, 0)
            sys.exit(0 if result else 1)

    # Step 2: Upload firmware
    print("\n--- Uploading firmware ---")
    try:
        if dev.is_kernel_driver_active(0):
            dev.detach_kernel_driver(0)
    except:
        pass

    upload_start = time.time()
    upload_firmware(dev, fw_path)
    upload_time = time.time() - upload_start
    print(f"  Upload took {upload_time:.2f}s")

    # Step 3: Wait for re-enumeration
    print("\n--- Waiting for re-enumeration ---")
    renum_start = time.time()

    # The old device handle is now invalid
    del dev

    # Wait for new device with bcdDevice=0.00
    for attempt in range(50):
        time.sleep(0.2)
        new_dev = usb.core.find(idVendor=SALEAE_VID, idProduct=SALEAE_PID)
        if new_dev is not None and new_dev.bcdDevice == 0x0000:
            renum_time = time.time() - renum_start
            print(f"  Re-enumerated in {renum_time:.2f}s")
            print(f"  bcdDevice={new_dev.bcdDevice:#06x} speed={new_dev.speed}")
            break
    else:
        print("  ERROR: Device did not re-enumerate within 10s")
        sys.exit(1)

    # Step 4: IMMEDIATELY test EP1
    total_time = time.time() - upload_start
    print(f"\n--- Testing EP1 ({total_time:.2f}s after upload) ---")

    try:
        result, seed = test_ep1_ping(new_dev, INITIAL_SEED)
    except Exception as e:
        print(f"  EP1 test failed: {e}")
        result = False

    if not result:
        # Try with initial_seed = 0
        print(f"\n--- Retrying with initial_seed=0 ---")
        try:
            usb.util.release_interface(new_dev, 0)
        except:
            pass
        try:
            new_dev.reset()
            time.sleep(1)
            new_dev = usb.core.find(idVendor=SALEAE_VID, idProduct=SALEAE_PID)
            if new_dev:
                result, seed = test_ep1_ping(new_dev, 0)
        except Exception as e:
            print(f"  Retry failed: {e}")

    if result:
        # Try GetDeviceInfo (opcode 0x8B)
        print(f"\n--- GetDeviceInfo (opcode 0x8B) ---")
        try:
            cmd_wire, seed = encrypt_command(seed, 0x8B, [])
            written = new_dev.write(EP1_OUT, cmd_wire, timeout=TIMEOUT_MS)
            print(f"  Sent: {written} bytes")
            resp = new_dev.read(EP1_IN, 256, timeout=TIMEOUT_MS)
            resp_bytes = bytes(resp)
            decrypted, seed = decrypt_response(seed, resp_bytes)
            try:
                text = decrypted.decode('ascii', errors='replace')
                print(f"  Device info: {text}")
            except:
                print(f"  Raw: {decrypted[:64].hex()}")
        except Exception as e:
            print(f"  GetDeviceInfo failed: {e}")

    try:
        usb.util.release_interface(new_dev, 0)
    except:
        pass

    print(f"\n{'SUCCESS' if result else 'FAILED'}")
    sys.exit(0 if result else 1)


if __name__ == '__main__':
    main()
