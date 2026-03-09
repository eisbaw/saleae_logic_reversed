# TraceEP1Commands.py -- Ghidra script to trace EP1 OUT/IN command protocol
# call sites in libgraph_server_shared.so.
#
# The FX3 firmware uses a command/response protocol on EP1 OUT (0x01) and
# EP1 IN (0x81). Commands are dispatched through a vtable pattern:
#   vtable[0x20] = write command to EP1 OUT
#   vtable[0x28] = read response from EP1 IN
#   vtable[0x18] = raw register access (no opcode, 2/6 byte packets)
#
# Known command opcodes (from binary analysis of v2.4.40):
#   0x01 = COMMAND_START_READ
#   0x02 = COMMAND_STOP_STREAM
#   0x22 = ReseedDevice (direct bulk write, not vtable)
#   0x55 = WriteEeprom
#   0x7E = ResetFpga
#   0x7F = WriteConfigurationData (FPGA bitstream/config)
#   0x81 = ReadRegisters
#   0x82 = ResetAndReEnumerate
#   0x86 = StopDataPath
#   0x88 = ReadI2c
#   0x89 = Device2_1 (device info query)
#   0x8A = CMD_ASSERT_TEST
#   0x8B = GetDeviceInfo
#
# This script finds functions that:
# 1. Reference known command-related strings
# 2. Construct command packets (mov byte [reg], imm8 patterns)
# 3. Call through vtable offsets 0x18, 0x20, 0x28
#
# Output: re/ghidra/reports/ep1-command-callsites.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceEP1Commands.py
#
# @author  saleae_reverse project
# @category USB
# @keybinding
# @menupath
# @toolbar

from ghidra.program.model.symbol import SymbolType
from ghidra.program.model.listing import CodeUnit
from ghidra.program.util import DefinedDataIterator
from ghidra.util.task import ConsoleTaskMonitor

import os
import time

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Known command opcodes and their names
COMMAND_OPCODES = {
    0x01: "COMMAND_START_READ",
    0x02: "COMMAND_STOP_STREAM",
    0x22: "ReseedDevice",
    0x55: "WriteEeprom",
    0x7E: "ResetFpga",
    0x7F: "WriteConfigurationData",
    0x81: "ReadRegisters",
    0x82: "ResetAndReEnumerate",
    0x86: "StopDataPath",
    0x88: "ReadI2c",
    0x89: "Device2_1",
    0x8A: "CMD_ASSERT_TEST",
    0x8B: "GetDeviceInfo",
}

# Strings that identify command protocol code paths
COMMAND_STRINGS = [
    "COMMAND_START_READ",
    "COMMAND_STOP_STREAM",
    "COMMAND_START_READ reply",
    "COMMAND_STOP_STREAM reply",
    "Start command error",
    "Stop command error",
    "Sending start command",
    "start command",
    "stop command",
    "too many registers for single write command",
    "ResetFpga",
    "WriteConfigurationData",
    "Failed to write configuration data",
    "failed to write capture parameters",
    "Failed to send start command",
    "Failed to send stop command",
    "Failed to send STOP_STREAM command",
    "Capture parameter failed to download",
    "unrecognized command",
    "process_raw_command",
    "Device2_1 reply",
    "Write eeprom response read",
    "I2c tunnel",
    "ReadI2c",
    "ReseedDevice",
    "TestFirmwareResponsivnessAndReseed",
    "CMD_ASSERT_TEST",
    "GetDeviceInfo",
]

# Vtable offsets for EP1 command protocol
VTABLE_OFFSETS = {
    0x18: "RAW_REGISTER_ACCESS",   # Direct 2/6 byte register read/write
    0x20: "COMMAND_WRITE_EP1OUT",   # Write command byte(s) to EP1 OUT
    0x28: "COMMAND_READ_EP1IN",     # Read response byte(s) from EP1 IN
}

REPORT_FILENAME = "ep1-command-callsites.txt"


def get_report_path():
    """Determine the output report path."""
    candidates = [
        os.path.expanduser("~/nvidia/saleae_reverse/re/ghidra/reports"),
        os.path.expanduser("~/saleae_reverse/re/ghidra/reports"),
        os.path.expanduser("~"),
    ]
    for d in candidates:
        if os.path.isdir(d):
            return os.path.join(d, REPORT_FILENAME)
    return os.path.join(os.path.expanduser("~"), REPORT_FILENAME)


