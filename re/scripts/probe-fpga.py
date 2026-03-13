#!/usr/bin/env python3
"""Upload FX3 firmware, program FPGA, and probe post-init registers.

Replicates Phases 1-4 of the Saleae init sequence from pcap.
Must run against device in bootloader mode (bcdDevice=1.00).

Usage: sudo python3 probe-fpga.py [firmware.img] [bitstream.bin]
  If bitstream not specified, tries all SLX9 candidates from /tmp/bitstream-*.bin
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


def write_registers(dev, seed, reg_vals):
    """Write register addr/value pairs via opcode 0x80.

    Pro format: [0x80] [count] [addr0] [val0] [addr1] [val1] ...
    Response: 1 byte ACK per pair (Pro devices)
    """
    count = len(reg_vals)
    payload = [count]
    for addr, val in reg_vals:
        payload.extend([addr, val])
    resp, seed = send_cmd(dev, seed, 0x80, payload)
    return resp, seed


def read_registers(dev, seed, addrs):
    """Read register values via opcode 0x81.

    Format: [0x81] [count] [addr0] [addr1] ...
    Response: [val0] [val1] ...
    """
    payload = [len(addrs)] + list(addrs)
    resp, seed = send_cmd(dev, seed, 0x81, payload)
    return resp, seed


def program_fpga(dev, seed, bitstream_data):
    """Upload FPGA bitstream via WriteConfigurationData (0x7F).

    Pro USB 3.0 format: [0x7F] [len_lo] [len_hi] [data...]
    Max data per chunk: 1020 bytes (USB 3.0) - matching pcap
    Response: 1 byte 0x00 ACK per chunk
    """
    chunk_size = 1020  # Pro USB 3.0
    total = len(bitstream_data)
    chunks = 0
    offset = 0

    while offset < total:
        end = min(offset + chunk_size, total)
        chunk = bitstream_data[offset:end]
        chunk_len = len(chunk)

        # Build payload: [len_lo, len_hi, data...]
        payload = [chunk_len & 0xFF, (chunk_len >> 8) & 0xFF] + list(chunk)

        resp, seed = send_cmd(dev, seed, 0x7F, payload)
        if resp[0] != 0x00:
            print(f"  Chunk {chunks} at offset {offset}: unexpected response {resp[0]:#04x}")
            return None, seed

        chunks += 1
        offset = end

        if chunks % 50 == 0:
            print(f"  {offset}/{total} bytes ({100*offset//total}%)")

    print(f"  Uploaded {total} bytes in {chunks} chunks")
    return True, seed


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
    bitstream_path = sys.argv[2] if len(sys.argv) > 2 else None

    # Find device
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if dev is None:
        print("No Saleae device found")
        sys.exit(1)
    if dev.bcdDevice != 0x0100:
        print(f"ERROR: Device not in bootloader mode (bcdDevice={dev.bcdDevice:#06x})")
        print("Need power cycle.")
        sys.exit(1)

    # Upload firmware
    try:
        if dev.is_kernel_driver_active(0):
            dev.detach_kernel_driver(0)
    except:
        pass
    t0 = time.time()
    upload_firmware(dev, fw_path)
    del dev
    for _ in range(50):
        time.sleep(0.2)
        dev = usb.core.find(idVendor=VID, idProduct=PID)
        if dev is not None and dev.bcdDevice == 0x0000:
            break
    else:
        print("Re-enumeration timeout")
        sys.exit(1)
    print(f"Firmware loaded in {time.time()-t0:.2f}s (speed={dev.speed})")

    # USB setup
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

    # ======================================================================
    # Phase 1: Initial Handshake
    # ======================================================================
    print(f"\n=== Phase 1: Handshake ===")
    new_seed_bytes = list(os.urandom(4))
    if all(b == 0 for b in new_seed_bytes):
        new_seed_bytes[0] = 1
    new_seed = struct.unpack('<I', bytes(new_seed_bytes))[0]
    reseed_wire = encrypt_reseed(INITIAL_SEED, new_seed_bytes)
    dev.write(EP1_OUT, reseed_wire, timeout=TIMEOUT_MS)
    seed = new_seed

    resp, seed = send_cmd(dev, seed, 0x7D, [0x55])
    assert resp[0] == 0xAA, f"Ping failed: {resp[0]:#04x}"
    print(f"Ping OK")

    resp, seed = send_cmd(dev, seed, 0x8B, [])
    print(f"DeviceInfo: {resp.decode('ascii', errors='replace').rstrip(chr(0))}")

    # ReadVersion: EEPROM offset 0x0008, 8 bytes (device ID)
    resp, seed = send_cmd(dev, seed, 0x07, [0x33, 0x81, 0x08, 0x00, 0x08, 0x00])
    print(f"DeviceID: {resp.hex()}")

    # ReadVersion: EEPROM offset 0x0020, 6 bytes (hw revision)
    resp, seed = send_cmd(dev, seed, 0x07, [0x33, 0x81, 0x20, 0x00, 0x06, 0x00])
    rev_text = resp.decode('ascii', errors='replace').rstrip('\x00')
    print(f"HW Rev: {resp.hex()} -> \"{rev_text}\"")

    # ResetFpga
    resp, seed = send_cmd(dev, seed, 0x7E, [])
    assert resp[0] == 0x00, f"ResetFpga failed: {resp[0]:#04x}"
    print(f"ResetFpga OK")

    # ======================================================================
    # Phase 2: FPGA Bitstream Upload
    # ======================================================================
    print(f"\n=== Phase 2: FPGA Bitstream ===")

    if bitstream_path is None:
        # Try to find SLX9 bitstreams - our device is Rev 1, Xilinx Spartan-6
        import glob
        candidates = sorted(glob.glob("/tmp/bitstream-spartan6-slx9-*.bin"))
        if not candidates:
            print("No bitstream specified and no candidates in /tmp/")
            print("Copy bitstreams to /tmp/: cp firmware/bitstream-spartan6-slx9-*.bin /tmp/")
            # Still useful - we got through Phase 1
            usb.util.release_interface(dev, 0)
            sys.exit(0)
        print(f"Found {len(candidates)} SLX9 candidates")
        bitstream_path = candidates[0]
        print(f"Trying first: {bitstream_path}")

    with open(bitstream_path, 'rb') as f:
        bitstream = f.read()
    print(f"Bitstream: {bitstream_path} ({len(bitstream)} bytes)")

    t_fpga = time.time()
    result, seed = program_fpga(dev, seed, bitstream)
    if result is None:
        print("FPGA upload FAILED")
        usb.util.release_interface(dev, 0)
        sys.exit(1)
    print(f"FPGA upload took {time.time()-t_fpga:.2f}s")

    # ======================================================================
    # Phase 3: FPGA Verification
    # ======================================================================
    print(f"\n=== Phase 3: FPGA Verification ===")

    # From pcap Phase 3a: Write 0xAA to r0x7F, read back to verify FPGA responsive
    resp, seed = write_registers(dev, seed, [(0x7F, 0xAA)])
    print(f"WriteRegisters r0x7F=0xAA: ack={resp.hex()}")

    resp, seed = write_registers(dev, seed, [(0x7F, 0xAA)])
    print(f"WriteRegisters r0x7F=0xAA: ack={resp.hex()}")

    resp, seed = read_registers(dev, seed, [0x7F])
    readback = resp[0] if resp else None
    print(f"ReadRegisters r0x7F: {resp.hex()} (expected 0xAA)")

    if readback == 0xAA:
        print(">>> FPGA IS RESPONSIVE! <<<")
    else:
        print(f"FPGA readback mismatch: got {readback:#04x if readback else 'None'}, expected 0xAA")
        print("Wrong bitstream or programming failed.")
        # Try reading a few more registers for debugging
        for reg in [0x00, 0x01, 0x40, 0x4A, 0x7F]:
            try:
                resp, seed = read_registers(dev, seed, [reg])
                print(f"  r{reg:#04x} = {resp.hex()}")
            except Exception as e:
                print(f"  r{reg:#04x}: {e}")
        usb.util.release_interface(dev, 0)
        sys.exit(1)

    # ======================================================================
    # Phase 3b: ADC Init (from pcap)
    # ======================================================================
    print(f"\n=== Phase 3b: ADC Init ===")

    # From pcap: 4-register writes via [r0x03, r0x16, r0x04, r0x05] pattern
    # r0x03 = ADC register address, r0x16 = bank (always 0), r0x04 = value, r0x05 = 0
    adc_writes = [
        # Bank 0x08: reset sequence
        (0x08, 0x03), (0x08, 0x00), (0x08, 0x01), (0x08, 0x00),
        # Bank 0x0d: value 0x00
        (0x0d, 0x00),
        # Bank 0x15: value 0x21
        (0x15, 0x21),
        # Bank 0x21: value 0xc2
        (0x21, 0xc2),
        # Bank 0x05: value 0x0f
        (0x05, 0x0f),
        # Bank 0x04: value 0x0f
        (0x04, 0x0f),
        # Bank 0x10: value 0x00
        (0x10, 0x00),
        # Bank 0x18: value 0x04
        (0x18, 0x04),
    ]

    for bank_addr, value in adc_writes:
        # Each ADC write is a 4-register burst: [r0x03=addr, r0x16=0, r0x04=val, r0x05=0]
        resp, seed = write_registers(dev, seed, [
            (0x03, bank_addr),
            (0x16, 0x00),
            (0x04, value),
            (0x05, 0x00),
        ])
        print(f"  ADC[{bank_addr:#04x}] = {value:#04x}: ack={resp.hex()}")

    # r0x15=0x00 (bank power off)
    resp, seed = write_registers(dev, seed, [(0x15, 0x00)])
    print(f"  r0x15=0x00 (bank power off): ack={resp.hex()}")

    # Bank 0x08: value 0x01 (enable)
    resp, seed = write_registers(dev, seed, [
        (0x03, 0x08),
        (0x16, 0x00),
        (0x04, 0x01),
        (0x05, 0x00),
    ])
    print(f"  ADC[0x08]=0x01 (enable): ack={resp.hex()}")

    # ======================================================================
    # Phase 3c: JESD Link (from pcap)
    # ======================================================================
    print(f"\n=== Phase 3c: JESD Link Establishment ===")

    # Power-down sequence: r0x00 transitions 0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00
    for val in [0x1E, 0x1C, 0x18, 0x08, 0x00]:
        resp, seed = write_registers(dev, seed, [(0x00, val)])
        print(f"  r0x00={val:#04x}: ack={resp.hex()}")

    # Poll r0x40 for JESD link status
    time.sleep(0.05)  # 50ms delay from pcap
    resp, seed = read_registers(dev, seed, [0x40])
    print(f"  r0x40 = {resp.hex()} (JESD status, want 0xE0)")

    resp, seed = read_registers(dev, seed, [0x4A])
    print(f"  r0x4A = {resp.hex()}")

    if resp[0] != 0xE0:
        # Retry with second power cycle (from pcap)
        print("  JESD not locked, retrying power cycle...")
        time.sleep(0.05)
        for val in [0x1E, 0x1C, 0x18, 0x08, 0x00]:
            resp, seed = write_registers(dev, seed, [(0x00, val)])

        time.sleep(0.05)
        resp, seed = read_registers(dev, seed, [0x40])
        print(f"  r0x40 = {resp.hex()} (after retry)")

        resp, seed = read_registers(dev, seed, [0x4A])
        print(f"  r0x4A = {resp.hex()}")

    # Read more status registers
    resp, seed = read_registers(dev, seed, [0x19])
    print(f"  r0x19 = {resp.hex()}")

    resp, seed = read_registers(dev, seed, [0x40])
    print(f"  r0x40 = {resp.hex()} (confirm)")

    # ======================================================================
    # Phase 3d: Calibration coefficients from EEPROM
    # ======================================================================
    print(f"\n=== Phase 3d: Load Calibration ===")

    # Read 16 calibration bytes from EEPROM offset 0x0010
    resp, seed = send_cmd(dev, seed, 0x07, [0x33, 0x81, 0x10, 0x00, 0x10, 0x00])
    cal_bytes = resp[:16]
    print(f"  Calibration from EEPROM: {cal_bytes.hex()}")

    # Write each calibration byte to FPGA register 0x17
    for i, b in enumerate(cal_bytes):
        resp, seed = write_registers(dev, seed, [(0x17, b)])
    print(f"  Wrote {len(cal_bytes)} calibration coefficients to r0x17")

    # ======================================================================
    # Status summary
    # ======================================================================
    print(f"\n=== Register Dump ===")
    for reg in [0x00, 0x03, 0x04, 0x05, 0x15, 0x16, 0x17, 0x19, 0x40, 0x4A, 0x7F]:
        try:
            resp, seed = read_registers(dev, seed, [reg])
            print(f"  r{reg:#04x} = {resp.hex()}")
        except Exception as e:
            print(f"  r{reg:#04x}: {e}")

    total = time.time() - t0
    print(f"\nTotal time: {total:.1f}s")

    usb.util.release_interface(dev, 0)
    print("Done.")


if __name__ == '__main__':
    main()
