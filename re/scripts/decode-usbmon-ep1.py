#!/usr/bin/env python3
"""Decode EP1 traffic from usbmon capture (extracted text format).

Reads the extracted EP1 data from re/captures/usbmon4-ep1.txt and decrypts
all command/response pairs using the LFSR-based XOR obfuscation scheme.

The input format is tab-separated:
    frame_num  timestamp  endpoint_hex  length  hex_data

Usage:
    python re/scripts/decode-usbmon-ep1.py [input_file]
"""

import sys
from dataclasses import dataclass

# --- LFSR and encryption primitives (matches obfuscation.rs) ---

CONTROL_MASK = 0xD7
RESEED_FLAG = 0x20
FIRMWARE_FLAG = 0x08
INITIAL_SEED = 0x354B248E


def lfsr_step(state: int) -> int:
    feedback = (state ^ (state >> 1) ^ (state >> 21) ^ (state >> 31)) & 1
    return ((state >> 1) | (feedback << 31)) & 0xFFFFFFFF


def advance_seed(state: int) -> int:
    n = (state & 0x1F) + 0x23
    for _ in range(n):
        state = lfsr_step(state)
    return state


def seed_byte(seed: int, i: int) -> int:
    return (seed >> ((i % 4) * 8)) & 0xFF


def try_decode_reseed(wire_bytes: bytes, initial_seed: int):
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
    if len(wire_bytes) < 2:
        return None, wire_bytes, seed
    opcode = wire_bytes[1] ^ seed_byte(seed, 1)
    payload = bytearray()
    for i in range(2, len(wire_bytes)):
        payload.append(wire_bytes[i] ^ seed_byte(seed, i))
    new_seed = advance_seed(seed)
    return opcode, bytes(payload), new_seed


def decrypt_response(wire_bytes: bytes, seed: int):
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


def format_hex(data: bytes, max_len: int = 40) -> str:
    s = data.hex()
    if len(data) > max_len:
        return s[:max_len * 2] + f"... ({len(data)}B)"
    return s


def format_writereg(payload: bytes) -> str:
    if not payload or len(payload) < 1:
        return ""
    count = payload[0]
    pairs = []
    for i in range(count):
        off = 1 + i * 2
        if off + 1 < len(payload):
            addr = payload[off]
            val = payload[off + 1]
            pairs.append(f"r0x{addr:02x}=0x{val:02x}")
    return f"count={count} [{', '.join(pairs)}]"


def format_readreg(payload: bytes) -> str:
    if not payload or len(payload) < 1:
        return ""
    count = payload[0]
    addrs = []
    for i in range(count):
        if 1 + i < len(payload):
            addrs.append(f"0x{payload[1+i]:02x}")
    return f"count={count} addrs=[{', '.join(addrs)}]"


def format_command(opcode: int, payload: bytes) -> str:
    name = opcode_name(opcode)
    if opcode == 0x80:
        return f"{name} {format_writereg(payload)}"
    elif opcode == 0x81:
        return f"{name} {format_readreg(payload)}"
    elif opcode == 0x7F:
        if len(payload) >= 2:
            chunk_len = payload[0] | (payload[1] << 8)
            return f"{name} chunk_len={chunk_len} ({len(payload)-2}B data)"
        return f"{name} {format_hex(payload)}"
    elif opcode == 0x7D:
        if payload:
            return f"{name} test=0x{payload[0]:02x}"
        return name
    elif opcode == 0x07:
        if len(payload) >= 6:
            return (f"{name} params=[0x{payload[0]:02x},0x{payload[1]:02x},"
                    f"0x{payload[2]:02x},0x{payload[3]:02x},"
                    f"0x{payload[4]:02x},0x{payload[5]:02x}]")
        return f"{name} {format_hex(payload)}"
    elif opcode == 0x01:
        return f"{name} {format_hex(payload)}"
    elif payload:
        return f"{name} {format_hex(payload)}"
    return name


def format_response(opcode: int, data: bytes) -> str:
    if opcode == 0x8B:
        try:
            text = data.rstrip(b'\x00').decode('ascii', errors='replace')
            return f'"{text}"'
        except Exception:
            pass
    elif opcode == 0x07:
        return format_hex(data)
    elif opcode == 0x81:
        vals = [f"0x{b:02x}" for b in data]
        return f"[{', '.join(vals)}]"
    elif opcode in (0x80, 0x7E, 0x7F, 0x89) and len(data) == 1:
        return f"0x{data[0]:02x} ({'ACK' if data[0] == 0 else 'NACK'})"
    elif opcode == 0x7D and data:
        return f"0x{data[0]:02x} ({'OK' if data[0] == 0xAA else 'BAD'})"
    return format_hex(data)


