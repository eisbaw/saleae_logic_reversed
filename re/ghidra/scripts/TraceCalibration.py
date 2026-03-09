# TraceCalibration.py -- Ghidra script to trace the calibration data flow
# in libgraph_server_shared.so.
#
# This script maps:
#   - CalibrationData parsing and storage
#   - AnalogCalibrationFilter internals (DF1 SOS / IIR filter)
#   - ADC-to-voltage transform chain
#   - EEPROM calibration data path (AT24C64D)
#   - CalibrationToMetadataConstants / MetadataConstantsToCalibration
#   - AfeConfig_RawHardware config numbering
#   - Full-scale voltage range calibration
#   - SetAdcFullScaleGain (Hittite HMCAD ADC gain)
#
# Key insight from asar analysis:
#   Calibration data is a Boost-serialized XML file (DeviceCalibrationSet)
#   downloaded from https://downloads.saleae.com/calibration/{deviceId}.cal
#   or from Saleae's GraphQL API at https://api.saleae.com/graphql.
#
#   The file contains per-channel, per-sample-rate IIR filter coefficients
#   in Direct Form I Second-Order Sections (DF1 SOS) format:
#     - 5 biquad sections per channel per sample rate
#     - Each section: {b0, b1, b2, a1, a2} (a0 normalized to 1.0)
#     - 6 values per section in the XML (b0,b1,b2,a0=1.0,a1,a2)
#     - Also: groupDelay and impulseLength per channel per sample rate
#
# Output: re/ghidra/reports/calibration-trace.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceCalibration.py
#
# @author  saleae_reverse project
# @category Calibration
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

# Functions involved in calibration data flow
CALIBRATION_FUNCTIONS = [
    # Calibration data types and conversion
    ("CalibrationData", "Graph calibration data container"),
    ("CalibrationToMetadataConstants",
     "Convert MsoCalibrationProfile -> MsoAfeMetadata::CalibrationConstants"),
    ("MetadataConstantsToCalibration",
     "Convert MsoAfeMetadata::CalibrationConstants -> MsoCalibrationProfile"),
    ("MsoCalibrationProfile",
     "MSO calibration profile (per-device calibration)"),
    ("MsoChannelCalibration",
     "Per-channel calibration data for MSO devices"),

    # AnalogCalibrationFilter -- the IIR/SOS filter node
    ("AnalogCalibrationFilter",
     "Graph node that applies IIR (DF1 SOS) filter to analog data"),
    ("SetCalibration",
     "Action to load calibration coefficients into filter node"),
    ("SetChannel",
     "Action to set which channel the calibration filter processes"),

    # IIR filter implementation
    ("SimdIirFilterPrototype",
     "SIMD-optimized IIR filter prototype"),
    ("SimdPrototype_Static2By3IirFilter",
     "SIMD 2x3 IIR filter (2 parallel, 3 coefficients per section?)"),

    # AFE configuration and calibration
    ("AfeConfig_RawHardware", "Raw AFE hardware config struct"),
    ("AfeConfig_VoltageScale", "AFE voltage scale config"),
    ("ForConfigNumber", "Map config number to AFE hardware settings"),
    ("GetConfigNumber", "Map AFE hardware settings to config number"),
    ("ReadAfeMetadata", "Read AFE metadata from EEPROM"),
    ("WriteAfeMetadata", "Write AFE metadata to EEPROM"),

    # EEPROM operations
    ("BuildAfeEeprom", "Build AFE EEPROM accessor (AT24C64D)"),
    ("Eeprom_AT24C64D", "AT24C64D EEPROM driver (64Kbit, I2C)"),
    ("ReadEeprom", "Read from EEPROM via USB I2C tunnel"),
    ("WriteEeprom", "Write to EEPROM via USB I2C tunnel"),
    ("TestEeprom", "EEPROM test (LogicProfessionalDevice)"),
    ("WipeEeprom", "Erase EEPROM contents"),

    # Voltage transform
    ("VoltageTransform", "ADC-to-voltage linear transform struct"),
    ("AnalogChannelConfigToHardwareConfig",
     "Translate user config to hardware config (uses calibration)"),

    # ADC gain calibration
    ("SetAdcFullScaleGain",
     "Set HMCAD1100 full-scale ADC gain (from hittiteAdcGainRegisterValue)"),
    ("HandleSetAdcFullScaleGain", "Handler for ADC gain requests"),
    ("FullScaleKernel", "Full-scale auto-calibration kernel"),
    ("VerticalScaleKernel", "Vertical scale adjustment kernel"),

    # DAC (offset) calibration
    ("ClampAndQuantizeDacVoltage", "DAC voltage clamping"),
    ("SetDacVoltage", "Set DAC offset voltage"),

    # Calibration data loading
    ("reading calibration", "Calibration file loading log message"),
]