def find_function_by_name(program, name):
    """Find a function by its exact symbol name."""
    sym_table = program.getSymbolTable()
    symbols = sym_table.getGlobalSymbols(name)
    for sym in symbols:
        if sym.getSymbolType() == SymbolType.FUNCTION:
            return getFunctionAt(sym.getAddress())
    fm = program.getFunctionManager()
    funcs = fm.getFunctions(True)
    for func in funcs:
        if func.getName() == name:
            return func
    return None


def find_functions_containing(program, substring):
    """Find functions whose name contains the given substring."""
    results = []
    fm = program.getFunctionManager()
    funcs = fm.getFunctions(True)
    for func in funcs:
        if substring in func.getName():
            results.append(func)
    return results


def get_callers(program, func):
    """Get all functions that call the given function (via XREF)."""
    callers = []
    if func is None:
        return callers
    entry = func.getEntryPoint()
    refs = getReferencesTo(entry)
    seen = set()
    for ref in refs:
        if ref.getReferenceType().isCall() or ref.getReferenceType().isUnconditionalFlow():
            caller_addr = ref.getFromAddress()
            caller_func = getFunctionContaining(caller_addr)
            if caller_func is not None and caller_func.getEntryPoint() not in seen:
                seen.add(caller_func.getEntryPoint())
                callers.append((caller_func, caller_addr))
    return callers


def get_string_refs_in_function(program, func):
    """Find string references within a function's body."""
    strings_found = []
    if func is None:
        return strings_found
    body = func.getBody()
    listing = program.getListing()
    inst_iter = listing.getInstructions(body, True)
    while inst_iter.hasNext():
        inst = inst_iter.next()
        for i in range(inst.getNumOperands()):
            refs = inst.getOperandReferences(i)
            for ref in refs:
                target = ref.getToAddress()
                data = listing.getDataAt(target)
                if data is not None and data.hasStringValue():
                    strings_found.append(str(data.getValue()))
    return strings_found


def find_vtable_calls_in_function(program, func):
    """Find indirect calls through vtable offsets in a function.

    Looks for patterns like:
        call [rax+0x20]   ; command write
        call [rax+0x28]   ; command read
        call [rax+0x18]   ; raw register access
    """
    vtable_calls = []
    if func is None:
        return vtable_calls
    body = func.getBody()
    listing = program.getListing()
    inst_iter = listing.getInstructions(body, True)
    while inst_iter.hasNext():
        inst = inst_iter.next()
        mnemonic = inst.getMnemonicString()
        if mnemonic == "CALL":
            # Check if the operand is an indirect reference like [REG+offset]
            op_repr = inst.getDefaultOperandRepresentation(0)
            for offset, name in VTABLE_OFFSETS.items():
                hex_offset = "0x{:x}".format(offset)
                if hex_offset in op_repr.lower():
                    vtable_calls.append({
                        "address": inst.getAddress(),
                        "offset": offset,
                        "name": name,
                        "operand": op_repr,
                    })
    return vtable_calls


def find_command_byte_setup(program, func):
    """Find mov byte [reg], imm8 instructions that set command opcodes.

    Looks for patterns like:
        mov byte ptr [rbx], 0x7f    ; set command opcode
        mov byte ptr [rdi], 0x01    ; set command opcode

    These typically appear just before a vtable[0x20] call.
    """
    opcode_sets = []
    if func is None:
        return opcode_sets
    body = func.getBody()
    listing = program.getListing()
    inst_iter = listing.getInstructions(body, True)
    while inst_iter.hasNext():
        inst = inst_iter.next()
        mnemonic = inst.getMnemonicString()
        if mnemonic == "MOV":
            if inst.getNumOperands() >= 2:
                op0 = inst.getDefaultOperandRepresentation(0)
                op1 = inst.getDefaultOperandRepresentation(1)
                # Look for byte-sized memory store of an immediate
                # that matches a known command opcode
                try:
                    if op1.startswith("0x"):
                        val = int(op1, 16)
                    elif op1.lstrip("-").isdigit():
                        val = int(op1)
                    else:
                        continue

                    if val in COMMAND_OPCODES:
                        opcode_sets.append({
                            "address": inst.getAddress(),
                            "opcode": val,
                            "name": COMMAND_OPCODES[val],
                            "dest": op0,
                            "instruction": str(inst),
                        })
                except (ValueError, AttributeError):
                    pass
    return opcode_sets


