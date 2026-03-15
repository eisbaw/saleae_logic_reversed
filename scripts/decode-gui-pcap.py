#!/usr/bin/env python3
"""Decode EP1 commands from GUI-captured pcap files.

Wraps the existing decode-pcap-init.py logic but accepts any pcap path.
Focuses on extracting WriteRegisters (0x80) commands that set capture config.

Usage:
    python3 scripts/decode-gui-pcap.py re/captures/gui_pcaps/usbmon4_digital_1mhz_8ch_*.pcap
"""

import os
import sys

# Add re/scripts to path so we can import the decoder
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 're', 'scripts'))

# Monkey-patch the PCAP_PATH before importing
import importlib.util
spec = importlib.util.spec_from_file_location(
    "decode_pcap_init",
    os.path.join(os.path.dirname(__file__), '..', 're', 'scripts', 'decode-pcap-init.py')
)
mod = importlib.util.module_from_spec(spec)

# We need to override PCAP_PATH and extract_frames, so just copy the crypto primitives
# and reimplement the main loop focused on register writes.

import subprocess
from dataclasses import dataclass


# --- LFSR and encryption primitives (from decode-pcap-init.py) ---

CONTROL_MASK = 0xD7
RESEED_FLAG = 0x20
INITIAL_SEED = 0x354B248E


def lfsr_step(state):
    feedback = (state ^ (state >> 1) ^ (state >> 21) ^ (state >> 31)) & 1
    return ((state >> 1) | (feedback << 31)) & 0xFFFFFFFF


def advance_seed(state):
    n = (state & 0x1F) + 0x23
    for _ in range(n):
        state = lfsr_step(state)
    return state


def seed_byte(seed, i):
    return (seed >> ((i % 4) * 8)) & 0xFF


def try_decode_reseed(wire_bytes, initial_seed):
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


def decrypt_command(wire_bytes, seed):
    if len(wire_bytes) < 2:
        return None, wire_bytes, seed
    opcode = wire_bytes[1] ^ seed_byte(seed, 1)
    payload = bytearray()
    for i in range(2, len(wire_bytes)):
        payload.append(wire_bytes[i] ^ seed_byte(seed, i))
    new_seed = advance_seed(seed)
    return opcode, bytes(payload), new_seed


def decrypt_response(wire_bytes, seed):
    data = bytearray(len(wire_bytes))
    for i in range(len(wire_bytes)):
        data[i] = wire_bytes[i] ^ seed_byte(seed, i)
    new_seed = advance_seed(seed)
    return bytes(data), new_seed


OPCODES = {
    0x01: "StartSampling",
    0x02: "StopStream",
    0x07: "ReadVersion",
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
}


@dataclass
class PcapFrame:
    frame_num: int
    timestamp: float
    endpoint: int
    data_len: int
    data: bytes


