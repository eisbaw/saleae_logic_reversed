#!/usr/bin/env python3
"""
Parse the GPIF II state machine descriptor from Saleae FX3 firmware.

The FX3 firmware loads a GPIF II state machine via CyU3PGpifLoad() during
initialization. This script extracts and decodes that descriptor from the
firmware binary image, revealing the parallel bus timing between the FX3
and the FPGA.

Usage:
    python3 re/scripts/parse-gpif.py [firmware_path]

Default firmware path: firmware/fw-fx3-pro8.img

References:
    - Cypress FX3 SDK: cyu3gpif.h (CyU3PGpifConfig_t struct)
    - Cypress FX3 SDK: gpif_regs.h (register bit field definitions)
    - Cypress FX3 SDK: cyu3gpif.c (CyU3PGpifLoad implementation)
"""

import struct
import sys
from pathlib import Path


# ---- FX3 Firmware Section Layout ----

SECTIONS = [
    # (name, file_offset, length_words, load_address)
    ("ITCM",   0x0C,    2272,  0x00000100),
    ("SYSMEM1", 0x2394, 16384, 0x40003000),
    ("SYSMEM2", 0x1239C, 9371, 0x40013000),
    ("SYSMEM3", 0x1B610, 1917, 0x40030000),
]

# CyU3PGpifConfig_t pointer is loaded at memory address 0x40005D30 in the
# LogicProApplicationInit function, from the literal pool value at 0x40005DC8.
# The value is 0x40008E58, which points to the config struct in SYSMEM1.
GPIF_CONFIG_STRUCT_ADDR = 0x40008E58


# ---- GPIF Register Names (from gpif_regs.h) ----

GPIF_REG_NAMES = {
    0x000: "GPIF_CONFIG",
    0x004: "GPIF_BUS_CONFIG",
    0x008: "GPIF_BUS_CONFIG2",
    0x00C: "GPIF_AD_CONFIG",
    0x010: "GPIF_STATUS",
    0x014: "GPIF_INTR",
    0x018: "GPIF_INTR_MASK",
    0x01C: "GPIF_SERIAL_IN_CONFIG",
    0x020: "GPIF_SERIAL_OUT_CONFIG",
    0x024: "GPIF_CTRL_BUS_DIRECTION",
    0x028: "GPIF_CTRL_BUS_DEFAULT",
    0x02C: "GPIF_CTRL_BUS_POLARITY",
    0x030: "GPIF_CTRL_BUS_TOGGLE",
}
for i in range(16):
    GPIF_REG_NAMES[0x034 + i * 4] = f"GPIF_CTRL_BUS_SELECT{i}"
GPIF_REG_NAMES.update({
    0x074: "GPIF_CTRL_COUNT_CONFIG",
    0x078: "GPIF_CTRL_COUNT_RESET",
    0x07C: "GPIF_CTRL_COUNT_LIMIT",
    0x080: "GPIF_ADDR_COUNT_CONFIG",
    0x084: "GPIF_ADDR_COUNT_RESET",
    0x088: "GPIF_ADDR_COUNT_LIMIT",
    0x08C: "GPIF_STATE_COUNT_CONFIG",
    0x090: "GPIF_STATE_COUNT_LIMIT",
    0x094: "GPIF_DATA_COUNT_CONFIG",
    0x098: "GPIF_DATA_COUNT_RESET",
    0x09C: "GPIF_DATA_COUNT_LIMIT",
    0x0A0: "GPIF_CTRL_COMP_VALUE",
    0x0A4: "GPIF_CTRL_COMP_MASK",
    0x0A8: "GPIF_DATA_COMP_VALUE",
    0x0AC: "GPIF_DATA_COMP_MASK",
    0x0B0: "GPIF_ADDR_COMP_VALUE",
    0x0B4: "GPIF_ADDR_COMP_MASK",
    0x0B8: "GPIF_DATA_CTRL",
    0x0BC: "GPIF_INGRESS_DATA0",
    0x0C0: "GPIF_INGRESS_DATA1",
    0x0C4: "GPIF_INGRESS_DATA2",
    0x0C8: "GPIF_INGRESS_DATA3",
    0x0CC: "GPIF_EGRESS_DATA0",
    0x0D0: "GPIF_EGRESS_DATA1",
    0x0D4: "GPIF_EGRESS_DATA2",
    0x0D8: "GPIF_EGRESS_DATA3",
    0x0DC: "GPIF_INGRESS_ADDRESS0",
    0x0E0: "GPIF_INGRESS_ADDRESS1",
    0x0E4: "GPIF_INGRESS_ADDRESS2",
    0x0E8: "GPIF_INGRESS_ADDRESS3",
    0x0EC: "GPIF_EGRESS_ADDRESS0",
    0x0F0: "GPIF_EGRESS_ADDRESS1",
    0x0F4: "GPIF_EGRESS_ADDRESS2",
    0x0F8: "GPIF_EGRESS_ADDRESS3",
    0x0FC: "GPIF_THREAD_CONFIG0",
    0x100: "GPIF_THREAD_CONFIG1",
    0x104: "GPIF_THREAD_CONFIG2",
    0x108: "GPIF_THREAD_CONFIG3",
    0x10C: "GPIF_LAMBDA_STAT0",
    0x110: "GPIF_LAMBDA_STAT1",
    0x114: "GPIF_LAMBDA_STAT2",
    0x118: "GPIF_LAMBDA_STAT3",
    0x11C: "GPIF_ALPHA_STAT0",
    0x120: "GPIF_ALPHA_STAT1",
    0x124: "GPIF_ALPHA_STAT2",
    0x128: "GPIF_ALPHA_STAT3",
    0x12C: "GPIF_BETA_STAT0",
})

