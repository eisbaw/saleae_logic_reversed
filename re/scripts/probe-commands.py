#!/usr/bin/env python3
"""Upload FX3 firmware and probe all EP1 commands in one shot.

This script MUST run against a device in bootloader mode (bcdDevice=1.00).
It uploads firmware, then immediately exercises the complete Phase 1 init
sequence from the pcap, plus EEPROM reads.

The FX3 command processing thread only starts once at firmware init and
dies after USB disconnect. There is no way to restart it without a fresh
firmware upload, which requires a power cycle to get back to bootloader.

Usage: sudo python3 probe-commands.py [firmware.img]
"""
import sys
import struct
import os
import time
import usb.core
import usb.util

VID = 0x21a9
PID = 0x1005
EP1_OUT = 0x01
EP1_IN = 0x81
TIMEOUT_MS = 5000

CONTROL_MASK = 0xD7
RESEED_FLAG = 0x20
PING_OPCODE = 0x7D
PING_TEST_BYTE = 0x55
PING_EXPECTED = 0xAA
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
    if len(data) < 4 or data[0:2] != b'CY':
        raise ValueError("Not a valid FX3 image")
    sections = []
    offset = 4
    while offset + 8 <= len(data):
        length = struct.unpack_from('<I', data, offset)[0]
        address = struct.unpack_from('<I', data, offset + 4)[0]
        if length == 0:
            return sections, address
        section_data = data[offset + 8: offset + 8 + length * 4]
        sections.append((address, section_data))
        offset += 8 + length * 4
    raise ValueError("No entry point found")


def upload_firmware(dev, fw_path):
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
    w_value = entry_address & 0xFFFF
    w_index = (entry_address >> 16) & 0xFFFF
    dev.ctrl_transfer(0x40, 0xA0, w_value, w_index, [], timeout=5000)
    return entry_address


def send_cmd(dev, seed, opcode, payload, resp_size=512):
    """Send encrypted command, read and decrypt response."""
    cmd_wire, seed = encrypt_command(seed, opcode, payload)
    dev.write(EP1_OUT, cmd_wire, timeout=TIMEOUT_MS)
    resp = dev.read(EP1_IN, resp_size, timeout=TIMEOUT_MS)
    decrypted, seed = decrypt_response(seed, bytes(resp))
    return decrypted, seed