def find_command_string_refs(program):
    """Find all defined strings matching command protocol patterns.

    Returns a dict of string_address -> (string_value, referencing_functions).
    """
    results = {}
    for data in DefinedDataIterator.definedStrings(program):
        val = str(data.getValue())
        for cmd_str in COMMAND_STRINGS:
            if cmd_str.lower() in val.lower():
                addr = data.getAddress()
                refs = getReferencesTo(addr)
                ref_funcs = []
                seen = set()
                for ref in refs:
                    func = getFunctionContaining(ref.getFromAddress())
                    if func and func.getEntryPoint() not in seen:
                        seen.add(func.getEntryPoint())
                        ref_funcs.append((func, ref.getFromAddress()))
                results[str(addr)] = {
                    "string": val,
                    "address": addr,
                    "refs": ref_funcs,
                }
                break  # Only match once per string
    return results


# ---------------------------------------------------------------------------
# Main script
# ---------------------------------------------------------------------------

def main():
    monitor = ConsoleTaskMonitor()
    program = currentProgram
    report_path = get_report_path()

    lines = []
    lines.append("=" * 78)
    lines.append("EP1 COMMAND PROTOCOL CALL SITE ANALYSIS")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")
    lines.append("Known command opcodes:")
    for opcode in sorted(COMMAND_OPCODES.keys()):
        lines.append("  0x{:02X} = {}".format(opcode, COMMAND_OPCODES[opcode]))
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: String references to command protocol code
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: COMMAND PROTOCOL STRING REFERENCES")
    lines.append("=" * 78)
    lines.append("")

    cmd_string_refs = find_command_string_refs(program)
    all_command_functions = {}  # func_entry -> func object

    for addr_str in sorted(cmd_string_refs.keys()):
        info = cmd_string_refs[addr_str]
        lines.append("  String: \"{}\"".format(info["string"][:100]))
        lines.append("  Address: {}".format(info["address"]))
        lines.append("  Referenced by {} function(s):".format(len(info["refs"])))
        for func, ref_addr in info["refs"]:
            lines.append("    {} @ {} (ref at {})".format(
                func.getName(), func.getEntryPoint(), ref_addr))
            all_command_functions[str(func.getEntryPoint())] = func
        lines.append("")

    lines.append("Total unique functions referencing command strings: {}".format(
        len(all_command_functions)))
    lines.append("")

    # ------------------------------------------------------------------
    # Part 2: Vtable call analysis in command functions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 2: VTABLE CALL ANALYSIS IN COMMAND FUNCTIONS")
    lines.append("(Looking for call [reg+0x18/0x20/0x28] patterns)")
    lines.append("=" * 78)
    lines.append("")

    for func_entry in sorted(all_command_functions.keys()):
        func = all_command_functions[func_entry]
        vtable_calls = find_vtable_calls_in_function(program, func)

        if vtable_calls:
            lines.append("  Function: {} @ {}".format(
                func.getName(), func.getEntryPoint()))
            for vc in vtable_calls:
                lines.append("    {} @ {} -> {} ({})".format(
                    vc["operand"], vc["address"], vc["name"],
                    "0x{:x}".format(vc["offset"])))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 3: Command opcode byte setup instructions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 3: COMMAND OPCODE BYTE SETUP")
    lines.append("(MOV instructions that set known command opcode values)")
    lines.append("=" * 78)
    lines.append("")

    opcode_locations = {}  # opcode -> list of (func, address)

    for func_entry in sorted(all_command_functions.keys()):
        func = all_command_functions[func_entry]
        opcode_sets = find_command_byte_setup(program, func)

        if opcode_sets:
            lines.append("  Function: {} @ {}".format(
                func.getName(), func.getEntryPoint()))
            for os_info in opcode_sets:
                lines.append("    0x{:02X} ({}) @ {} -- {}".format(
                    os_info["opcode"], os_info["name"],
                    os_info["address"], os_info["instruction"]))
                if os_info["opcode"] not in opcode_locations:
                    opcode_locations[os_info["opcode"]] = []
                opcode_locations[os_info["opcode"]].append(
                    (func, os_info["address"]))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 4: Per-opcode cross-reference summary
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 4: PER-OPCODE CROSS-REFERENCE SUMMARY")
    lines.append("=" * 78)
    lines.append("")

    for opcode in sorted(COMMAND_OPCODES.keys()):
        name = COMMAND_OPCODES[opcode]
        locs = opcode_locations.get(opcode, [])
        lines.append("  0x{:02X} {} -- {} location(s)".format(
            opcode, name, len(locs)))
        for func, addr in locs:
            lines.append("    {} @ {} (set at {})".format(
                func.getName(), func.getEntryPoint(), addr))

            # Also show strings in this function for context
            str_refs = get_string_refs_in_function(program, func)
            relevant = [s for s in str_refs if any(
                kw in s.lower() for kw in [
                    "command", "start", "stop", "fpga", "register",
                    "config", "eeprom", "i2c", "reseed", "reset",
                    "firmware", "device", "error", "failed"])]
            if relevant:
                lines.append("      Strings:")
                for s in relevant[:8]:
                    lines.append("        \"{}\"".format(s[:100]))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 5: Broader search -- any function with vtable 0x20/0x28 calls
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 5: BROADER VTABLE 0x20/0x28 CALL SEARCH")
    lines.append("(Scanning all functions in the binary for EP1 vtable calls)")
    lines.append("NOTE: This section may take several minutes on large binaries.")
    lines.append("=" * 78)
    lines.append("")

    # This is a broader scan. To keep it tractable, we focus on functions
    # that also have command-related string references.
    fm = program.getFunctionManager()
    func_iter = fm.getFunctions(True)
    broader_hits = []
    scanned = 0

    while func_iter.hasNext():
        func = func_iter.next()
        scanned += 1
        if scanned % 5000 == 0:
            println("Scanned {} functions...".format(scanned))
            if monitor.isCancelled():
                lines.append("  [CANCELLED by user after {} functions]".format(
                    scanned))
                break

        # Quick check: does this function have vtable calls at our offsets?
        vtable_calls = find_vtable_calls_in_function(program, func)
        if vtable_calls:
            # Filter to only 0x20 and 0x28 (command write/read)
            cmd_calls = [vc for vc in vtable_calls
                         if vc["offset"] in (0x20, 0x28)]
            if cmd_calls:
                func_entry_str = str(func.getEntryPoint())
                if func_entry_str not in all_command_functions:
                    str_refs = get_string_refs_in_function(program, func)
                    broader_hits.append({
                        "func": func,
                        "vtable_calls": cmd_calls,
                        "strings": str_refs,
                    })

    lines.append("  Scanned {} total functions".format(scanned))
    lines.append("  Found {} additional functions with vtable 0x20/0x28 calls".format(
        len(broader_hits)))
    lines.append("")

    for hit in broader_hits:
        func = hit["func"]
        lines.append("  {} @ {}".format(func.getName(), func.getEntryPoint()))
        for vc in hit["vtable_calls"]:
            lines.append("    {} @ {} -> {}".format(
                vc["operand"], vc["address"], vc["name"]))

        # Check for command opcode setup
        opcode_sets = find_command_byte_setup(program, func)
        if opcode_sets:
            for os_info in opcode_sets:
                lines.append("    OPCODE: 0x{:02X} ({}) @ {}".format(
                    os_info["opcode"], os_info["name"], os_info["address"]))

        relevant = [s for s in hit["strings"] if any(
            kw in s.lower() for kw in [
                "command", "start", "stop", "fpga", "register",
                "config", "eeprom", "i2c", "reseed", "error"])]
        if relevant:
            lines.append("    Strings:")
            for s in relevant[:5]:
                lines.append("      \"{}\"".format(s[:100]))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 6: Summary
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("SUMMARY")
    lines.append("=" * 78)
    lines.append("")
    lines.append("Command protocol vtable offsets:")
    for offset, name in sorted(VTABLE_OFFSETS.items()):
        lines.append("  [+0x{:02X}] {}".format(offset, name))
    lines.append("")
    lines.append("Command opcodes found in code:")
    for opcode in sorted(opcode_locations.keys()):
        name = COMMAND_OPCODES.get(opcode, "UNKNOWN")
        count = len(opcode_locations[opcode])
        lines.append("  0x{:02X} {} -- {} call site(s)".format(
            opcode, name, count))
    lines.append("")
    lines.append("Functions referencing command strings: {}".format(
        len(all_command_functions)))
    lines.append("Additional functions with vtable 0x20/0x28: {}".format(
        len(broader_hits)))
    lines.append("")

    # ------------------------------------------------------------------
    # Write report
    # ------------------------------------------------------------------
    report = "\n".join(lines)

    try:
        with open(report_path, "w") as f:
            f.write(report)
        println("Report written to: {}".format(report_path))
    except IOError as e:
        println("ERROR writing report: {}".format(e))
        println("Printing report to console instead:")
        println(report)

    println("")
    println("=== SUMMARY ===")
    println("Command string refs: {}".format(len(cmd_string_refs)))
    println("Command functions: {}".format(len(all_command_functions)))
    println("Opcode locations: {}".format(len(opcode_locations)))
    println("Broader vtable hits: {}".format(len(broader_hits)))
    println("Report: {}".format(report_path))


# Entry point
if __name__ == "__main__":
    main()
else:
    main()