# ---- Lambda Input Names (from cyu3gpif.h) ----

LAMBDA_NAMES = {
    0:  "CTRL[0]",
    1:  "CTRL[1]",
    2:  "CTRL[2]",
    3:  "CTRL[3]",
    4:  "CTRL[4]",
    5:  "CTRL[5]",
    6:  "CTRL[6]",
    7:  "CTRL[7]",
    8:  "CTRL[8]",
    9:  "CTRL[9]",
    10: "CTRL[10]",
    11: "CTRL[11]",
    12: "CTRL[12]",
    13: "CTRL[13]",
    14: "CTRL[14]",
    15: "CTRL[15]",
    16: "DOUT_VALID (egress data valid)",
    17: "DIN_VALID (ingress data valid)",
    18: "CTRL_CNT (control counter hit limit)",
    19: "ADDR_CNT (address counter hit limit)",
    20: "DATA_CNT (data counter hit limit)",
    21: "CTRL_CMP (control comparator match)",
    22: "ADDR_CMP (address comparator match)",
    23: "DATA_CMP (data comparator match)",
    24: "DMA_THRES (DMA above threshold)",
    25: "DMA_AREADY (address socket ready)",
    26: "DMA_DREADY (data socket ready)",
    27: "CRC_ERROR",
    28: "ONE (constant 1)",
    29: "INTR_ACTV (CPU interrupt active)",
    30: "CPU_DEF (CPU controlled)",
    31: "RESERVED",
}

# ---- Alpha Output Names (from cyu3gpif.h) ----

ALPHA_NAMES = {
    0: "DQ_OEN (data bus output enable)",
    1: "UPD_DOUT (pop from FIFO, update output data)",
    2: "SAMP_DIN (sample input data)",
    3: "SAMP_AIN (sample input address)",
    4: "USER0 (alpha_user0)",
    5: "USER1 (alpha_user1)",
    6: "USER2 (alpha_user2)",
    7: "USER3 (alpha_user3)",
}

# ---- Beta Output Names (from cyu3gpif.h) ----

BETA_NAMES = {
    0:  "USER0",
    1:  "USER1",
    2:  "USER2",
    3:  "USER3",
    4:  "RESERVED0 (thread number bit 0)",
    5:  "RESERVED1 (thread number bit 1)",
    6:  "POP_RQ (pop from read queue)",
    7:  "PUSH_WQ (push into write queue)",
    8:  "POP_ARQ (pop address from read queue)",
    9:  "PUSH_AWQ (push address into write queue)",
    10: "ADDR_OEN (address output enable)",
    11: "CTRL_OEN (control output enable)",
    12: "INC_CTRL (increment control counter)",
    13: "RST_CTRL (reset control counter)",
    14: "INC_ADDR (increment address counter)",
    15: "RST_ADDR (reset address counter)",
    16: "INC_DATA (increment data counter)",
    17: "RST_DATA (reset data counter)",
    18: "INTR_CPU (raise interrupt to ARM CPU)",
    19: "INTR_HOST (raise interrupt to host)",
    20: "UPD_AOUT (update address output)",
    21: "REG_ACCESS (MMIO register access)",
    22: "INIT_CRC (initialize CRC)",
    23: "CALC_CRC (include data in CRC)",
    24: "USE_CRC (check/output CRC)",
    25: "SET_DRQ (assert DRQ)",
    26: "CLR_DRQ (de-assert DRQ)",
    27: "THR_DONE (shut down thread)",
    28: "THR_WRAPUP (shut down thread, wrap up buffer)",
    29: "SEND_EOP (send end-of-packet to DMA)",
    30: "SEND_EOT (send end-of-transfer to DMA)",
    31: "PP_ACCESS (direct PP register access)",
}


