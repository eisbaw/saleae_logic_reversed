# TraceCaptureSequence.py -- Ghidra script to trace the capture start/stop
# command sequence in libgraph_server_shared.so.
#
# This script maps the complete capture lifecycle:
#   1. IPC message handling (StartCapture, StopCapture)
#   2. Capture parameter configuration (SetCaptureParameters)
#   3. FX3 start/stop commands (SendStartCommand, SendStopCommand)
#   4. USB stream management (TransferQueue, BulkReadStream)
#   5. Error handling and recovery paths
#
# Target functions:
#   - LogicDeviceNode::HandleStartCapture -- IPC entry point
#   - LogicDeviceNode::HandleStopCapture (via StopCapture registration)
#   - Logic2FpgaDevice::SetCaptureParameters -- FPGA config write
#   - LogicDevice::StartSampling (+ device-specific overrides)
#   - SendStartCommand / SendStopCommand -- FX3 commands
#   - FpgaDeviceFeatures::WriteConfigurationData -- config data bulk write
#   - FpgaDeviceFeatures::WriteRegisters -- individual register writes
#   - LogicDevice::ComputeTransferSize -- USB transfer size
#   - ReadStream_QueueBuffersThread -- async USB read loop
#   - StopBulkReadStream -- stop capture data flow
#
# Output: re/ghidra/reports/capture-sequence-trace.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceCaptureSequence.py
#
# @author  saleae_reverse project
# @category CaptureSequence
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

# Functions in the capture start sequence, in execution order
START_SEQUENCE_FUNCTIONS = [
    # IPC entry
    ("HandleStartCapture", "IPC entry: validates state, registers callbacks"),
    ("HandleStopCapture", "IPC entry: initiates capture stop"),

    # Device acquisition
    ("HandleAcquireLogicDeviceRequest", "Acquires device for a session"),
    ("HandleReleaseDeviceRequest", "Releases device from session"),

    # Configuration (before capture)
    ("HandleSetSampleRate", "Sets digital/analog sample rates"),
    ("HandleEnableChannels", "Enables/disables channels"),
    ("HandleSetDigitalVoltageThreshold", "Sets digital threshold voltage"),
    ("HandleSetAdcFullScaleGain", "Sets ADC gain"),
    ("HandleSetLed", "Controls device LED"),
    ("GetDefaultLogicDeviceConfiguration", "Gets default config for device"),
    ("GetDeviceSettings", "Returns device settings struct"),
    ("GetDeviceConstants", "Returns device constants"),

    # Capture parameter programming
    ("SetCaptureParameters", "Writes capture config to FPGA registers"),
    ("WriteConfigurationData", "Bulk-writes config data via EP1 OUT"),
    ("WriteRegisters", "Writes individual FPGA registers"),
    ("WriteRegisterBit", "Writes single FPGA register bit"),
    ("ReadRegisters", "Reads FPGA registers via EP1 IN"),

    # Bank power and PLL
    ("SetBankPower", "Enables IO bank power for channels"),

    # Transfer size computation
    ("ComputeTransferSize", "Computes USB bulk transfer buffer size"),
    ("AdjustTransferSize", "Adjusts transfer size"),

    # Device-specific start implementations
    ("StartSampling", "Starts sampling (virtual, per device type)"),
    ("SendStartCommand", "Sends start command to FX3"),
    ("SendStopCommand", "Sends stop command to FX3"),

    # Connection test
    ("TestFirmwareResponsivnessAndReseed", "Tests FX3 responsiveness"),
    ("ReseedDevice", "Reseeds/tests device endpoint"),
    ("IsDeviceResponsive", "Checks device responsiveness"),

    # USB stream management
    ("ReadStream_QueueBuffersThread", "Async bulk read thread"),
    ("on_libusb_transfer_complete", "Transfer completion callback"),
    ("StopBulkReadStream", "Stops bulk read stream"),

    # Data reception in graph
    ("OnDataBuffer", "Receives raw USB data buffer"),
    ("GetDisaggregatorSettings", "Gets disaggregator channel layout"),
    ("HandleSamplingConfigurationData", "Receives sampling config"),

    # Capture completion
    ("CaptureCompleteData", "End-of-capture signal through graph"),

    # Error handling
    ("HandleDeviceError", "Handles device error during capture"),
    ("OnWrite", "Write completion callback"),

    # Data format helpers
    ("ExpandWriteData", "Formats data for EP1 OUT commands"),
    ("ContractReadData", "Parses data from EP1 IN responses"),
]