@dataclass
class Frame:
    frame_num: int
    timestamp: float
    endpoint: int
    length: int
    data: bytes


def parse_input(path: str) -> list:
    frames = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            parts = line.split('\t')
            if len(parts) < 5:
                continue
            try:
                frame_num = int(parts[0])
                timestamp = float(parts[1])
                endpoint = int(parts[2], 16)
                length = int(parts[3])
                data = bytes.fromhex(parts[4])
                frames.append(Frame(frame_num, timestamp, endpoint, length, data))
            except (ValueError, IndexError):
                continue
    return frames


def main():
    input_file = sys.argv[1] if len(sys.argv) > 1 else "re/captures/usbmon4-ep1.txt"
    frames = parse_input(input_file)
    print(f"Loaded {len(frames)} EP1 frames from {input_file}\n")

    seed = INITIAL_SEED
    last_opcode = None
    fpga_chunks = 0
    fpga_bytes = 0
    in_fpga_upload = False
    fpga_start_frame = None
    fpga_end_frame = None
    seq_num = 0
    reseed_count = 0

    print(f"{'Seq':>4} {'Frame':>6} {'Time':>12} {'Dir':>4} {'Len':>5}  Decrypted")
    print("-" * 110)

    i = 0
    while i < len(frames):
        f = frames[i]

        if f.endpoint == 0x01:  # EP1 OUT
            is_reseed, reseed_val = try_decode_reseed(f.data, INITIAL_SEED)

            if is_reseed:
                if in_fpga_upload:
                    seq_num += 1
                    print(f"{seq_num:4} {fpga_start_frame:6}-{fpga_end_frame:<6}          OUT       "
                          f"WriteConfigData: {fpga_chunks} chunks, {fpga_bytes}B bitstream")
                    in_fpga_upload = False

                reseed_count += 1
                seq_num += 1
                print(f"{seq_num:4} {f.frame_num:6} {f.timestamp:12.6f}  OUT {f.length:5}  "
                      f"RESEED #{reseed_count} -> 0x{reseed_val:08x}")
                seed = reseed_val
                last_opcode = None
                i += 1
                continue

            opcode, payload, new_seed = decrypt_command(f.data, seed)

            if opcode == 0x7F and f.length >= 100:
                if not in_fpga_upload:
                    in_fpga_upload = True
                    fpga_start_frame = f.frame_num
                    fpga_chunks = 0
                    fpga_bytes = 0
                fpga_chunks += 1
                if len(payload) >= 2:
                    fpga_bytes += payload[0] | (payload[1] << 8)
                fpga_end_frame = f.frame_num
                seed = new_seed
                i += 1
                if i < len(frames) and frames[i].endpoint == 0x81:
                    _, seed = decrypt_response(frames[i].data, seed)
                    last_opcode = 0x7F
                    i += 1
                continue

            if in_fpga_upload:
                seq_num += 1
                print(f"{seq_num:4} {fpga_start_frame:6}-{fpga_end_frame:<6}          OUT       "
                      f"WriteConfigData: {fpga_chunks} chunks, {fpga_bytes}B bitstream")
                in_fpga_upload = False

            seq_num += 1
            cmd_str = format_command(opcode, payload)
            print(f"{seq_num:4} {f.frame_num:6} {f.timestamp:12.6f}  OUT {f.length:5}  "
                  f"{cmd_str}")
            seed = new_seed
            last_opcode = opcode

        elif f.endpoint == 0x81:  # EP1 IN
            data, new_seed = decrypt_response(f.data, seed)
            seed = new_seed

            if last_opcode is not None:
                resp_str = format_response(last_opcode, data)
                seq_num += 1
                print(f"{seq_num:4} {f.frame_num:6} {f.timestamp:12.6f}   IN {f.length:5}  "
                      f"  -> {resp_str}")
            else:
                seq_num += 1
                print(f"{seq_num:4} {f.frame_num:6} {f.timestamp:12.6f}   IN {f.length:5}  "
                      f"  -> {format_hex(data)}")
            last_opcode = None

        i += 1

    if in_fpga_upload:
        seq_num += 1
        print(f"{seq_num:4} {fpga_start_frame:6}-{fpga_end_frame:<6}          OUT       "
              f"WriteConfigData: {fpga_chunks} chunks, {fpga_bytes}B bitstream")

    print(f"\nTotal: {seq_num} operations, {reseed_count} reseeds")
    print(f"Final seed: 0x{seed:08x}")


if __name__ == "__main__":
    main()