def mem_to_file(mem_addr: int, sections: list) -> int:
    """Convert an FX3 memory address to a file offset in the firmware image."""
    for name, file_off, length_words, load_addr in sections:
        size = length_words * 4
        if load_addr <= mem_addr < load_addr + size:
            return file_off + (mem_addr - load_addr)
    raise ValueError(f"Memory address 0x{mem_addr:08x} not in any section")


def decode_transition_function(fn_val: int) -> str:
    """Decode a 16-bit transition function truth table.

    Each function takes 4 inputs (Fa, Fb, Fc, Fd) and produces 1 output.
    The 16-bit value is the truth table: bit N = output when inputs = N.
    """
    if fn_val == 0x0000:
        return "ALWAYS FALSE (0x0000)"
    if fn_val == 0xFFFF:
        return "ALWAYS TRUE (0xFFFF)"
    if fn_val == 0xAAAA:
        return "Fa (0xAAAA) -- output = input A"
    if fn_val == 0x5555:
        return "NOT Fa (0x5555) -- output = !A"
    if fn_val == 0xCCCC:
        return "Fb (0xCCCC) -- output = input B"
    if fn_val == 0x3333:
        return "NOT Fb (0x3333) -- output = !B"
    if fn_val == 0xF0F0:
        return "Fc (0xF0F0) -- output = input C"
    if fn_val == 0x0F0F:
        return "NOT Fc (0x0F0F) -- output = !C"
    if fn_val == 0xFF00:
        return "Fd (0xFF00) -- output = input D"
    if fn_val == 0x00FF:
        return "NOT Fd (0x00FF) -- output = !D"
    if fn_val == 0x8888:
        return "Fa AND Fb (0x8888)"
    if fn_val == 0xEEEE:
        return "Fa OR Fb (0xEEEE)"
    return f"0x{fn_val:04x} (custom truth table)"


def decode_waveform(left: list, right: list, trans_functions: list) -> dict:
    """Decode a CyU3PGpifWaveData entry into human-readable fields.

    Waveform descriptor bit fields (from gpif_regs.h):

    LEFT_WAVEFORM0 (leftData[0]):
      [7:0]   NEXT_STATE  - Next state index for left transition
      [12:8]  FA          - Lambda index for transition function input A
      [17:13] FB          - Lambda index for transition function input B
      [22:18] FC          - Lambda index for transition function input C
      [27:23] FD          - Lambda index for transition function input D
      [31:28] F0_L        - Transition function select (low 4 bits)

    LEFT_WAVEFORM1 (leftData[1]):
      [0]     F0_H        - Transition function select (high bit)
      [5:1]   F1          - Second transition function select
      [13:6]  ALPHA_LEFT  - Alpha outputs for left edge
      [21:14] ALPHA_RIGHT - Alpha outputs for right edge
      [31:22] BETA_L      - Beta outputs (low 10 bits)

    LEFT_WAVEFORM2 (leftData[2]):
      [21:0]  BETA_H      - Beta outputs (high 22 bits)
      [29:22] REPEAT_COUNT - Stay in state for (N+1) clock cycles
      [30]    BETA_DEASSERT - De-assert betas after one cycle
      [31]    VALID        - Entry is valid
    """
    result = {}

    # Left descriptor
    w0 = left[0]
    w1 = left[1]
    w2 = left[2]

    result["left_valid"] = bool(w2 & (1 << 31))
    result["left_next_state"] = w0 & 0xFF
    result["left_fa"] = (w0 >> 8) & 0x1F
    result["left_fb"] = (w0 >> 13) & 0x1F
    result["left_fc"] = (w0 >> 18) & 0x1F
    result["left_fd"] = (w0 >> 23) & 0x1F
    result["left_f0"] = ((w0 >> 28) & 0xF) | ((w1 & 1) << 4)
    result["left_f1"] = (w1 >> 1) & 0x1F
    result["left_alpha_left"] = (w1 >> 6) & 0xFF
    result["left_alpha_right"] = (w1 >> 14) & 0xFF
    beta_low = (w1 >> 22) & 0x3FF
    beta_high = w2 & 0x3FFFFF
    result["left_beta"] = beta_low | (beta_high << 10)
    result["left_repeat"] = (w2 >> 22) & 0xFF
    result["left_beta_deassert"] = bool(w2 & (1 << 30))

    # Right descriptor
    w0 = right[0]
    w1 = right[1]
    w2 = right[2]

    result["right_valid"] = bool(w2 & (1 << 31))
    result["right_next_state"] = w0 & 0xFF
    result["right_fa"] = (w0 >> 8) & 0x1F
    result["right_fb"] = (w0 >> 13) & 0x1F
    result["right_fc"] = (w0 >> 18) & 0x1F
    result["right_fd"] = (w0 >> 23) & 0x1F
    result["right_f0"] = ((w0 >> 28) & 0xF) | ((w1 & 1) << 4)
    result["right_f1"] = (w1 >> 1) & 0x1F
    result["right_alpha_left"] = (w1 >> 6) & 0xFF
    result["right_alpha_right"] = (w1 >> 14) & 0xFF
    beta_low = (w1 >> 22) & 0x3FF
    beta_high = w2 & 0x3FFFFF
    result["right_beta"] = beta_low | (beta_high << 10)
    result["right_repeat"] = (w2 >> 22) & 0xFF
    result["right_beta_deassert"] = bool(w2 & (1 << 30))

    return result