# String patterns specific to capture sequence
CAPTURE_STRING_PATTERNS = [
    # Start sequence
    "Cannot start capture",
    "start capture",
    "device is not currently acquired",
    "session id already in use",
    "setting capture settings",
    "reading back capture settings",
    "Capture parameter failed",
    "Failed to set capture settings",
    "Sending start command",
    "startCommand",
    "Start command error",
    "Start command failed",
    "StartSampling failed",
    "failed to write capture parameters",
    "bank enabled",
    "PLL is locked",
    "PLL IS NOT LOCKED",

    # Transfer size
    "ideal transfer size",
    "Transfer size is",
    "capping transfer size",
    "SALEAE_USB_TRANSFER_LIMIT",
    "Starting bulk read thread",
    "BulkReadThread:",

    # Stop sequence
    "stop capture",
    "COMMAND_STOP_STREAM",
    "Failed to send stop command",
    "Failed to send STOP_STREAM",
    "Stopping bulk read stream",
    "stopping stream",
    "Done with usb stream",
    "Cancelling transfers",
    "Cancelling",

    # Configuration
    "Setting sampling config",
    "WriteConfigurationData",
    "FPGA ReadRegister",
    "FPGA WriteRegister",

    # Errors during capture
    "ReadStream Error",
    "USB ReadStream Error",
    "USB Transfer Error",
    "libusb_bulk_transfer",
    "Device error",
    "Device is not responsive",
    "Device unresponsive",

    # Configuration constraints
    "mSamplesPerChunk",
    "SamplesPerPacketMultiple_WithStats",
    "PreTriggerLength",
    "PostTriggerLength",
    "TriggerBuffer",
    "sample_length",
]

# Sample rate and performance option strings
SAMPLE_RATE_PATTERNS = [
    "GetDigitalOnlyPerforamnceOptions",
    "GetAnalogBasicPerformanceOptions",
    "GetAnalogAdvancedPerformanceOptions",
    "SetDigitalOnlyPerformanceOption",
    "set sample rate",
    "sample rate not found",
    "digital sample rate is not valid",
    "analog sample rate is not valid",
    "GetInterfaceMaximumBitRate",
    "SampleRateOption",
]

REPORT_FILENAME = "capture-sequence-trace.txt"


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
                break
    return results


