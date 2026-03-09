# TraceDisaggregator.py -- Ghidra script to trace the DisaggregatorNode and
# data interleaving architecture in libgraph_server_shared.so.
#
# This script analyzes how the interleaved USB bulk transfer data stream is
# split into per-channel digital and analog data. It traces:
#
#   - DisaggregatorNode -- the kernel that splits interleaved data
#   - DisaggregatorSettingsData -- channel layout description struct
#   - DisaggregatorNodeState2 -- runtime state of the disaggregator
#   - DisaggregatorActions::ConfigureGlitchFilters -- glitch filter config
#   - LogicDeviceNode::GetDisaggregatorSettings -- settings construction
#   - LogicDeviceNode::OnDataBuffer -- raw USB data entry point
#   - LogicDevice::ContractReadData -- data compaction (reveals format)
#   - LogicDevice::ExpandWriteData -- data expansion (reverse of contract)
#   - Logic2FpgaDevice::FakeReadThread -- simulation (reveals interleaving)
#   - LogicDevice::ModifyFakeData -- fake data generation (reveals layout)
#
# The goal is to determine:
#   1. How digital and analog channels are interleaved in the EP2 IN stream
#   2. The structure of DisaggregatorSettingsData
#   3. Whether interleaving is sample-level, chunk-level, or block-level
#   4. The role of GPIF width in determining the data layout
#
# Output: re/ghidra/reports/disaggregator-trace.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceDisaggregator.py
#
# @author  saleae_reverse project
# @category DataInterleaving
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

# Key functions related to data interleaving and disaggregation
PIPELINE_FUNCTIONS = [
    # Core disaggregator
    ("DisaggregatorNode", "Splits interleaved USB stream into per-channel data"),
    ("DisaggregatorSettingsData", "Channel layout description for disaggregation"),
    ("DisaggregatorNodeState2", "Runtime state of the disaggregator"),
    ("ConfigureGlitchFilters", "Per-channel digital glitch filter config"),
    ("GetDisaggregatorSettings", "Constructs DisaggregatorSettingsData"),

    # Data entry and formatting
    ("OnDataBuffer", "Raw USB data buffer reception from TransferQueue"),
    ("ContractReadData", "Compacts raw FPGA data (removes padding bytes)"),
    ("ExpandWriteData", "Expands data for FPGA write (adds padding bytes)"),

    # Simulation (reveals data format)
    ("FakeReadThread", "Simulation read thread -- generates test data"),
    ("ModifyFakeData", "Inserts known patterns into fake data buffers"),
    ("GenerateSimulationData", "Generates simulation data for analyzers"),

    # Data width and format
    ("ComputeTransferSize", "Computes USB bulk transfer buffer size"),
    ("AdjustTransferSize", "Adjusts transfer size for constraints"),

    # Pipe handlers (show what data types flow through disaggregator)
    ("RegisterPipeHandler", "Graph pipe handler registration"),

    # Channel configuration
    ("SetEnabledSmartCableDigitalChannels", "Smart cable channel enable"),
    ("EnableGpif", "Enables GPIF data interface between FX3 and FPGA"),

    # Capture configuration
    ("SetCaptureParameters", "Writes capture config to FPGA registers"),
    ("StartSampling", "Device-specific sampling start"),
]

# String patterns that reveal data interleaving format
INTERLEAVING_STRING_PATTERNS = [
    "DisaggregatorNode requires",
    "Cannot configure glitch filters",
    "Tried to configure glitch filter",
    "No valid analog and gpif width combo",
    "num_channels==0",
    "channel_count ==",
    "rawdata.size() * 8 == encoding_state.num_samples",
    "ContractReadData",
    "ExpandWriteData",
    "FakeReadThread",
    "ModifyFakeData",
    "Enabling GPIF",
    "EnableGpif",
    "AnalogDeserializer",
    "WaveformDeserializer",
    "digital channel cannot be enabled",
    "data expand algorithm failed",
    "Got start packet",
    "Expected start packet",
    "Received digital data",
    "HandleDigitalData",
    "Setting sampling config",
    "Digital Fifo overflow",
    "Analog Fifo overflow",
    "Master Fifo overflow",
    "Digital stream reset",
    "digital fsm error",
    "analog fsm error",
    "sample_count %",
    "config_id",
    "mSamplesPerChunk",
]

