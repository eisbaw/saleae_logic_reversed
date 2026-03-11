#!/usr/bin/env python3
"""Decode the EP1 initialization sequence from usb.logic8pro.pcap.

Decrypts all EP1 command/response pairs using the LFSR-based XOR
obfuscation scheme. The encryption algorithm matches
crates/saleae-proto/src/obfuscation.rs exactly.

KEY DISCOVERY: Reseed packets are ALWAYS encrypted with the initial/default
LFSR seed (0x354b248e), NOT the current rolling seed. This allows the device
to decode reseeds regardless of the current LFSR state. Normal commands use
the rolling seed that advances after each write/read.

Usage:
    python re/scripts/decode-pcap-init.py

Requires: tshark (provided by nix-shell)
"""

import subprocess
import sys
from dataclasses import dataclass

# --- LFSR and encryption primitives ---

CONTROL_MASK = 0xD7
RESEED_FLAG = 0x20
FIRMWARE_FLAG = 0x08


def lfsr_step(state: int) -> int:
    """One step of the 32-bit LFSR (taps at bits 0, 1, 21, 31)."""
    feedback = (state ^ (state >> 1) ^ (state >> 21) ^ (state >> 31)) & 1
    return ((state >> 1) | (feedback << 31)) & 0xFFFFFFFF


def advance_seed(state: int) -> int:
    """Advance LFSR by (state & 0x1F) + 0x23 steps."""
    n = (state & 0x1F) + 0x23
    for _ in range(n):
        state = lfsr_step(state)
    return state


def seed_byte(seed: int, i: int) -> int:
    """Extract byte i (mod 4) from seed in little-endian order."""
    return (seed >> ((i % 4) * 8)) & 0xFF


def try_decode_reseed(wire_bytes: bytes, initial_seed: int):
    """Try to decode an OUT packet as a reseed using the initial seed.

    Reseed packets are always encrypted with the initial/default LFSR seed,
    not the current rolling seed. This is because the device needs a fixed
    key to decode the new seed value.

    Returns (is_reseed, new_seed_value) or (False, 0) if not a reseed.
    """
    if len(wire_bytes) != 5:
        return False, 0

    control = wire_bytes[0]
    sb0_masked = seed_byte(initial_seed, 0) & CONTROL_MASK
    recovered_control = control ^ sb0_masked
    is_reseed = bool(recovered_control & RESEED_FLAG)

    if not is_reseed:
        return False, 0

    seed_bytes_raw = bytearray(4)
    for i in range(4):
        seed_bytes_raw[i] = wire_bytes[1 + i] ^ seed_byte(initial_seed, i + 1)
    new_seed_val = int.from_bytes(seed_bytes_raw, 'little')
    return True, new_seed_val


def decrypt_command(wire_bytes: bytes, seed: int):
    """Decrypt an EP1 OUT command using the current rolling seed.

    Returns (opcode, payload, new_seed).
    """
    if len(wire_bytes) < 2:
        return None, wire_bytes, seed

    # Recover opcode from wire[1] ^ sb1
    opcode = wire_bytes[1] ^ seed_byte(seed, 1)

    # Decrypt payload (starts at wire[2], XOR with sb2, sb3, sb0, sb1, ...)
    payload = bytearray()
    for i in range(2, len(wire_bytes)):
        payload.append(wire_bytes[i] ^ seed_byte(seed, i))

    # Advance seed after write
    new_seed = advance_seed(seed)
    return opcode, bytes(payload), new_seed


def decrypt_response(wire_bytes: bytes, seed: int):
    """Decrypt an EP1 IN response.

    Returns (decrypted_data, new_seed).
    """
    data = bytearray(len(wire_bytes))
    for i in range(len(wire_bytes)):
        data[i] = wire_bytes[i] ^ seed_byte(seed, i)

    new_seed = advance_seed(seed)
    return bytes(data), new_seed


# --- Known opcodes ---