def try_extract_immediate_constants(program, func, max_instructions=300):
    """Extract immediate constants from a function that might be
    register addresses, command bytes, or buffer sizes."""
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
           mnemonic.startswith("LEA") or mnemonic.startswith("AND") or \
           mnemonic.startswith("OR") or mnemonic.startswith("TEST"):
            for i in range(inst.getNumOperands()):
                op = inst.getDefaultOperandRepresentation(i)
                try:
                    if op.startswith("0x"):
                        val = int(op, 16)
                        if 0 <= val <= 0x1000000:
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
    lines.append("CAPTURE START/STOP COMMAND SEQUENCE TRACE")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: Capture sequence functions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: CAPTURE SEQUENCE FUNCTIONS")
    lines.append("=" * 78)
    lines.append("")

    found_functions = {}

    for search_name, description in START_SEQUENCE_FUNCTIONS:
        matches = find_functions_containing(program, search_name)
        if matches:
            found_functions[search_name] = []
            for m in matches:
                found_functions[search_name].append((m, description))
                lines.append("  [+] {} @ {}".format(
                    m.getName()[:120], m.getEntryPoint()))
                lines.append("      Purpose: {}".format(description))
                lines.append("      Size: {} bytes".format(
                    m.getBody().getNumAddresses()))

                callers = get_callers(program, m)
                if callers:
                    lines.append("      Callers ({}):" .format(len(callers)))
                    for c, addr in callers[:8]:
                        lines.append("        <- {} @ {}".format(
                            c.getName()[:80], c.getEntryPoint()))

                callees = get_callees(program, m)
                if callees:
                    lines.append("      Callees ({}):" .format(len(callees)))
                    for c, addr in callees[:8]:
                        lines.append("        -> {} @ {}".format(
                            c.getName()[:80], c.getEntryPoint()))

                # Show strings referenced by this function
                str_refs = get_string_refs_in_function(program, m)
                capture_strs = [s for s in str_refs
                                if any(kw in s.lower()
                                       for kw in ["capture", "start",
                                                   "stop", "sample",
                                                   "error", "failed",
                                                   "command", "config",
                                                   "register", "bank",
                                                   "pll", "transfer",
                                                   "stream", "fpga"])]
                if capture_strs:
                    lines.append("      Capture-related strings:")
                    for s in capture_strs[:10]:
                        lines.append("        \"{}\"".format(s[:100]))

                lines.append("")
        else:
            lines.append("  [-] {} -- NOT FOUND".format(search_name))
            lines.append("      Expected: {}".format(description))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 2: Capture string analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 2: CAPTURE SEQUENCE STRINGS")
    lines.append("=" * 78)
    lines.append("")

    capture_strings = find_strings_matching(program, CAPTURE_STRING_PATTERNS)
    for addr, val, pattern in sorted(capture_strings, key=lambda x: x[2]):
        lines.append("  [{:35s}] \"{}\"".format(pattern[:35], val[:100]))
        lines.append("  {:37s} @ {}".format("", addr))

        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("  {:37s} Referenced by: {} @ {}".format(
                    "", func.getName()[:60], func.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 3: Start command analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 3: START/STOP COMMAND ANALYSIS")
    lines.append("(Constants and call patterns in SendStartCommand/SendStopCommand)")
    lines.append("=" * 78)
    lines.append("")

    for cmd_name in ["SendStartCommand", "SendStopCommand",
                      "SetCaptureParameters", "WriteConfigurationData",
                      "WriteRegisters", "WriteRegisterBit"]:
        matches = find_functions_containing(program, cmd_name)
        for m in matches:
            lines.append("  {} @ {}".format(m.getName()[:100],
                                             m.getEntryPoint()))
            lines.append("    Size: {} bytes".format(
                m.getBody().getNumAddresses()))

            constants = try_extract_immediate_constants(program, m)
            if constants:
                lines.append("    Constants (potential command bytes, "
                             "register addrs, sizes):")
                for val, addr, mnemonic, op in constants[:40]:
                    annotation = ""
                    if val == 0x40 or val == 0xC0:
                        annotation = " <- USB vendor request type"
                    elif val == 0xA0:
                        annotation = " <- Cypress FW upload bRequest"
                    elif val == 0xAA:
                        annotation = " <- FPGA readback magic"
                    elif val == 62:
                        annotation = " <- USB 2.0 config data limit"
                    elif val == 509:
                        annotation = " <- USB 3.0 config data limit"
                    elif val == 0x01:
                        annotation = " <- EP1 OUT?"
                    elif val == 0x81:
                        annotation = " <- EP1 IN?"
                    elif val == 0x82:
                        annotation = " <- EP2 IN (capture data)?"
                    lines.append("      {} = {} (0x{:x}) @ {}{}".format(
                        mnemonic, val, val, addr, annotation))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 4: Sample rate configuration
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 4: SAMPLE RATE AND PERFORMANCE OPTIONS")
    lines.append("=" * 78)
    lines.append("")

    rate_strings = find_strings_matching(program, SAMPLE_RATE_PATTERNS)
    for addr, val, pattern in sorted(rate_strings, key=lambda x: x[2]):
        lines.append("  [{:40s}] \"{}\"".format(pattern[:40], val[:100]))
        lines.append("  {:42s} @ {}".format("", addr))

        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("  {:42s} Referenced by: {} @ {}".format(
                    "", func.getName()[:60], func.getEntryPoint()))
        lines.append("")

    # Performance option functions
    for opt_name in ["GetDigitalOnlyPerforamnceOptions",
                     "GetAnalogBasicPerformanceOptions",
                     "GetAnalogAdvancedPerformanceOptions",
                     "GetInterfaceMaximumBitRate"]:
        matches = find_functions_containing(program, opt_name)
        for m in matches:
            lines.append("  {} @ {}".format(m.getName()[:100],
                                             m.getEntryPoint()))
            lines.append("    Size: {} bytes".format(
                m.getBody().getNumAddresses()))

            constants = try_extract_immediate_constants(program, m, 200)
            rate_constants = [(v, a, mn, op) for v, a, mn, op in constants
                              if v >= 1000]  # Likely Hz values
            if rate_constants:
                lines.append("    Potential sample rate constants:")
                for val, addr, mnemonic, op in rate_constants[:20]:
                    if val >= 1000000:
                        lines.append("      {} = {} ({} MHz) @ {}".format(
                            mnemonic, val, val / 1000000.0, addr))
                    elif val >= 1000:
                        lines.append("      {} = {} ({} kHz) @ {}".format(
                            mnemonic, val, val / 1000.0, addr))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 5: Device-specific StartSampling
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 5: DEVICE-SPECIFIC StartSampling IMPLEMENTATIONS")
    lines.append("=" * 78)
    lines.append("")

    for device_class in ["LogicDevice", "LogicGraduateDevice",
                         "LogicProfessionalDevice", "LogicStudentDevice",
                         "Logic16Device"]:
        sampling_funcs = [f for f in find_functions_containing(
            program, "StartSampling") if device_class in f.getName()]
        for f in sampling_funcs:
            lines.append("  {} @ {}".format(f.getName(), f.getEntryPoint()))
            lines.append("    Size: {} bytes".format(
                f.getBody().getNumAddresses()))

            callees = get_callees(program, f)
            if callees:
                lines.append("    Call sequence:")
                for c, addr in callees[:20]:
                    lines.append("      -> {} @ {}".format(
                        c.getName()[:80], addr))

            constants = try_extract_immediate_constants(program, f)
            if constants:
                lines.append("    Constants:")
                for val, addr, mnemonic, op in constants[:20]:
                    lines.append("      {} = {} (0x{:x}) @ {}".format(
                        mnemonic, val, val, addr))

            str_refs = get_string_refs_in_function(program, f)
            if str_refs:
                lines.append("    String references:")
                for s in str_refs[:10]:
                    lines.append("      \"{}\"".format(s[:100]))

            lines.append("")

    # ------------------------------------------------------------------
    # Part 6: USB control transfer parameters
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 6: USB CONTROL TRANSFER AND BULK WRITE ANALYSIS")
    lines.append("=" * 78)
    lines.append("")

    for usb_func in ["SendControlTransfer", "BulkWrite", "BulkRead"]:
        matches = find_functions_containing(program, usb_func)
        for m in matches:
            lines.append("  {} @ {}".format(m.getName()[:100],
                                             m.getEntryPoint()))
            lines.append("    Size: {} bytes".format(
                m.getBody().getNumAddresses()))

            callers = get_callers(program, m)
            capture_callers = [c for c, a in callers
                               if any(kw in c.getName().lower()
                                      for kw in ["start", "stop", "config",
                                                  "capture", "register",
                                                  "command", "sampling",
                                                  "bank", "fpga"])]
            if capture_callers:
                lines.append("    Capture-related callers:")
                for c in capture_callers[:10]:
                    lines.append("      <- {} @ {}".format(
                        c.getName()[:80], c.getEntryPoint()))
            elif callers:
                lines.append("    All callers ({}):" .format(len(callers)))
                for c, addr in callers[:10]:
                    lines.append("      <- {} @ {}".format(
                        c.getName()[:80], c.getEntryPoint()))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 7: Capture constraints (FpgaConstants, MsoConstants)
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 7: CAPTURE CONFIGURATION CONSTRAINTS")
    lines.append("=" * 78)
    lines.append("")

    constraint_patterns = [
        "FpgaConstants::",
        "MsoConstants::",
        "mSamplesPerChunk",
        "mPreTriggerLength",
        "mPostTriggerLength",
        "sample_length",
        "mTriggerAuto_",
        "mTriggerNormal_",
        "mTriggerInitial",
        "mTriggerThreshold",
        "mTriggerHysteresis",
        "channel_count ==",
        "sample_count %",
    ]
    constraint_strings = find_strings_matching(program, constraint_patterns)
    for addr, val, pattern in sorted(constraint_strings, key=lambda x: x[1]):
        lines.append("  \"{}\"".format(val[:120]))
        lines.append("    @ {}".format(addr))
        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("    Referenced by: {} @ {}".format(
                    func.getName()[:80], func.getEntryPoint()))
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
    println("Capture sequence functions found: {}/{}".format(
        len(found_functions), len(START_SEQUENCE_FUNCTIONS)))
    println("Capture strings found: {}".format(len(capture_strings)))
    println("Constraint strings found: {}".format(len(constraint_strings)))
    println("Report: {}".format(report_path))


# Entry point
if __name__ == "__main__":
    main()
else:
    main()