def send_cmd_no_response(dev, seed, opcode, payload):
    """Send encrypted command, don't read response."""
    cmd_wire, seed = encrypt_command(seed, opcode, payload)
    dev.write(EP1_OUT, cmd_wire, timeout=TIMEOUT_MS)
    return seed


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
    fw_path = sys.argv[1] if len(sys.argv) > 1 else "/tmp/fw-fx3-pro8.img"

    # Step 0: Find device, require bootloader mode
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if dev is None:
        print("No Saleae device found")
        sys.exit(1)

    if dev.bcdDevice != 0x0100:
        print(f"ERROR: Device has firmware loaded (bcdDevice={dev.bcdDevice:#06x})")
        print("Need a power cycle to get back to bootloader mode.")
        print("The FX3 command thread only starts once at firmware init.")
        sys.exit(1)

    print(f"Device in bootloader mode (bcdDevice=0x0100)")

    # Step 1: Upload firmware
    try:
        if dev.is_kernel_driver_active(0):
            dev.detach_kernel_driver(0)
    except:
        pass

    t0 = time.time()
    entry = upload_firmware(dev, fw_path)
    t_upload = time.time() - t0
    print(f"Firmware uploaded in {t_upload:.2f}s (entry={entry:#010x})")
    del dev

    # Step 2: Wait for re-enumeration
    for _ in range(50):
        time.sleep(0.2)
        dev = usb.core.find(idVendor=VID, idProduct=PID)
        if dev is not None and dev.bcdDevice == 0x0000:
            break
    else:
        print("ERROR: Re-enumeration timeout")
        sys.exit(1)

    t_enum = time.time() - t0
    print(f"Re-enumerated in {t_enum:.2f}s (speed={dev.speed})")

    # Step 3: USB setup (matching pcap: SET_CONFIG + CLEAR_HALT EP1_IN)
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

    t_setup = time.time() - t0
    print(f"USB setup done at {t_setup:.2f}s")

    # ======================================================================
    # Phase 1: Initial Handshake (pcap frames 15-36)
    # ======================================================================
    print(f"\n{'='*60}")
    print(f"Phase 1: Initial Handshake")
    print(f"{'='*60}")

    # Reseed
    new_seed_bytes = list(os.urandom(4))
    if all(b == 0 for b in new_seed_bytes):
        new_seed_bytes[0] = 1
    new_seed = struct.unpack('<I', bytes(new_seed_bytes))[0]
    reseed_wire = encrypt_reseed(INITIAL_SEED, new_seed_bytes)
    dev.write(EP1_OUT, reseed_wire, timeout=TIMEOUT_MS)
    seed = new_seed
    print(f"  [1] Reseed -> {seed:#010x}")

    # Ping
    resp, seed = send_cmd(dev, seed, PING_OPCODE, [PING_TEST_BYTE])
    ok = resp[0] == PING_EXPECTED
    print(f"  [2] Ping: {'OK' if ok else 'FAIL'} (got {resp[0]:#04x})")
    if not ok:
        sys.exit(1)

    # GetDeviceInfo (0x8B)
    resp, seed = send_cmd(dev, seed, 0x8B, [])
    info = resp.decode('ascii', errors='replace').rstrip('\x00')
    print(f"  [3] GetDeviceInfo: {info}")

    # ReadVersion #1: EEPROM offset 0x0008, 8 bytes
    # pcap params: [0x33, 0x81, 0x08, 0x00, 0x08, 0x00]
    print(f"  [4] ReadVersion (EEPROM 0x0008, 8 bytes):")
    try:
        resp, seed = send_cmd(dev, seed, 0x07, [0x33, 0x81, 0x08, 0x00, 0x08, 0x00])
        hexdump(resp, prefix="       ")
    except Exception as e:
        print(f"       FAILED: {e}")

    # ReadVersion #2: EEPROM offset 0x0020, 6 bytes -> "REV."
    # pcap params: [0x33, 0x81, 0x20, 0x00, 0x06, 0x00]
    print(f"  [5] ReadVersion (EEPROM 0x0020, 6 bytes):")
    try:
        resp, seed = send_cmd(dev, seed, 0x07, [0x33, 0x81, 0x20, 0x00, 0x06, 0x00])
        hexdump(resp, prefix="       ")
        text = resp.decode('ascii', errors='replace').rstrip('\x00')
        if text:
            print(f"       -> \"{text}\"")
    except Exception as e:
        print(f"       FAILED: {e}")

    # ResetFpga (0x7E)
    print(f"  [6] ResetFpga (0x7E):")
    try:
        resp, seed = send_cmd(dev, seed, 0x7E, [])
        print(f"       Response: {resp.hex()} {'(OK)' if resp[0] == 0 else '(unexpected)'}")
    except Exception as e:
        print(f"       FAILED: {e}")

    # ======================================================================
    # Bonus: EEPROM dump
    # ======================================================================
    print(f"\n{'='*60}")
    print(f"EEPROM Dump (via ReadVersion opcode 0x07)")
    print(f"{'='*60}")

    eeprom = bytearray()
    for off in range(0, 512, 32):
        try:
            payload = [0x33, 0x81, off & 0xFF, (off >> 8) & 0xFF, 0x20, 0x00]
            resp, seed = send_cmd(dev, seed, 0x07, payload)
            eeprom.extend(resp)
        except Exception as e:
            print(f"  Read offset {off:#06x}: FAILED - {e}")
            break

    if eeprom:
        hexdump(eeprom)
        with open("/tmp/saleae-eeprom.bin", 'wb') as f:
            f.write(eeprom)
        print(f"\n  Saved {len(eeprom)} bytes to /tmp/saleae-eeprom.bin")

        # Find ASCII strings
        print(f"\n  ASCII strings found:")
        i = 0
        while i < len(eeprom):
            if 32 <= eeprom[i] < 127:
                end = i
                while end < len(eeprom) and 32 <= eeprom[end] < 127:
                    end += 1
                if end - i >= 4:
                    print(f"    {i:#06x}: \"{eeprom[i:end].decode('ascii')}\"")
                i = end
            else:
                i += 1

    # ======================================================================
    # Bonus: Try additional commands
    # ======================================================================
    print(f"\n{'='*60}")
    print(f"Additional Commands")
    print(f"{'='*60}")

    # WriteRegisters (0x80) and ReadRegisters (0x81) need FPGA loaded
    # Let's try them anyway to see what happens

    # ReadRegisters (0x81): read register 0x7F (FPGA magic test register)
    print(f"  ReadRegisters (0x81) r0x7F:")
    try:
        resp, seed = send_cmd(dev, seed, 0x81, [0x01, 0x7F])
        print(f"    Response: {resp.hex()}")
    except Exception as e:
        print(f"    FAILED: {e}")

    # COMMAND_START_READ (0x01) - just to see response format
    print(f"  StartRead (0x01):")
    try:
        resp, seed = send_cmd(dev, seed, 0x01, [])
        print(f"    Response: {resp.hex()}")
    except Exception as e:
        print(f"    FAILED: {e}")

    # COMMAND_STOP_STREAM (0x02)
    print(f"  StopStream (0x02):")
    try:
        resp, seed = send_cmd(dev, seed, 0x02, [])
        print(f"    Response: {resp.hex()}")
    except Exception as e:
        print(f"    FAILED: {e}")

    # GetDeviceInfo again to verify seed is still in sync
    print(f"  GetDeviceInfo (verify seed sync):")
    try:
        resp, seed = send_cmd(dev, seed, 0x8B, [])
        text = resp.decode('ascii', errors='replace').rstrip('\x00')
        print(f"    {text}")
        print(f"    Seed still in sync!")
    except Exception as e:
        print(f"    FAILED (seed out of sync?): {e}")

    # ======================================================================
    # Timing test: How long until EP1 dies?
    # ======================================================================
    print(f"\n{'='*60}")
    print(f"Liveness Test (ping every 5s until failure)")
    print(f"{'='*60}")

    ping_count = 0
    start_time = time.time()
    while True:
        try:
            test_byte = (ping_count + 1) & 0xFF
            if test_byte == 0:
                test_byte = 1
            resp, seed = send_cmd(dev, seed, PING_OPCODE, [test_byte])
            expected = (~test_byte) & 0xFF
            elapsed = time.time() - start_time
            if resp[0] == expected:
                ping_count += 1
                print(f"  Ping #{ping_count} at {elapsed:.1f}s: OK")
            else:
                print(f"  Ping #{ping_count+1} at {elapsed:.1f}s: WRONG (got {resp[0]:#04x}, expected {expected:#04x})")
                break
        except Exception as e:
            elapsed = time.time() - start_time
            print(f"  Ping #{ping_count+1} at {elapsed:.1f}s: FAILED - {e}")
            break

        # Wait 5 seconds between pings
        time.sleep(5)

        # Stop after 2 minutes
        if time.time() - start_time > 120:
            print(f"  Stopped after 2 minutes ({ping_count} successful pings)")
            break

    usb.util.release_interface(dev, 0)
    total = time.time() - t0
    print(f"\nTotal time: {total:.1f}s")
    print(f"Done.")


if __name__ == '__main__':
    main()