OPCODES = {
    0x01: "StartSampling",
    0x02: "StopStream",
    0x07: "ReadVersion",
    0x55: "WriteEeprom",
    0x7D: "Ping",
    0x7E: "ResetFpga",
    0x7F: "WriteConfigData",
    0x80: "WriteRegisters",
    0x81: "ReadRegisters",
    0x82: "ResetAndReEnumerate",
    0x86: "StopDataPath",
    0x87: "WriteI2C",
    0x88: "ReadI2C",
    0x89: "Device2_1",
    0x8A: "CmdAssertTest",
    0x8B: "GetDeviceInfo",
}


def opcode_name(op: int) -> str:
    return OPCODES.get(op, f"Unknown_{op:#04x}")


def format_hex(data: bytes, max_len: int = 32) -> str:
    s = data.hex()
    if len(data) > max_len:
        return s[:max_len * 2] + f"... ({len(data)} bytes total)"
    return s


def format_payload_detail(opcode: int, payload: bytes) -> str:
    """Provide human-readable interpretation of known command payloads."""
    if opcode == 0x7D:  # Ping
        if payload:
            return f"test_byte=0x{payload[0]:02x}"
        return ""
    elif opcode == 0x8B:  # GetDeviceInfo
        return "(no payload)"
    elif opcode == 0x07:  # ReadVersion
        if len(payload) >= 6:
            return (f"params=[{payload[0]:#04x}, {payload[1]:#04x}, "
                    f"{payload[2]:#04x}, {payload[3]:#04x}, "
                    f"{payload[4]:#04x}, {payload[5]:#04x}]")
        return f"params={format_hex(payload)}"
    elif opcode == 0x7E:  # ResetFpga
        return "(no payload)"
    elif opcode == 0x7F:  # WriteConfigData
        if len(payload) >= 2:
            chunk_len = payload[0] | (payload[1] << 8)
            return f"chunk_len={chunk_len}, data={len(payload) - 2} bytes"
        return f"raw={format_hex(payload)}"
    elif opcode == 0x80:  # WriteRegisters
        if payload and len(payload) >= 1:
            count = payload[0]
            pairs = []
            for i in range(count):
                if 1 + i * 2 + 1 < len(payload):
                    addr = payload[1 + i * 2]
                    val = payload[1 + i * 2 + 1]
                    pairs.append(f"r{addr:#04x}={val:#04x}")
            return f"count={count} [{', '.join(pairs)}]"
        return f"raw={format_hex(payload)}"
    elif opcode == 0x81:  # ReadRegisters
        if payload and len(payload) >= 1:
            count = payload[0]
            addrs = [f"{payload[1+i]:#04x}" for i in range(count)
                     if 1 + i < len(payload)]
            return f"count={count} addrs=[{', '.join(addrs)}]"
        return f"raw={format_hex(payload)}"
    elif opcode == 0x87:  # WriteI2C
        if len(payload) >= 4:
            addr = payload[0]
            reg = payload[1]
            count = payload[2] | (payload[3] << 8)
            data = payload[4:]
            return f"addr=0x{addr:02x} reg=0x{reg:02x} count={count} data={format_hex(data)}"
        return f"params={format_hex(payload)}"
    elif opcode == 0x88:  # ReadI2C
        if len(payload) >= 4:
            addr = payload[0]
            reg = payload[1]
            count = payload[2] | (payload[3] << 8)
            return f"addr=0x{addr:02x} reg=0x{reg:02x} count={count}"
        return f"params={format_hex(payload)}" if payload else "(no payload)"
    elif opcode == 0x89:  # Device2_1
        return "(no payload)"
    return f"data={format_hex(payload)}" if payload else "(no payload)"