def format_alpha(alpha_val: int) -> str:
    """Format alpha output bits as human-readable string."""
    if alpha_val == 0:
        return "(none)"
    parts = []
    for bit in range(8):
        if alpha_val & (1 << bit):
            parts.append(ALPHA_NAMES.get(bit, f"bit{bit}"))
    return " | ".join(parts)


def format_beta(beta_val: int) -> str:
    """Format beta output bits as human-readable string."""
    if beta_val == 0:
        return "(none)"
    parts = []
    for bit in range(32):
        if beta_val & (1 << bit):
            parts.append(BETA_NAMES.get(bit, f"bit{bit}"))
    return " | ".join(parts)


def decode_gpif_config(val: int) -> list:
    """Decode GPIF_CONFIG register bits."""
    fields = []
    if val & (1 << 31):
        fields.append("ENABLE")
    if val & (1 << 30):
        fields.append("PP_MODE")
    if val & (1 << 15):
        fields.append("THREAD_IN_STATE")
    if val & (1 << 14):
        fields.append("SERIAL_MODE")
    if val & (1 << 13):
        fields.append("DATA_COMP_TOGGLE")
    if val & (1 << 12):
        fields.append("CTRL_COMP_TOGGLE")
    if val & (1 << 11):
        fields.append("ADDR_COMP_TOGGLE")
    if val & (1 << 10):
        fields.append("BIG_ENDIAN")
    if val & (1 << 9):
        fields.append("SYNC_SPEED (50-100MHz)")
    else:
        fields.append("SYNC_SPEED (<50MHz)")
    if val & (1 << 8):
        fields.append("SYNC")
    else:
        fields.append("ASYNC")
    if val & (1 << 7):
        fields.append("DOUT_POP_EN")
    if val & (1 << 6):
        fields.append("DDR_MODE (2x clock)")
    if val & (1 << 5):
        fields.append("CLK_OUT (output clock on pin)")
    if val & (1 << 4):
        fields.append("CLK_SOURCE (internal GCTL)")
    else:
        fields.append("CLK_SOURCE (external pin)")
    if val & (1 << 3):
        fields.append("CLK_INVERT")
    if val & (1 << 2):
        fields.append("DATA_COMP_ENABLE")
    if val & (1 << 1):
        fields.append("ADDR_COMP_ENABLE")
    if val & (1 << 0):
        fields.append("CTRL_COMP_ENABLE")
    return fields


def decode_bus_config(val: int) -> dict:
    """Decode GPIF_BUS_CONFIG register."""
    bus_widths = {0: "8-bit", 1: "16-bit", 2: "24-bit", 3: "32-bit"}
    pin_counts = {0: "47-pin", 1: "43-pin (no S1)", 2: "35-pin (no S0)", 3: "31-pin (no S0/S1)"}
    result = {}
    result["pin_count"] = pin_counts.get(val & 0x3, f"unknown({val & 0x3})")
    result["bus_width"] = bus_widths.get((val >> 2) & 0x3, f"unknown({(val >> 2) & 0x3})")
    result["mux_mode"] = bool(val & (1 << 4))
    result["adr_ctrl"] = (val >> 5) & 0xF
    result["ce_present"] = bool(val & (1 << 9))
    result["we_present"] = bool(val & (1 << 10))
    result["dle_present"] = bool(val & (1 << 11))
    result["oe_present"] = bool(val & (1 << 12))
    result["drq_present"] = bool(val & (1 << 13))
    result["fio0_present"] = bool(val & (1 << 14))
    result["fio1_present"] = bool(val & (1 << 15))
    result["cntr_present"] = bool(val & (1 << 16))
    result["ale_present"] = bool(val & (1 << 17))
    return result