def extract_frames(pcap_path):
    cmd = [
        "tshark", "-r", pcap_path,
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


def decode_pcap(pcap_path):
    """Decode a pcap, focusing on WriteRegisters and key commands."""
    print(f"\n{'='*80}")
    print(f"Decoding: {pcap_path}")
    print(f"{'='*80}")

    frames = extract_frames(pcap_path)
    print(f"Found {len(frames)} EP1 frames")

    seed = INITIAL_SEED
    last_opcode = None
    register_writes = []  # Collect all WriteRegisters for summary
    all_commands = []

    i = 0
    while i < len(frames):
        f = frames[i]

        if f.endpoint == 0x01:  # EP1 OUT
            is_reseed, reseed_val = try_decode_reseed(f.data, INITIAL_SEED)
            if is_reseed:
                seed = reseed_val
                all_commands.append(("RESEED", f.frame_num, f"-> 0x{reseed_val:08x}"))
                last_opcode = None
                i += 1
                continue

            opcode, payload, new_seed = decrypt_command(f.data, seed)
            seed = new_seed

            name = OPCODES.get(opcode, f"Unknown_0x{opcode:02x}")

            if opcode == 0x80:  # WriteRegisters
                if payload and len(payload) >= 1:
                    count = payload[0]
                    pairs = []
                    for j in range(count):
                        if 1 + j * 2 + 1 < len(payload):
                            addr = payload[1 + j * 2]
                            val = payload[1 + j * 2 + 1]
                            pairs.append((addr, val))
                    register_writes.append((f.frame_num, pairs))
                    detail = ", ".join(f"r0x{a:02x}=0x{v:02x}" for a, v in pairs)
                    all_commands.append(("WREG", f.frame_num, f"count={count} [{detail}]"))
            elif opcode == 0x81:  # ReadRegisters
                if payload and len(payload) >= 1:
                    count = payload[0]
                    addrs = [payload[1+j] for j in range(count) if 1+j < len(payload)]
                    detail = ", ".join(f"0x{a:02x}" for a in addrs)
                    all_commands.append(("RREG", f.frame_num, f"count={count} addrs=[{detail}]"))
            elif opcode == 0x7F:  # WriteConfigData (FPGA bitstream)
                if f.data_len >= 100:
                    all_commands.append(("FPGA", f.frame_num, f"chunk {f.data_len} bytes"))
                else:
                    all_commands.append(("FPGA_END", f.frame_num, f"final chunk {f.data_len} bytes"))
            elif opcode == 0x01:  # StartSampling
                all_commands.append(("START", f.frame_num, "StartSampling"))
            elif opcode == 0x02:  # StopStream
                all_commands.append(("STOP", f.frame_num, "StopStream"))
            elif opcode == 0x87:  # WriteI2C (ADC config)
                if len(payload) >= 4:
                    addr = payload[0]
                    reg = payload[1]
                    cnt = payload[2] | (payload[3] << 8)
                    data = payload[4:]
                    detail = f"addr=0x{addr:02x} reg=0x{reg:02x} cnt={cnt} data={data.hex()}"
                    all_commands.append(("I2C_W", f.frame_num, detail))
            elif opcode == 0x88:  # ReadI2C
                if len(payload) >= 4:
                    addr = payload[0]
                    reg = payload[1]
                    cnt = payload[2] | (payload[3] << 8)
                    all_commands.append(("I2C_R", f.frame_num, f"addr=0x{addr:02x} reg=0x{reg:02x} cnt={cnt}"))
            else:
                all_commands.append((name, f.frame_num, f"payload={payload.hex()}" if payload else ""))

            last_opcode = opcode

        elif f.endpoint == 0x81:  # EP1 IN (response)
            data, new_seed = decrypt_response(f.data, seed)
            seed = new_seed

            if last_opcode == 0x81:  # ReadRegisters response
                vals = [f"0x{b:02x}" for b in data]
                all_commands.append(("RREG_R", f.frame_num, f"  -> [{', '.join(vals)}]"))
            elif last_opcode == 0x88:  # ReadI2C response
                all_commands.append(("I2C_RR", f.frame_num, f"  -> {data.hex()}"))

            last_opcode = None

        i += 1

    # Print non-FPGA commands (skip bulk FPGA upload)
    fpga_count = 0
    print(f"\n{'Frame':>6}  {'Type':>8}  Detail")
    print("-" * 80)
    for cmd_type, frame, detail in all_commands:
        if cmd_type == "FPGA":
            fpga_count += 1
            continue
        if cmd_type == "FPGA_END" and fpga_count > 0:
            print(f"   ...  {'FPGA':>8}  {fpga_count} chunks uploaded")
            fpga_count = 0
            continue
        print(f"{frame:6}  {cmd_type:>8}  {detail}")

    # Print capture register summary
    if register_writes:
        print(f"\n{'='*80}")
        print("CAPTURE REGISTER SUMMARY (WriteRegisters 0x80):")
        print("-" * 80)
        # Find the last batch of register writes (the capture config)
        # Usually the capture config is the last set of WriteRegisters before StartSampling
        for frame_num, pairs in register_writes:
            for addr, val in pairs:
                print(f"  r0x{addr:02x} = 0x{val:02x}  (frame {frame_num})")
        print()

    return register_writes


def main():
    if len(sys.argv) < 2:
        print("Usage: python3 scripts/decode-gui-pcap.py <pcap_file> [pcap_file2 ...]")
        sys.exit(1)

    for pcap_path in sys.argv[1:]:
        decode_pcap(pcap_path)


if __name__ == "__main__":
    main()