def format_response_detail(opcode: int, data: bytes) -> str:
    """Provide human-readable interpretation of known response payloads."""
    if opcode == 0x7D:  # Ping
        if data:
            return f"response=0x{data[0]:02x} ({'OK: ~0x55' if data[0] == 0xAA else 'UNEXPECTED'})"
        return ""
    elif opcode == 0x8B:  # GetDeviceInfo
        try:
            text = data.rstrip(b'\x00').decode('ascii', errors='replace')
            return f'"{text}" ({len(data)} bytes)'
        except Exception:
            return format_hex(data)
    elif opcode == 0x07:  # ReadVersion
        try:
            text = data.decode('ascii', errors='replace')
            return f'"{text}" ({len(data)} bytes)'
        except Exception:
            return format_hex(data)
    elif opcode == 0x7E:  # ResetFpga
        if data and data[0] == 0x00:
            return "0x00 (success)"
        return format_hex(data)
    elif opcode == 0x7F:  # WriteConfigData
        if data and data[0] == 0x00:
            return "0x00 (ACK)"
        return format_hex(data)
    elif opcode == 0x80:  # WriteRegisters
        if data and data[0] == 0x00:
            return "0x00 (ACK)"
        return format_hex(data)
    elif opcode == 0x81:  # ReadRegisters
        vals = [f"0x{b:02x}" for b in data]
        return f"[{', '.join(vals)}]"
    elif opcode == 0x88:  # ReadI2C
        return format_hex(data)
    elif opcode == 0x89:  # Device2_1
        if data and data[0] == 0x00:
            return "0x00 (ACK)"
        return format_hex(data)
    return format_hex(data)


# --- Pcap extraction ---

PCAP_PATH = "~/topics/saleae_logic_8_pro/data-002/usb.logic8pro.pcap"

# Initial LFSR state -- this is the default/fixed seed used for reseed packet
# encryption. All reseed packets are encrypted with this value, not the
# current rolling seed. Validated against pcap frames 15, 1627, 1639, 1651.
INITIAL_SEED = 0x354B248E


@dataclass
class PcapFrame:
    frame_num: int
    timestamp: float
    endpoint: int  # 0x01 = OUT, 0x81 = IN
    data_len: int
    data: bytes


def extract_frames():
    """Extract EP1 frames from pcap using tshark."""
    import os
    pcap = os.path.expanduser(PCAP_PATH)

    cmd = [
        "tshark", "-r", pcap,
        "-Y", "(usb.endpoint_address == 0x01 || usb.endpoint_address == 0x81) && usb.data_len > 0",
        "-T", "fields",
        "-e", "frame.number",
        "-e", "frame.time_relative",
        "-e", "usb.endpoint_address",
        "-e", "usb.data_len",
        "-e", "usb.capdata",
    ]

    result = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    if result.returncode != 0:
        print(f"tshark failed: {result.stderr}", file=sys.stderr)
        sys.exit(1)

    frames = []
    for line in result.stdout.strip().split('\n'):
        if not line.strip():
            continue
        parts = line.split('\t')
        if len(parts) < 5:
            continue
        frame_num = int(parts[0])
        timestamp = float(parts[1])
        endpoint = int(parts[2], 16)
        data_len = int(parts[3])
        data = bytes.fromhex(parts[4]) if parts[4] else b''
        frames.append(PcapFrame(frame_num, timestamp, endpoint, data_len, data))

    return frames