# String patterns related to calibration
CALIBRATION_STRING_PATTERNS = [
    "calibrat",
    "CalibrationData",
    "CalibrationProfile",
    "CalibrationConstants",
    "CalibrationToMetadata",
    "MetadataConstantsToCalibration",
    "afeConfigCalibrations",
    "calibrationSets",
    "adcToVoltageTransform",
    "adcToDacVoltageScale",
    "VoltageTransform",
    "MissingVoltageTransform",
    "voltageTransform",
    "adcTransform",
    "fullScaleGain",
    "hittiteAdcGainRegisterValue",
    "fullScaleVoltageRanges",
    "reading calibration",
    "df1SosCoefficients",
    "SimdIirFilter",
    "SimdPrototype",
    "AFE_CONFIG_COUNT",
    "MAX_AFE_CHANNEL_COUNT",
    "TotalAfeConfigurations",
    "Scope calibration is missing",
    "sample rate does not match",
    "SALEAE_MSO_CALIBRATION_PATH",
    "ReadAfeMetadata",
    "WriteAfeMetadata",
    "EEPROM",
    "eeprom",
    "EEprom error",
    "AT24C64D",
    "dacVoltage",
    "dacScale",
    "maxDacVoltage",
    "minDacVoltage",
    "attn100Transform",
    "attn10Transform",
    "ADC failed to finish foreground calibration",
    "Waiting for ADC calibration",
    "Locked and calibrated DDR",
    "Ch {} transform",
]

REPORT_FILENAME = "calibration-trace.txt"


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