REPORT_FILENAME = "disaggregator-trace.txt"


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
    """Get all functions that call the given function."""
    callers = []
    if func is None:
        return callers
    entry = func.getEntryPoint()
    refs = getReferencesTo(entry)
    seen = set()
    for ref in refs:
        if ref.getReferenceType().isCall() or \
           ref.getReferenceType().isUnconditionalFlow():
            caller_addr = ref.getFromAddress()
            caller_func = getFunctionContaining(caller_addr)
            if caller_func is not None and \
               caller_func.getEntryPoint() not in seen:
                seen.add(caller_func.getEntryPoint())
                callers.append((caller_func, caller_addr))
    return callers


def get_callees(program, func):
    """Get all functions called by the given function."""
    callees = []
    if func is None:
        return callees
    body = func.getBody()
    listing = program.getListing()
    inst_iter = listing.getInstructions(body, True)
    seen = set()
    while inst_iter.hasNext():
        inst = inst_iter.next()
        mnemonic = inst.getMnemonicString()
        if mnemonic.startswith("CALL"):
            for i in range(inst.getNumOperands()):
                refs = inst.getOperandReferences(i)
                for ref in refs:
                    target = ref.getToAddress()
                    callee = getFunctionAt(target)
                    if callee is not None and target not in seen:
                        seen.add(target)
                        callees.append((callee, inst.getAddress()))
    return callees


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


def find_strings_matching(program, patterns):
    """Find defined strings matching any of the given patterns."""
    results = []
    for data in DefinedDataIterator.definedStrings(program):
        val = str(data.getValue())
        for pat in patterns:
            if pat.lower() in val.lower():
                results.append((data.getAddress(), val, pat))
                break
    return results


def try_extract_immediate_constants(program, func, max_instructions=300):
    """Extract immediate constants from a function."""
    constants = []
    if func is None:
        return constants
    body = func.getBody()
    listing = program.getListing()
    inst_iter = listing.getInstructions(body, True)
    count = 0
    while inst_iter.hasNext() and count < max_instructions:
        inst = inst_iter.next()
        count += 1
        mnemonic = inst.getMnemonicString()
        if mnemonic.startswith("MOV") or mnemonic.startswith("CMP") or \
           mnemonic.startswith("TEST") or mnemonic.startswith("AND") or \
           mnemonic.startswith("SHL") or mnemonic.startswith("SHR") or \
           mnemonic.startswith("ADD") or mnemonic.startswith("SUB") or \
           mnemonic.startswith("IMUL") or mnemonic.startswith("LEA"):
            for i in range(inst.getNumOperands()):
                op = inst.getDefaultOperandRepresentation(i)
                try:
                    if op.startswith("0x"):
                        val = int(op, 16)
                        if 1 <= val <= 0x1000000:
                            constants.append((val, inst.getAddress(),
                                              mnemonic, op,
                                              str(inst)))
                except (ValueError, AttributeError):
                    pass
    return constants


def find_rtti_typeinfo(program, typename):
    """Find RTTI typeinfo objects for a given type name string."""
    results = []
    for data in DefinedDataIterator.definedStrings(program):
        val = str(data.getValue())
        if typename in val:
            results.append((data.getAddress(), val))
            # Find what references this type name string (typeinfo objects)
            refs = getReferencesTo(data.getAddress())
            for ref in refs:
                results.append((ref.getFromAddress(),
                                "typeinfo ref from " + str(ref.getFromAddress())))
    return results