def main():
    print("Extracting EP1 frames from pcap...")
    frames = extract_frames()
    print(f"Found {len(frames)} EP1 frames with data\n")

    seed = INITIAL_SEED
    last_opcode = None
    fpga_chunks = 0
    fpga_bytes = 0
    in_fpga_upload = False
    fpga_start_frame = None
    fpga_end_frame = None
    seq_num = 0
    reseed_count = 0
    unknown_count = 0

    print("=" * 100)
    print("INITIALIZATION SEQUENCE (Logic Pro 8, from usb.logic8pro.pcap)")
    print("=" * 100)
    print(f"{'Seq':>4} {'Frame':>6} {'Time':>12} {'Dir':>4} {'Len':>5}  "
          f"{'Decrypted':60}")
    print("-" * 100)

    i = 0
    while i < len(frames):
        f = frames[i]

        if f.endpoint == 0x01:  # EP1 OUT (command)
            # First check if this is a reseed (encrypted with initial seed)
            is_reseed, reseed_val = try_decode_reseed(f.data, INITIAL_SEED)

            if is_reseed:
                # End FPGA upload summary if needed
                if in_fpga_upload:
                    seq_num += 1
                    print(f"{seq_num:4} {fpga_start_frame:6}-{fpga_end_frame:<6} "
                          f"{'':>6}  OUT       "
                          f"WriteConfigData (0x7F): {fpga_chunks} chunks, "
                          f"{fpga_bytes} bytes FPGA bitstream")
                    in_fpga_upload = False

                reseed_count += 1
                seq_num += 1
                print(f"{seq_num:4} {f.frame_num:6} {f.timestamp:12.6f}  OUT {f.data_len:5}  "
                      f"RESEED #{reseed_count} -> 0x{reseed_val:08x}")
                seed = reseed_val
                last_opcode = None
                i += 1
                continue

            # Not a reseed -- decode as normal command
            opcode, payload, new_seed = decrypt_command(f.data, seed)

            # Check if this is a WriteConfigData (FPGA bitstream)
            if opcode == 0x7F and f.data_len >= 100:
                if not in_fpga_upload:
                    in_fpga_upload = True
                    fpga_start_frame = f.frame_num
                    fpga_chunks = 0
                    fpga_bytes = 0
                fpga_chunks += 1
                if len(payload) >= 2:
                    chunk_len = payload[0] | (payload[1] << 8)
                    fpga_bytes += chunk_len
                fpga_end_frame = f.frame_num
                seed = new_seed

                # Read the ACK response
                i += 1
                if i < len(frames) and frames[i].endpoint == 0x81:
                    ack_data, seed = decrypt_response(frames[i].data, seed)
                    last_opcode = 0x7F
                    i += 1
                continue

            # End of FPGA upload - print summary
            if in_fpga_upload:
                seq_num += 1
                print(f"{seq_num:4} {fpga_start_frame:6}-{fpga_end_frame:<6} "
                      f"{'':>6}  OUT       "
                      f"WriteConfigData (0x7F): {fpga_chunks} chunks, "
                      f"{fpga_bytes} bytes FPGA bitstream")
                in_fpga_upload = False

            seq_num += 1
            detail = format_payload_detail(opcode, payload)
            name = opcode_name(opcode)
            if name.startswith("Unknown"):
                unknown_count += 1
            print(f"{seq_num:4} {f.frame_num:6} {f.timestamp:12.6f}  OUT {f.data_len:5}  "
                  f"{name} (0x{opcode:02x}) {detail}")

            seed = new_seed
            last_opcode = opcode

        elif f.endpoint == 0x81:  # EP1 IN (response)
            data, new_seed = decrypt_response(f.data, seed)
            seed = new_seed

            if last_opcode is not None:
                detail = format_response_detail(last_opcode, data)
                name = opcode_name(last_opcode)
                seq_num += 1
                print(f"{seq_num:4} {f.frame_num:6} {f.timestamp:12.6f}   IN {f.data_len:5}  "
                      f"  -> {name} resp: {detail}")
            else:
                seq_num += 1
                print(f"{seq_num:4} {f.frame_num:6} {f.timestamp:12.6f}   IN {f.data_len:5}  "
                      f"  -> {format_hex(data)}")

            last_opcode = None

        i += 1

    # Final FPGA summary if still in upload
    if in_fpga_upload:
        seq_num += 1
        print(f"{seq_num:4} {fpga_start_frame:6}-{fpga_end_frame:<6} "
              f"{'':>6}  OUT       "
              f"WriteConfigData (0x7F): {fpga_chunks} chunks, "
              f"{fpga_bytes} bytes FPGA bitstream")

    print("\n" + "=" * 100)
    print(f"Total decoded: {seq_num} operations")
    print(f"Reseeds: {reseed_count}")
    print(f"Unknown opcodes: {unknown_count}")
    print(f"Final LFSR seed: 0x{seed:08x}")


if __name__ == "__main__":
    main()