def try_extract_immediate_constants(program, func, max_instructions=200):
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
           mnemonic.startswith("AND") or mnemonic.startswith("OR") or \
           mnemonic.startswith("SH") or mnemonic.startswith("LEA"):
            for i in range(inst.getNumOperands()):
                op = inst.getDefaultOperandRepresentation(i)
                try:
                    if op.startswith("0x"):
                        val = int(op, 16)
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
    lines.append("CALIBRATION DATA FLOW TRACE")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: Calibration pipeline functions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: CALIBRATION PIPELINE FUNCTIONS")
    lines.append("=" * 78)
    lines.append("")

    found_functions = {}

    for search_name, description in CALIBRATION_FUNCTIONS:
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
                    for c, addr in callers[:10]:
                        lines.append("        <- {} @ {}".format(
                            c.getName()[:80], c.getEntryPoint()))

                callees = get_callees(program, m)
                if callees:
                    lines.append("      Callees ({}):" .format(len(callees)))
                    for c, addr in callees[:10]:
                        lines.append("        -> {} @ {}".format(
                            c.getName()[:80], c.getEntryPoint()))

                lines.append("")
        else:
            lines.append("  [-] {} -- NOT FOUND".format(search_name))
            lines.append("      Expected: {}".format(description))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 2: Calibration-related strings
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 2: CALIBRATION-RELATED STRINGS")
    lines.append("=" * 78)
    lines.append("")

    cal_strings = find_strings_matching(program,
                                        CALIBRATION_STRING_PATTERNS)
    for addr, val, pattern in sorted(cal_strings, key=lambda x: x[2]):
        lines.append("  [{:40s}] \"{}\"".format(pattern, val[:120]))
        lines.append("  {:42s} @ {}".format("", addr))

        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("  {:42s} Referenced by: {} @ {}".format(
                    "", func.getName()[:80], func.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 3: AnalogCalibrationFilter deep analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 3: ANALOG CALIBRATION FILTER DEEP ANALYSIS")
    lines.append("(IIR filter implementation: DF1 SOS biquad cascade)")
    lines.append("=" * 78)
    lines.append("")

    # Find the filter kernel implementation functions
    filter_funcs = find_functions_containing(program,
                                             "AnalogCalibrationFilter")
    for f in filter_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        lines.append("    Size: {} bytes".format(
            f.getBody().getNumAddresses()))

        callers = get_callers(program, f)
        if callers:
            lines.append("    Callers ({}):" .format(len(callers)))
            for c, addr in callers[:10]:
                lines.append("      <- {} @ {}".format(
                    c.getName()[:80], c.getEntryPoint()))

        callees = get_callees(program, f)
        if callees:
            lines.append("    Callees ({}):" .format(len(callees)))
            for c, addr in callees[:15]:
                lines.append("      -> {} @ {}".format(
                    c.getName()[:80], c.getEntryPoint()))

        str_refs = get_string_refs_in_function(program, f)
        if str_refs:
            lines.append("    String references:")
            for s in str_refs[:20]:
                lines.append("      \"{}\"".format(s[:100]))

        constants = try_extract_immediate_constants(program, f, 500)
        if constants:
            lines.append("    Constants (filter taps, buffer sizes, etc.):")
            for val, addr, mnemonic, op in constants[:30]:
                lines.append("      {} {} = {} (0x{:x}) @ {}".format(
                    mnemonic, op, val, val, addr))

        lines.append("")

    # IIR filter implementation
    iir_funcs = find_functions_containing(program, "SimdIirFilter")
    iir_funcs += find_functions_containing(program, "Static2By3IirFilter")
    for f in iir_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        lines.append("    Size: {} bytes".format(
            f.getBody().getNumAddresses()))
        constants = try_extract_immediate_constants(program, f, 300)
        if constants:
            lines.append("    Constants:")
            for val, addr, mnemonic, op in constants[:20]:
                lines.append("      {} {} = {} (0x{:x}) @ {}".format(
                    mnemonic, op, val, val, addr))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 4: EEPROM calibration data path
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 4: EEPROM CALIBRATION DATA PATH")
    lines.append("(AT24C64D: 64Kbit I2C EEPROM for MSO AFE metadata)")
    lines.append("=" * 78)
    lines.append("")

    eeprom_funcs = find_functions_containing(program, "Eeprom")
    eeprom_funcs += find_functions_containing(program, "EEPROM")
    for f in eeprom_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        lines.append("    Size: {} bytes".format(
            f.getBody().getNumAddresses()))

        str_refs = get_string_refs_in_function(program, f)
        if str_refs:
            lines.append("    String references:")
            for s in str_refs[:10]:
                lines.append("      \"{}\"".format(s[:100]))

        constants = try_extract_immediate_constants(program, f, 300)
        if constants:
            lines.append("    Constants (EEPROM addresses, page sizes):")
            for val, addr, mnemonic, op in constants[:20]:
                lines.append("      {} {} = {} (0x{:x}) @ {}".format(
                    mnemonic, op, val, val, addr))
        lines.append("")

    # AFE metadata functions
    afe_meta_funcs = find_functions_containing(program, "AfeMetadata")
    for f in afe_meta_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        callers = get_callers(program, f)
        if callers:
            for c, addr in callers[:5]:
                lines.append("    <- {} @ {}".format(
                    c.getName()[:80], c.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 5: AFE config numbering and voltage ranges
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 5: AFE CONFIG NUMBERING AND VOLTAGE RANGES")
    lines.append("=" * 78)
    lines.append("")

    config_funcs = find_functions_containing(program, "ForConfigNumber")
    config_funcs += find_functions_containing(program, "GetConfigNumber")
    for f in config_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        lines.append("    Size: {} bytes".format(
            f.getBody().getNumAddresses()))

        constants = try_extract_immediate_constants(program, f, 500)
        if constants:
            lines.append("    Constants (config numbers, voltage values):")
            for val, addr, mnemonic, op in constants[:30]:
                lines.append("      {} {} = {} (0x{:x}) @ {}".format(
                    mnemonic, op, val, val, addr))

        str_refs = get_string_refs_in_function(program, f)
        if str_refs:
            lines.append("    String references:")
            for s in str_refs[:10]:
                lines.append("      \"{}\"".format(s[:80]))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 6: Voltage transform chain
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 6: VOLTAGE TRANSFORM CHAIN")
    lines.append("(ADC code -> voltage conversion)")
    lines.append("=" * 78)
    lines.append("")

    transform_funcs = find_functions_containing(program, "VoltageTransform")
    transform_funcs += find_functions_containing(program,
                                                  "adcToVoltageTransform")
    for f in transform_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        callers = get_callers(program, f)
        if callers:
            for c, addr in callers[:5]:
                lines.append("    <- {} @ {}".format(
                    c.getName()[:80], c.getEntryPoint()))
        callees = get_callees(program, f)
        if callees:
            for c, addr in callees[:5]:
                lines.append("    -> {} @ {}".format(
                    c.getName()[:80], c.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 7: CalibrationData JSON serialization fields
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 7: CALIBRATION JSON SERIALIZATION FIELDS")
    lines.append("=" * 78)
    lines.append("")

    json_patterns = [
        "/calibration",
        "/calibrationSets",
        "/afeConfigCalibrations",
        "/afeConfig",
        "/afeMetadata",
        "/adcToVoltageTransform",
        "/adcToDacVoltageScale",
        "/adcTransform",
        "/voltageTransform",
        "/dacVoltage",
        "/dacScale",
        "/maxDacVoltage",
        "/minDacVoltage",
        "/attn100Transform",
        "/attn10Transform",
        "/gain",
        "/offset",
        "/scale",
        "/sampleRate",
        "/channels",
        "/df1SosCoefficients",
        "/hittiteAdcGainRegisterValue",
    ]
    json_strings = find_strings_matching(program, json_patterns)
    for addr, val, pattern in sorted(json_strings, key=lambda x: x[0]):
        lines.append("  \"{}\" @ {}".format(val[:100], addr))
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
    println("=== CALIBRATION TRACE SUMMARY ===")
    println("Pipeline functions found: {}/{}".format(
        len(found_functions), len(CALIBRATION_FUNCTIONS)))
    println("Calibration strings found: {}".format(len(cal_strings)))
    println("Filter functions (AnalogCalibrationFilter): {}".format(
        len(filter_funcs)))
    println("IIR filter implementations: {}".format(len(iir_funcs)))
    println("EEPROM functions: {}".format(len(eeprom_funcs)))
    println("JSON field strings: {}".format(len(json_strings)))
    println("Report: {}".format(report_path))


# Entry point
if __name__ == "__main__":
    main()
else:
    main()