def analyze_struct_accesses(program, func, max_instructions=500):
    """Analyze memory access patterns in a function to infer struct layout.
    Looks for offset-based loads/stores that suggest struct field accesses."""
    accesses = []
    if func is None:
        return accesses
    body = func.getBody()
    listing = program.getListing()
    inst_iter = listing.getInstructions(body, True)
    count = 0
    while inst_iter.hasNext() and count < max_instructions:
        inst = inst_iter.next()
        count += 1
        mnemonic = inst.getMnemonicString()
        inst_str = str(inst)
        # Look for register+offset memory accesses like [reg + 0xNN]
        if "+" in inst_str and ("0x" in inst_str) and \
           (mnemonic.startswith("MOV") or mnemonic.startswith("LEA") or
            mnemonic.startswith("CMP")):
            accesses.append((inst.getAddress(), mnemonic, inst_str))
    return accesses


# ---------------------------------------------------------------------------
# Main script
# ---------------------------------------------------------------------------

def main():
    monitor = ConsoleTaskMonitor()
    program = currentProgram
    report_path = get_report_path()

    lines = []
    lines.append("=" * 78)
    lines.append("DISAGGREGATOR AND DATA INTERLEAVING TRACE")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: Locate disaggregator pipeline functions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: DISAGGREGATOR PIPELINE FUNCTIONS")
    lines.append("=" * 78)
    lines.append("")

    found_functions = {}

    for search_name, description in PIPELINE_FUNCTIONS:
        matches = find_functions_containing(program, search_name)
        if matches:
            found_functions[search_name] = []
            for m in matches:
                found_functions[search_name].append((m, description))
                lines.append("  [+] {} @ {}".format(
                    m.getName(), m.getEntryPoint()))
                lines.append("      Purpose: {}".format(description))
                lines.append("      Size: {} bytes".format(
                    m.getBody().getNumAddresses()))

                callers = get_callers(program, m)
                if callers:
                    lines.append("      Callers ({}):" .format(len(callers)))
                    for c, addr in callers[:10]:
                        lines.append("        <- {} @ {}".format(
                            c.getName(), c.getEntryPoint()))

                callees = get_callees(program, m)
                if callees:
                    lines.append("      Callees ({}):" .format(len(callees)))
                    for c, addr in callees[:10]:
                        lines.append("        -> {} @ {}".format(
                            c.getName(), c.getEntryPoint()))

                # Show strings referenced by this function
                str_refs = get_string_refs_in_function(program, m)
                relevant = [s for s in str_refs
                            if any(kw in s.lower()
                                   for kw in ["disaggregat", "channel",
                                              "glitch", "width", "analog",
                                              "digital", "gpif", "sample",
                                              "chunk", "offset", "stride",
                                              "contract", "expand",
                                              "interleav", "byte"])]
                if relevant:
                    lines.append("      Relevant strings in function:")
                    for s in relevant[:15]:
                        lines.append("        \"{}\"".format(s[:100]))

                lines.append("")
        else:
            lines.append("  [-] {} -- NOT FOUND".format(search_name))
            lines.append("      Expected: {}".format(description))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 2: Interleaving format strings
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 2: DATA INTERLEAVING FORMAT STRINGS")
    lines.append("=" * 78)
    lines.append("")

    format_strings = find_strings_matching(program,
                                           INTERLEAVING_STRING_PATTERNS)
    for addr, val, pattern in sorted(format_strings, key=lambda x: x[2]):
        lines.append("  [{:40s}] \"{}\"".format(pattern, val[:100]))
        lines.append("  {:42s} @ {}".format("", addr))
        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("  {:42s} Ref by: {} @ {}".format(
                    "", func.getName(), func.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 3: RTTI type analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 3: RTTI TYPE ANALYSIS (Disaggregator Types)")
    lines.append("=" * 78)
    lines.append("")

    rtti_types = [
        "N6Saleae5Graph25DisaggregatorSettingsDataE",
        "N6Saleae5Graph17DisaggregatorNodeE",
        "N6Saleae5Graph23DisaggregatorNodeState2E",
        "N6Saleae5Graph20DisaggregatorActions22ConfigureGlitchFiltersE",
    ]
    for typename in rtti_types:
        results = find_rtti_typeinfo(program, typename)
        if results:
            lines.append("  Type: {}".format(typename))
            for addr, desc in results:
                lines.append("    {} @ {}".format(desc[:80], addr))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 4: DisaggregatorSettingsData struct analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 4: DisaggregatorSettingsData STRUCT ANALYSIS")
    lines.append("(Analyzing GetDisaggregatorSettings to infer struct fields)")
    lines.append("=" * 78)
    lines.append("")

    get_settings = find_functions_containing(program,
                                             "GetDisaggregatorSettings")
    for func in get_settings:
        lines.append("  Function: {} @ {}".format(func.getName(),
                                                    func.getEntryPoint()))
        lines.append("  Size: {} bytes".format(
            func.getBody().getNumAddresses()))
        lines.append("")

        # Extract immediate constants (potential struct field values)
        constants = try_extract_immediate_constants(program, func, 500)
        if constants:
            lines.append("  Immediate constants (potential field offsets/values):")
            for val, addr, mnemonic, op, inst_str in constants:
                lines.append("    {} = {} (0x{:x}) @ {} -- {}".format(
                    mnemonic, val, val, addr, inst_str[:60]))
            lines.append("")

        # Analyze struct-like accesses
        accesses = analyze_struct_accesses(program, func, 500)
        if accesses:
            lines.append("  Memory access patterns (struct field accesses):")
            for addr, mnemonic, inst_str in accesses[:40]:
                lines.append("    @ {} {} {}".format(addr, mnemonic,
                                                      inst_str[:70]))
            lines.append("")

        # Show all strings referenced
        str_refs = get_string_refs_in_function(program, func)
        if str_refs:
            lines.append("  All strings referenced:")
            for s in str_refs:
                lines.append("    \"{}\"".format(s[:100]))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 5: ContractReadData / ExpandWriteData analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 5: DATA FORMAT TRANSFORMATION (Contract/Expand)")
    lines.append("(ContractReadData removes padding, ExpandWriteData adds it)")
    lines.append("=" * 78)
    lines.append("")

    for func_name in ["ContractReadData", "ExpandWriteData"]:
        funcs = find_functions_containing(program, func_name)
        for func in funcs:
            lines.append("  Function: {} @ {}".format(func.getName(),
                                                        func.getEntryPoint()))
            lines.append("  Size: {} bytes".format(
                func.getBody().getNumAddresses()))

            constants = try_extract_immediate_constants(program, func, 300)
            if constants:
                lines.append("  Constants (bus widths, byte counts, masks):")
                small = [(v, a, m, o, i) for v, a, m, o, i in constants
                         if v <= 256]
                for val, addr, mnemonic, op, inst_str in small[:20]:
                    lines.append("    {} = {} (0x{:x}) @ {} -- {}".format(
                        mnemonic, val, val, addr, inst_str[:60]))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 6: FakeReadThread analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 6: FakeReadThread ANALYSIS (Simulation Data Format)")
    lines.append("(FakeReadThread generates data in the same format as HW)")
    lines.append("=" * 78)
    lines.append("")

    fake_funcs = find_functions_containing(program, "FakeReadThread")
    for func in fake_funcs:
        lines.append("  Function: {} @ {}".format(func.getName(),
                                                    func.getEntryPoint()))
        lines.append("  Size: {} bytes".format(
            func.getBody().getNumAddresses()))

        str_refs = get_string_refs_in_function(program, func)
        if str_refs:
            lines.append("  Strings:")
            for s in str_refs:
                lines.append("    \"{}\"".format(s[:100]))

        constants = try_extract_immediate_constants(program, func, 500)
        if constants:
            lines.append("  Constants:")
            for val, addr, mnemonic, op, inst_str in constants[:30]:
                lines.append("    {} = {} (0x{:x}) @ {}".format(
                    mnemonic, val, val, addr))

        callees = get_callees(program, func)
        if callees:
            lines.append("  Callees:")
            for c, addr in callees[:20]:
                lines.append("    -> {} @ {}".format(
                    c.getName(), c.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 7: ModifyFakeData analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 7: ModifyFakeData ANALYSIS")
    lines.append("(Reveals per-channel data placement within USB buffer)")
    lines.append("=" * 78)
    lines.append("")

    modify_funcs = find_functions_containing(program, "ModifyFakeData")
    for func in modify_funcs:
        lines.append("  Function: {} @ {}".format(func.getName(),
                                                    func.getEntryPoint()))
        lines.append("  Size: {} bytes".format(
            func.getBody().getNumAddresses()))

        str_refs = get_string_refs_in_function(program, func)
        if str_refs:
            lines.append("  Strings:")
            for s in str_refs:
                lines.append("    \"{}\"".format(s[:100]))

        constants = try_extract_immediate_constants(program, func, 300)
        if constants:
            lines.append("  Constants (offsets, masks, channel indices):")
            for val, addr, mnemonic, op, inst_str in constants[:20]:
                lines.append("    {} = {} (0x{:x}) @ {} -- {}".format(
                    mnemonic, val, val, addr, inst_str[:60]))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 8: Pipe handler registration analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 8: DISAGGREGATOR PIPE HANDLERS")
    lines.append("(What data types flow in/out of DisaggregatorNode)")
    lines.append("=" * 78)
    lines.append("")

    pipe_handlers = find_functions_containing(program,
                                              "RegisterPipeHandler")
    disagg_pipes = [f for f in pipe_handlers
                    if "DisaggregatorNode" in f.getName()]
    lines.append("  DisaggregatorNode pipe handler registrations:")
    for f in disagg_pipes:
        name = f.getName()
        # Extract template params to identify data types
        if "OwnedSpanIhE" in name:
            dtype = "OwnedSpan<uint8_t> (raw bytes IN)"
        elif "DisaggregatorSettingsData" in name:
            dtype = "DisaggregatorSettingsData (settings IN)"
        elif "CaptureCompleteData" in name:
            dtype = "CaptureCompleteData (end signal IN)"
        elif "VectorDataIhE" in name:
            dtype = "VectorData<uint8_t> (raw bytes)"
        elif "VectorDataIsE" in name:
            dtype = "VectorData<int16_t> (analog samples)"
        elif "DigitalData" in name:
            dtype = "DigitalData (digital channel data)"
        else:
            dtype = "(unknown type)"
        lines.append("    {} -- {}".format(dtype, f.getEntryPoint()))
    lines.append("")

    # Also check what DisaggregatorNode emits
    # Look for EmitPipe or similar output registration
    emit_funcs = find_functions_containing(program, "EmitData")
    disagg_emits = [f for f in emit_funcs
                    if "Disaggregator" in f.getName()]
    if disagg_emits:
        lines.append("  DisaggregatorNode emit registrations:")
        for f in disagg_emits:
            lines.append("    {} @ {}".format(f.getName(),
                                               f.getEntryPoint()))
    lines.append("")

    # ------------------------------------------------------------------
    # Part 9: GPIF width analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 9: GPIF WIDTH AND BUS FORMAT")
    lines.append("(GPIF II interface between FX3 and FPGA)")
    lines.append("=" * 78)
    lines.append("")

    gpif_funcs = find_functions_containing(program, "EnableGpif")
    for func in gpif_funcs:
        lines.append("  Function: {} @ {}".format(func.getName(),
                                                    func.getEntryPoint()))
        str_refs = get_string_refs_in_function(program, func)
        if str_refs:
            lines.append("  Strings:")
            for s in str_refs:
                lines.append("    \"{}\"".format(s[:100]))
        constants = try_extract_immediate_constants(program, func, 200)
        if constants:
            lines.append("  Constants:")
            for val, addr, mnemonic, op, inst_str in constants[:15]:
                lines.append("    {} = {} (0x{:x}) @ {}".format(
                    mnemonic, val, val, addr))
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
        println("Printing to console instead:")
        println(report)

    println("")
    println("=== SUMMARY ===")
    println("Pipeline functions found: {}/{}".format(
        len(found_functions), len(PIPELINE_FUNCTIONS)))
    println("Interleaving format strings found: {}".format(
        len(format_strings)))
    println("Report: {}".format(report_path))


# Entry point
if __name__ == "__main__":
    main()
else:
    main()