def decode_thread_config(val: int) -> dict:
    """Decode GPIF_THREAD_CONFIG register."""
    result = {}
    result["socket"] = val & 0x1F
    result["wm_cfg"] = bool(val & (1 << 7))
    result["burst_size"] = (val >> 8) & 0xF
    result["burst_bytes"] = (1 << ((val >> 8) & 0xF)) * 4 if ((val >> 8) & 0xF) > 0 else 4
    result["watermark"] = (val >> 16) & 0x3FFF
    result["enable"] = bool(val & (1 << 31))
    return result


def decode_count_config(val: int) -> dict:
    """Decode a counter config register (CTRL/ADDR/DATA_COUNT_CONFIG)."""
    result = {}
    result["enable"] = bool(val & (1 << 0))
    result["reload"] = bool(val & (1 << 1))
    result["up_count"] = bool(val & (1 << 2))
    result["increment"] = (val >> 3) & 0xFF if val & 0x100 else (val >> 3) & 0x7
    result["sw_reset_en"] = bool(val & (1 << 8))
    return result


def main():
    fw_path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("firmware/fw-fx3-pro8.img")

    if not fw_path.exists():
        print(f"Error: firmware file not found: {fw_path}", file=sys.stderr)
        sys.exit(1)

    data = fw_path.read_bytes()
    print(f"Firmware: {fw_path} ({len(data)} bytes)")
    print()

    # Build section lookup
    sections = []
    for name, file_off, length_words, load_addr in SECTIONS:
        # Parse actual section headers from the firmware
        sections.append((name, file_off, length_words, load_addr))

    # Locate the CyU3PGpifConfig_t struct
    try:
        config_off = mem_to_file(GPIF_CONFIG_STRUCT_ADDR, sections)
    except ValueError as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

    # Parse CyU3PGpifConfig_t
    state_count = struct.unpack_from("<H", data, config_off)[0]
    state_data_ptr = struct.unpack_from("<I", data, config_off + 4)[0]
    state_pos_ptr = struct.unpack_from("<I", data, config_off + 8)[0]
    func_count = struct.unpack_from("<H", data, config_off + 12)[0]
    func_data_ptr = struct.unpack_from("<I", data, config_off + 16)[0]
    reg_count = struct.unpack_from("<H", data, config_off + 20)[0]
    reg_data_ptr = struct.unpack_from("<I", data, config_off + 24)[0]

    print("=" * 72)
    print("CyU3PGpifConfig_t")
    print("=" * 72)
    print(f"  Location:         0x{GPIF_CONFIG_STRUCT_ADDR:08x} (file offset 0x{config_off:x})")
    print(f"  stateCount:       {state_count}")
    print(f"  stateData:        0x{state_data_ptr:08x}")
    print(f"  statePosition:    0x{state_pos_ptr:08x}")
    print(f"  functionCount:    {func_count}")
    print(f"  functionData:     0x{func_data_ptr:08x}")
    print(f"  registerCount:    {reg_count}")
    print(f"  registerData:     0x{reg_data_ptr:08x}")
    print()

    # ---- Transition Functions ----
    print("=" * 72)
    print(f"Transition Functions ({func_count} functions)")
    print("=" * 72)
    func_off = mem_to_file(func_data_ptr, sections)
    trans_functions = []
    for i in range(func_count):
        fn_val = struct.unpack_from("<H", data, func_off + i * 2)[0]
        trans_functions.append(fn_val)
        print(f"  Function {i}: {decode_transition_function(fn_val)}")
    print()

    # ---- State Position Mapping ----
    print("=" * 72)
    print(f"State Position Mapping ({state_count} states)")
    print("=" * 72)
    sp_off = mem_to_file(state_pos_ptr, sections)
    state_positions = []
    for i in range(state_count):
        pos = data[sp_off + i]
        state_positions.append(pos)
        print(f"  State {i} -> waveform entry {pos}")
    print()

    # ---- Waveform Descriptors ----
    max_entry = max(state_positions) if state_positions else 0
    num_entries = max_entry + 1

    print("=" * 72)
    print(f"Waveform Descriptors ({num_entries} unique entries, {state_count} states)")
    print("=" * 72)

    sd_off = mem_to_file(state_data_ptr, sections)
    waveforms = []
    for entry in range(num_entries):
        off = sd_off + entry * 24
        left = [struct.unpack_from("<I", data, off + j * 4)[0] for j in range(3)]
        right = [struct.unpack_from("<I", data, off + j * 4 + 12)[0] for j in range(3)]
        wf = decode_waveform(left, right, trans_functions)
        waveforms.append(wf)

    # Print state table
    for state_idx in range(state_count):
        entry = state_positions[state_idx]
        wf = waveforms[entry]
        print(f"\n  --- State {state_idx} (waveform entry {entry}) ---")

        if wf["left_valid"]:
            print(f"  LEFT DESCRIPTOR:")
            print(f"    Next state:     {wf['left_next_state']}")
            print(f"    Repeat count:   {wf['left_repeat']} (+1 = {wf['left_repeat']+1} clocks)")

            # Transition function
            f0 = wf["left_f0"]
            f1 = wf["left_f1"]
            fa = wf["left_fa"]
            fb = wf["left_fb"]
            fc = wf["left_fc"]
            fd = wf["left_fd"]
            print(f"    Transition F0:  function[{f0}]", end="")
            if f0 < len(trans_functions):
                print(f" = {decode_transition_function(trans_functions[f0])}")
            else:
                print(" (out of range)")
            print(f"    Transition F1:  function[{f1}]", end="")
            if f1 < len(trans_functions):
                print(f" = {decode_transition_function(trans_functions[f1])}")
            else:
                print(" (out of range)")
            print(f"    Inputs:         Fa={LAMBDA_NAMES.get(fa, str(fa))}")
            print(f"                    Fb={LAMBDA_NAMES.get(fb, str(fb))}")
            print(f"                    Fc={LAMBDA_NAMES.get(fc, str(fc))}")
            print(f"                    Fd={LAMBDA_NAMES.get(fd, str(fd))}")

            # Alpha outputs
            al = wf["left_alpha_left"]
            ar = wf["left_alpha_right"]
            print(f"    Alpha (left):   0x{al:02x} = {format_alpha(al)}")
            print(f"    Alpha (right):  0x{ar:02x} = {format_alpha(ar)}")

            # Beta outputs
            beta = wf["left_beta"]
            print(f"    Beta:           0x{beta:08x} = {format_beta(beta)}")
            if wf["left_beta_deassert"]:
                print(f"    Beta deassert:  YES (pulse for one clock)")

        else:
            print(f"  LEFT DESCRIPTOR: (not valid)")

        if wf["right_valid"]:
            print(f"  RIGHT DESCRIPTOR:")
            print(f"    Next state:     {wf['right_next_state']}")
            print(f"    Repeat count:   {wf['right_repeat']} (+1 = {wf['right_repeat']+1} clocks)")
            f0 = wf["right_f0"]
            print(f"    Transition F0:  function[{f0}]", end="")
            if f0 < len(trans_functions):
                print(f" = {decode_transition_function(trans_functions[f0])}")
            else:
                print(" (out of range)")
        else:
            print(f"  RIGHT DESCRIPTOR: (not valid)")
    print()

    # ---- State Machine Summary ----
    print("=" * 72)
    print("State Machine Transition Summary")
    print("=" * 72)
    for state_idx in range(state_count):
        entry = state_positions[state_idx]
        wf = waveforms[entry]
        if wf["left_valid"]:
            f0 = wf["left_f0"]
            f1 = wf["left_f1"]
            next_s = wf["left_next_state"]
            repeat = wf["left_repeat"] + 1
            # Transition logic: F0 selects left vs right, F1 may modify
            # If F0 is TRUE, take left transition (to left_next_state)
            # If F0 is FALSE, stay or take right transition
            print(f"  State {state_idx}: stay {repeat} clock(s), "
                  f"then -> State {next_s} "
                  f"(F0=fn[{f0}], F1=fn[{f1}])")
    print()

    # ---- Register Init Data ----
    print("=" * 72)
    print(f"Register Init Data ({reg_count} registers)")
    print("=" * 72)
    print()
    reg_off = mem_to_file(reg_data_ptr, sections)

    # regData[0] -> GPIF_CONFIG (written last)
    # regData[1] -> GPIF_BUS_CONFIG (0xE0014004)
    # regData[N] -> GPIF register at 0xE0014000 + N*4

    reg_values = []
    for i in range(reg_count):
        val = struct.unpack_from("<I", data, reg_off + i * 4)[0]
        reg_values.append(val)
        reg_addr = 0xE0014000 + i * 4
        reg_name = GPIF_REG_NAMES.get(i * 4, f"GPIF_REG_0x{i*4:03x}")
        if val != 0:  # Only show non-zero registers for brevity
            print(f"  [{i:2d}] 0x{reg_addr:08x} {reg_name:35s} = 0x{val:08x}")

    print()

    # ---- Detailed Register Decoding ----
    print("=" * 72)
    print("Key Register Interpretation")
    print("=" * 72)
    print()

    # GPIF_CONFIG (regData[0])
    if reg_count > 0:
        val = reg_values[0]
        print(f"  GPIF_CONFIG = 0x{val:08x}")
        for field in decode_gpif_config(val):
            print(f"    - {field}")
        print()

    # GPIF_BUS_CONFIG (regData[1])
    if reg_count > 1:
        val = reg_values[1]
        bc = decode_bus_config(val)
        print(f"  GPIF_BUS_CONFIG = 0x{val:08x}")
        print(f"    Pin count:    {bc['pin_count']}")
        print(f"    Bus width:    {bc['bus_width']}")
        print(f"    MUX mode:     {bc['mux_mode']} (addr/data time-multiplexed)")
        print(f"    ADR_CTRL:     {bc['adr_ctrl']} (ctrl pins used as address)")
        active = [k for k in ["ce_present", "we_present", "dle_present",
                               "oe_present", "drq_present", "fio0_present",
                               "fio1_present", "cntr_present", "ale_present"]
                  if bc[k]]
        if active:
            print(f"    Active flags: {', '.join(active)}")
        print()

    # GPIF_AD_CONFIG (regData[3])
    if reg_count > 3:
        val = reg_values[3]
        print(f"  GPIF_AD_CONFIG = 0x{val:08x}")
        dq_mode = val & 0x3
        dq_modes = {0: "always outputs", 1: "always inputs",
                    2: "controlled by alpha[DQ_OEN]", 3: "reserved"}
        print(f"    DQ direction: {dq_modes.get(dq_mode, 'unknown')}")
        addr_mode = (val >> 2) & 0x3
        print(f"    ADDR mode:    {addr_mode}")
        data_thread = (val >> 4) & 0x3
        addr_thread = (val >> 6) & 0x3
        print(f"    Data thread:  {data_thread}")
        print(f"    Addr thread:  {addr_thread}")
        print()

    # CTRL_BUS_DIRECTION (regData[9])
    if reg_count > 9:
        val = reg_values[9]
        print(f"  GPIF_CTRL_BUS_DIRECTION = 0x{val:08x}")
        out_pins = []
        for bit in range(16):
            if val & (1 << bit):
                out_pins.append(f"CTRL[{bit}]")
        if out_pins:
            print(f"    Output pins:  {', '.join(out_pins)}")
        else:
            print(f"    All CTRL pins are inputs")
        print()

    # CTRL_BUS_DEFAULT (regData[10])
    if reg_count > 10:
        val = reg_values[10]
        print(f"  GPIF_CTRL_BUS_DEFAULT = 0x{val:04x}")
        if val == 0xFFFF:
            print(f"    All CTRL pins default HIGH")
        elif val == 0:
            print(f"    All CTRL pins default LOW")
        else:
            high_pins = [f"CTRL[{b}]" for b in range(16) if val & (1 << b)]
            print(f"    Default HIGH: {', '.join(high_pins)}")
        print()

    # CTRL_BUS_POLARITY (regData[11])
    if reg_count > 11:
        val = reg_values[11]
        print(f"  GPIF_CTRL_BUS_POLARITY = 0x{val:04x}")
        inv_pins = [f"CTRL[{b}]" for b in range(16) if val & (1 << b)]
        if inv_pins:
            print(f"    Inverted:     {', '.join(inv_pins)}")
        print()

    # Counter configs
    for name, idx in [("CTRL_COUNT", 29), ("ADDR_COUNT", 32), ("DATA_COUNT", 37)]:
        if reg_count > idx:
            cfg_val = reg_values[idx]
            reset_val = reg_values[idx + 1] if reg_count > idx + 1 else 0
            limit_val = reg_values[idx + 2] if reg_count > idx + 2 else 0
            if cfg_val != 0 or limit_val != 0:
                cc = decode_count_config(cfg_val)
                print(f"  {name}_CONFIG = 0x{cfg_val:08x}")
                print(f"    Enable:     {cc['enable']}")
                print(f"    Reload:     {cc['reload']}")
                print(f"    Up count:   {cc['up_count']}")
                print(f"    Increment:  {cc['increment']}")
                print(f"    Reset val:  0x{reset_val:08x}")
                print(f"    Limit:      0x{limit_val:08x} ({limit_val})")
                print()

    # Thread configs
    for t in range(4):
        idx = 63 + t  # THREAD_CONFIG0 is at register index 63
        if reg_count > idx:
            val = reg_values[idx]
            if val != 0:
                tc = decode_thread_config(val)
                print(f"  GPIF_THREAD_CONFIG{t} = 0x{val:08x}")
                print(f"    Socket:     {tc['socket']}")
                print(f"    Burst:      log2={tc['burst_size']} ({tc['burst_bytes']} bytes)")
                print(f"    Watermark:  {tc['watermark']} words")
                print(f"    WM config:  {'data > WM' if tc['wm_cfg'] else 'data <= WM'}")
                print(f"    Enable:     {tc['enable']}")
                print()

    # ---- Protocol Summary ----
    print("=" * 72)
    print("GPIF II Protocol Summary")
    print("=" * 72)
    print()

    if reg_count > 0:
        config_val = reg_values[0]
        sync = bool(config_val & (1 << 8))
        clk_src = "internal" if (config_val & (1 << 4)) else "external"
        clk_out = bool(config_val & (1 << 5))
        ddr = bool(config_val & (1 << 6))

        print(f"  Interface mode:     {'Synchronous' if sync else 'Asynchronous'}")
        print(f"  Clock source:       {clk_src}")
        print(f"  Clock output:       {'yes (master)' if clk_out else 'no (slave)'}")
        if ddr:
            print(f"  DDR mode:           enabled (2x clock)")

    if reg_count > 1:
        bc = decode_bus_config(reg_values[1])
        print(f"  Data bus width:     {bc['bus_width']}")
        print(f"  Pin configuration:  {bc['pin_count']}")

    if reg_count > 3:
        ad_val = reg_values[3]
        dq_mode = ad_val & 0x3
        if dq_mode == 1:
            print(f"  Data direction:     input only (FPGA -> FX3)")
        elif dq_mode == 0:
            print(f"  Data direction:     output only (FX3 -> FPGA)")
        elif dq_mode == 2:
            print(f"  Data direction:     bidirectional (alpha controlled)")

    print(f"  State count:        {state_count}")
    print(f"  Transition fns:     {func_count}")
    print()

    # Describe the state machine operation
    if state_count <= 4:
        print("  State machine description:")
        for state_idx in range(state_count):
            entry = state_positions[state_idx]
            wf = waveforms[entry]
            if wf["left_valid"]:
                next_s = wf["left_next_state"]
                repeat = wf["left_repeat"] + 1
                beta = wf["left_beta"]
                alpha_l = wf["left_alpha_left"]
                alpha_r = wf["left_alpha_right"]

                desc = f"    State {state_idx}: "
                actions = []
                if alpha_l & 0x04:  # SAMP_DIN
                    actions.append("sample input data")
                if beta & (1 << 7):  # PUSH_WQ
                    actions.append("push to write queue (DMA)")
                if beta & (1 << 16):  # INC_DATA
                    actions.append("increment data counter")
                if not actions:
                    actions.append("idle/wait")
                desc += ", ".join(actions)
                desc += f", repeat={repeat}, next={next_s}"
                print(desc)
        print()

    # Compare with fw-fx3-pro16.img if it exists
    pro16_path = fw_path.parent / "fw-fx3-pro16.img"
    if pro16_path.exists():
        pro16_data = pro16_path.read_bytes()
        pro16_config_off = mem_to_file(GPIF_CONFIG_STRUCT_ADDR, sections)
        pro16_state_count = struct.unpack_from("<H", pro16_data, pro16_config_off)[0]
        pro16_reg_data_ptr = struct.unpack_from("<I", pro16_data, pro16_config_off + 24)[0]
        pro16_reg_off = mem_to_file(pro16_reg_data_ptr, sections)

        # Compare register data
        diffs = 0
        for i in range(min(reg_count, 76)):
            v1 = struct.unpack_from("<I", data, reg_off + i * 4)[0]
            v2 = struct.unpack_from("<I", pro16_data, pro16_reg_off + i * 4)[0]
            if v1 != v2:
                diffs += 1
        print(f"  Pro 8 vs Pro 16:    {diffs} register differences in GPIF config")
        if diffs == 0:
            print(f"                      GPIF configuration is IDENTICAL")
        print()


if __name__ == "__main__":
    main()
