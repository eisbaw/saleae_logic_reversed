# TraceDigitalCapture.py -- Ghidra script to trace the digital capture data
# pipeline in libgraph_server_shared.so.
#
# This script maps the full data path from USB bulk transfer reception through
# deserialization, disaggregation, and into the DigitalStore. It also extracts
# the capture configuration parameters and identifies the RLE encoding used
# for digital data storage.
#
# Target functions and classes:
#   - LogicDeviceNode::OnDataBuffer -- entry point from USB
#   - LogicDeviceNode::HandleStartCapture -- capture setup
#   - Logic2FpgaDevice::SetCaptureParameters -- FPGA register config
#   - LogicDevice::ComputeTransferSize -- USB transfer size computation
#   - LogicDevice::StartSampling -- initiates capture
#   - DisaggregatorNode -- splits interleaved USB data into per-channel streams
#   - DigitalCreatorNode -- creates DigitalData from raw bytes
#   - Saleae::Digital::Detail::AppendRle -- RLE encoding for digital data
#   - FpgaRegisters::CalculateDigitalConfigurationAddress_* -- FPGA config
#   - LogicDevice::ModifyFakeData -- simulation/test data (reveals format)
#
# Output: re/ghidra/reports/digital-capture-trace.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceDigitalCapture.py
#
# @author  saleae_reverse project
# @category DigitalCapture
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

# Key functions in the digital capture data pipeline, ordered from USB
# reception through to storage. Each entry: (search_name, description)
PIPELINE_FUNCTIONS = [
    # Capture initiation
    ("HandleStartCapture", "Capture start handler - sets up callbacks"),
    ("SetCaptureParameters", "Writes capture config to FPGA registers"),
    ("ComputeTransferSize", "Computes USB bulk transfer buffer size"),
    ("StartSampling", "Starts the actual sampling on the device"),
    ("SetBankPower", "Enables IO bank power before capture"),

    # USB data reception
    ("ReadStream_QueueBuffersThread", "Async bulk transfer queue thread"),
    ("on_libusb_transfer_complete", "Async transfer completion callback"),
    ("OnDataBuffer", "Entry point for received USB data buffers"),

    # Data disaggregation (interleaved -> per-channel)
    ("DisaggregatorNode", "Splits interleaved USB data by channel"),
    ("HandleSamplingConfigurationData", "Receives sampling config"),

    # Digital data creation
    ("DigitalCreatorNode", "Creates DigitalData from raw byte vectors"),
    ("HandleDigitalData", "Processes digital data in trigger node"),
    ("MakeDigitalData", "Factory for DigitalData objects"),

    # Digital data encoding (RLE)
    ("AppendRle", "Run-length encoding for digital transitions"),
    ("BuildDataAndLookupTable", "Builds lookup table for RLE data"),
    ("DigitalInternals", "Internal representation of digital data"),

    # FPGA register configuration
    ("CalculateDigitalConfigurationAddress_DigitalDerived",
     "FPGA register address for digital-derived config"),
    ("CalculateDigitalConfigurationAddress_SmartCable",
     "FPGA register address for smart cable config"),
    ("SetDigitalDerivedVoltageThresholds",
     "Sets voltage thresholds for digital channels"),
    ("EnableSmartCablePorts", "Enables smart cable digital ports"),

    # Transfer size and packet format
    ("AdjustTransferSize", "Adjusts USB transfer size"),
    ("ModifyFakeData", "Simulation data format (reveals packet structure)"),
    ("FakeReadThread", "Simulation read thread (reveals data flow)"),

    # Device-specific implementations
    ("LogicGraduateDevice", "Logic 8 device implementation"),
    ("LogicProfessionalDevice", "Logic Pro 8/16 device implementation"),
    ("LogicStudentDevice", "Logic 4 device implementation"),

    # Performance and sample rate
    ("GetDigitalOnlyPerforamnceOptions", "Digital-only sample rate options"),
    ("SetDigitalOnlyPerformanceOption", "Set digital sample rate"),
    ("DigitalSampleRateHz", "Get digital sample rate in Hz"),
    ("GetInterfaceMaximumBitRate", "Max USB bitrate for device"),
]

# String patterns that reveal digital capture data format
FORMAT_STRING_PATTERNS = [
    "SamplesPerPacketMultiple_WithStats",
    "MaxSamplesPerPacket_Altera",
    "MinSamplesPerPacket_Altera",
    "MaxSamplesPerPacket_Xilinx",
    "MinSamplesPerPacket_Xilinx",
    "mSamplesPerChunk",
    "Digital Fifo overflow",
    "Analog Fifo overflow",
    "Master Fifo overflow",
    "Digital stream reset",
    "Digital stream starting",
    "Got start packet",
    "Expected start packet",
    "Received digital data",
    "starting digital channel",
    "digital fsm error",
    "channel_count ==",
    "sample_count %",
    "config_id",
    "Setting sampling config",
    "Transfer size is",
    "transfer_size=",
    "ideal transfer size",
    "SALEAE_USB_TRANSFER_LIMIT",
    "USB Transfer Error",
    "overflow",
]

REPORT_FILENAME = "digital-capture-trace.txt"


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
                break  # Only match once per string
    return results


def try_extract_immediate_constants(program, func, max_instructions=200):
    """Extract immediate constants from a function that might be
    packet sizes, buffer sizes, or sample counts."""
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
        if mnemonic.startswith("MOV") or mnemonic.startswith("CMP"):
            for i in range(inst.getNumOperands()):
                op = inst.getDefaultOperandRepresentation(i)
                try:
                    if op.startswith("0x"):
                        val = int(op, 16)
                        # Filter for "interesting" sizes
                        if 8 <= val <= 0x100000:
                            constants.append((val, inst.getAddress(),
                                              mnemonic, op))
                except (ValueError, AttributeError):
                    pass
    return constants


# ---------------------------------------------------------------------------
# Main script
# ---------------------------------------------------------------------------

def main():
    monitor = ConsoleTaskMonitor()
    program = currentProgram
    report_path = get_report_path()

    lines = []
    lines.append("=" * 78)
    lines.append("DIGITAL CAPTURE DATA PIPELINE TRACE")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: Locate pipeline functions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: DIGITAL CAPTURE PIPELINE FUNCTIONS")
    lines.append("=" * 78)
    lines.append("")

    found_functions = {}  # name -> list of (func, description)

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

                # Show callers
                callers = get_callers(program, m)
                if callers:
                    lines.append("      Callers ({}):" .format(len(callers)))
                    for c, addr in callers[:10]:
                        lines.append("        <- {} @ {}".format(
                            c.getName(), c.getEntryPoint()))

                # Show callees
                callees = get_callees(program, m)
                if callees:
                    lines.append("      Callees ({}):" .format(len(callees)))
                    for c, addr in callees[:10]:
                        lines.append("        -> {} @ {}".format(
                            c.getName(), c.getEntryPoint()))

                lines.append("")
        else:
            lines.append("  [-] {} -- NOT FOUND".format(search_name))
            lines.append("      Expected: {}".format(description))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 2: Data format strings
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 2: DATA FORMAT STRINGS")
    lines.append("(Strings that reveal packet format, sizes, and encoding)")
    lines.append("=" * 78)
    lines.append("")

    format_strings = find_strings_matching(program, FORMAT_STRING_PATTERNS)
    for addr, val, pattern in sorted(format_strings, key=lambda x: x[2]):
        lines.append("  [{:30s}] \"{}\"".format(pattern, val[:100]))
        lines.append("  {:32s} @ {}".format("", addr))

        # Find which functions reference this string
        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("  {:32s} Referenced by: {} @ {}".format(
                    "", func.getName(), func.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 3: ComputeTransferSize analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 3: TRANSFER SIZE AND PACKET SIZE CONSTANTS")
    lines.append("(Constants from ComputeTransferSize and related functions)")
    lines.append("=" * 78)
    lines.append("")

    size_funcs = ["ComputeTransferSize", "AdjustTransferSize",
                  "SetCaptureParameters", "ModifyFakeData"]
    for sf_name in size_funcs:
        matches = find_functions_containing(program, sf_name)
        for m in matches:
            constants = try_extract_immediate_constants(program, m)
            if constants:
                lines.append("  {} @ {}:".format(m.getName(),
                                                  m.getEntryPoint()))
                for val, addr, mnemonic, op in constants:
                    lines.append("    {} {} = {} (0x{:x}) @ {}".format(
                        mnemonic, op, val, val, addr))
                lines.append("")

    # ------------------------------------------------------------------
    # Part 4: FpgaConstants assertions
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 4: FPGA CONSTANTS AND ASSERTIONS")
    lines.append("(Constraints on packet sizes and sample counts)")
    lines.append("=" * 78)
    lines.append("")

    fpga_patterns = [
        "FpgaConstants::",
        "MsoConstants::",
        "SamplesPerPacket",
        "SamplesPerChunk",
        "TriggerBuffer",
    ]
    fpga_strings = find_strings_matching(program, fpga_patterns)
    for addr, val, pattern in sorted(fpga_strings, key=lambda x: x[1]):
        lines.append("  \"{}\"".format(val[:120]))
        lines.append("    @ {}".format(addr))
        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("    Referenced by: {} @ {}".format(
                    func.getName(), func.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 5: Digital data graph pipeline
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 5: DIGITAL DATA GRAPH PIPELINE")
    lines.append("(Data flow through the processing graph)")
    lines.append("=" * 78)
    lines.append("")

    # Trace pipe handler registrations
    pipe_handlers = find_functions_containing(program,
                                              "RegisterPipeHandler")
    digital_pipes = [f for f in pipe_handlers if "Digital" in f.getName()]

    lines.append("  Digital pipe handler registrations:")
    for f in digital_pipes:
        lines.append("    {} @ {}".format(f.getName()[:100],
                                           f.getEntryPoint()))
    lines.append("")

    # Trace VectorData<uint8_t> pipes (raw USB data)
    vector_pipes = [f for f in pipe_handlers
                    if "VectorDataIhE" in f.getName()]  # h = uint8_t
    lines.append("  VectorData<uint8_t> pipe handlers (raw USB bytes):")
    for f in vector_pipes:
        lines.append("    {} @ {}".format(f.getName()[:100],
                                           f.getEntryPoint()))
    lines.append("")

    # Trace OwnedSpan<uint8_t> pipes (disaggregated data)
    span_pipes = [f for f in pipe_handlers
                  if "OwnedSpanIhE" in f.getName()]
    lines.append("  OwnedSpan<uint8_t> pipe handlers (disaggregated data):")
    for f in span_pipes:
        lines.append("    {} @ {}".format(f.getName()[:100],
                                           f.getEntryPoint()))
    lines.append("")

    # ------------------------------------------------------------------
    # Part 6: RLE encoding analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 6: RLE (RUN-LENGTH ENCODING) ANALYSIS")
    lines.append("=" * 78)
    lines.append("")

    rle_funcs = find_functions_containing(program, "AppendRle")
    if rle_funcs:
        for f in rle_funcs:
            lines.append("  {} @ {}".format(f.getName(),
                                             f.getEntryPoint()))
            lines.append("    Size: {} bytes".format(
                f.getBody().getNumAddresses()))

            # Extract constants to understand RLE encoding
            constants = try_extract_immediate_constants(program, f, 100)
            if constants:
                lines.append("    Constants:")
                for val, addr, mnemonic, op in constants:
                    lines.append("      {} {} = {} (0x{:x}) @ {}".format(
                        mnemonic, op, val, val, addr))

            # Show callers
            callers = get_callers(program, f)
            lines.append("    Callers ({}):" .format(len(callers)))
            for c, addr in callers[:10]:
                lines.append("      <- {} @ {}".format(
                    c.getName(), c.getEntryPoint()))

            lines.append("")
    else:
        lines.append("  [!] No AppendRle function found")
        lines.append("")

    # Also look at BuildDataAndLookupTable
    bld_funcs = find_functions_containing(program, "BuildDataAndLookupTable")
    if bld_funcs:
        for f in bld_funcs:
            lines.append("  {} @ {}".format(f.getName(),
                                             f.getEntryPoint()))
            lines.append("    Size: {} bytes".format(
                f.getBody().getNumAddresses()))

            constants = try_extract_immediate_constants(program, f, 200)
            if constants:
                lines.append("    Constants (potential bit masks, sizes):")
                for val, addr, mnemonic, op in constants[:20]:
                    lines.append("      {} {} = {} (0x{:x}) @ {}".format(
                        mnemonic, op, val, val, addr))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 7: Overflow and error detection
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 7: OVERFLOW AND ERROR DETECTION")
    lines.append("=" * 78)
    lines.append("")

    overflow_patterns = [
        "Digital Fifo overflow",
        "Analog Fifo overflow",
        "Master Fifo overflow",
        "digital fsm error",
        "Digital stream reset",
    ]
    overflow_strings = find_strings_matching(program, overflow_patterns)
    for addr, val, pattern in overflow_strings:
        lines.append("  \"{}\" @ {}".format(val[:100], addr))
        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("    Referenced by: {} @ {}".format(
                    func.getName(), func.getEntryPoint()))
                # Check what other strings this function references
                str_refs = get_string_refs_in_function(program, func)
                relevant = [s for s in str_refs
                            if any(kw in s.lower()
                                   for kw in ["overflow", "error", "reset",
                                              "packet", "sample", "fifo"])]
                if relevant:
                    lines.append("    Other relevant strings in function:")
                    for s in relevant[:5]:
                        lines.append("      \"{}\"".format(s[:80]))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 8: Device-specific StartSampling
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 8: DEVICE-SPECIFIC StartSampling IMPLEMENTATIONS")
    lines.append("=" * 78)
    lines.append("")

    sampling_funcs = find_functions_containing(program, "StartSampling")
    for f in sampling_funcs:
        lines.append("  {} @ {}".format(f.getName(), f.getEntryPoint()))
        lines.append("    Size: {} bytes".format(
            f.getBody().getNumAddresses()))

        # Extract constants that might be register addresses
        constants = try_extract_immediate_constants(program, f, 300)
        interesting = [(v, a, m, o) for v, a, m, o in constants
                       if v < 0x10000]  # Register-sized values
        if interesting:
            lines.append("    Constants (potential register values):")
            for val, addr, mnemonic, op in interesting[:30]:
                lines.append("      {} = {} (0x{:x}) @ {}".format(
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

    # Summary
    println("")
    println("=== SUMMARY ===")
    println("Pipeline functions found: {}/{}".format(
        len(found_functions), len(PIPELINE_FUNCTIONS)))
    println("Data format strings found: {}".format(len(format_strings)))
    println("RLE functions found: {}".format(len(rle_funcs)))
    println("Report: {}".format(report_path))


# Entry point
if __name__ == "__main__":
    main()
else:
    main()
