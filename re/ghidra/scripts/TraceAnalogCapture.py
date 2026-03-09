# TraceAnalogCapture.py -- Ghidra script to trace the analog capture data
# pipeline in libgraph_server_shared.so.
#
# This script maps the full data path from USB bulk transfer reception through
# disaggregation, analog deserialization, calibration filtering, and into
# the AnalogStore. It extracts ADC configuration, AFE settings, decimation
# constants, and the ADC-to-voltage transform chain.
#
# Target functions and classes:
#   - DisaggregatorNode -- splits interleaved USB data into per-channel streams
#   - AnalogCreatorNode -- creates AnalogData from VectorData<int16_t>
#   - AnalogCalibrationFilter -- applies calibration (FIR filters)
#   - AnalogDownsamplerNode -- decimation/downsampling pipeline
#   - AnalogStore -- final analog data storage
#   - AnalogTriggerNode -- analog trigger detection
#   - AnalogMeasurerV1/V2 -- analog measurement implementations
#   - AnalogDeserializer -- low-level ADC data deserialization
#   - WaveformDeserializer -- waveform (MSO) data deserialization
#   - Logic2FpgaDevice::InitHmcad1100 -- HMCAD1100 ADC init
#   - Logic2FpgaDevice::SetAd9637Enabled -- AD9637 ADC control
#   - Logic2FpgaDevice::WriteAd9637Register -- AD9637 SPI register writes
#   - MsoDeviceInterface::ConfigureAfe -- AFE configuration
#   - DeviceSettings::GetAnalogBasicPerformanceOptions -- sample rate config
#   - DeviceSettings::GetAnalogAdvancedPerformanceOptions -- multi-rate config
#   - SetAdcFullScaleGain -- full-scale ADC gain control
#   - SetAds528xEnabled/SetAds528xDrive -- ADS528x ADC control (Lattice ECP5)
#
# Output: re/ghidra/reports/analog-capture-trace.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceAnalogCapture.py
#
# @author  saleae_reverse project
# @category AnalogCapture
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

# Key functions in the analog capture data pipeline, ordered from hardware
# configuration through USB reception, disaggregation, calibration, to storage.
PIPELINE_FUNCTIONS = [
    # ADC initialization and configuration
    ("InitHmcad1100", "HMCAD1100 ADC initialization (Logic 4/8, older revs)"),
    ("SetTestPattenHmcad1100", "HMCAD1100 test pattern mode"),
    ("SetAd9637Enabled", "AD9637 ADC enable/disable (Logic Pro, older revs)"),
    ("WriteAd9637Register", "AD9637 SPI register write (U16 addr, U16 data)"),
    ("SetAds528xEnabled", "ADS528x ADC enable (Lattice ECP5 devices)"),
    ("SetAds528xDrive", "ADS528x drive strength configuration"),
    ("GetAdcRegisterConfiguration",
     "ADC register config based on MsoDeviceConstants"),
    ("SetAdcFullScaleGain", "Full-scale ADC gain adjustment"),
    ("HandleSetAdcFullScaleGain", "Handler for ADC gain requests"),

    # AFE (analog front-end) configuration
    ("ConfigureAfe", "Configure analog front-end (4 channels)"),
    ("AfeConfig_RawHardware", "Raw AFE hardware config struct"),
    ("AfeConfig_VoltageScale", "AFE voltage scale config"),
    ("ReadAfeMetadata", "Read AFE metadata from device"),
    ("WriteAfeMetadata", "Write AFE metadata to device"),
    ("BuildAfeEeprom", "Build AFE EEPROM accessor"),
    ("EncodeVgaConfig", "Encode VGA configuration register"),
    ("BuildVga", "Build VGA (variable gain amplifier) component"),
    ("ClampAndQuantizeDacVoltage", "DAC voltage clamping and quantization"),

    # Capture configuration (analog-specific)
    ("SetCaptureParameters",
     "Writes capture config to FPGA (analog + digital)"),
    ("StartSampling", "Starts sampling on device"),
    ("SetBankPower", "Enables IO bank power before capture"),
    ("GetAnalogBasicPerformanceOptions",
     "Basic analog+digital sample rate options"),
    ("GetAnalogAdvancedPerformanceOptions",
     "Advanced multi-rate analog options"),
    ("SetAnalogBasicPerformanceOption", "Set basic analog sample rate"),
    ("SetAnalogAdvancedPerformanceOption", "Set advanced analog sample rate"),
    ("AnalogPhysicalSampleRateHz",
     "Get analog physical sample rate in Hz"),
    ("AnalogChannelConfigToHardwareConfig",
     "Translate user config to hardware register values"),

    # USB data reception and disaggregation
    ("OnDataBuffer", "Entry point for received USB data buffers"),
    ("DisaggregatorNode",
     "Splits interleaved USB data by channel type"),
    ("GetDisaggregatorSettings",
     "Get disaggregator settings for current config"),
    ("ConfigureGlitchFilters",
     "Configure digital glitch filters in disaggregator"),

    # Analog data pipeline nodes
    ("AnalogCreatorNode",
     "Creates AnalogData from VectorData<int16_t>"),
    ("AnalogCalibrationFilter",
     "Applies calibration filter to analog data"),
    ("AnalogDownsamplerNode",
     "Analog decimation/downsampling"),
    ("SetAnalogDownsampleRate", "Set analog downsample rate"),
    ("AnalogStore", "Final analog data storage"),
    ("AnalogTriggerNode", "Analog trigger detection"),
    ("AnalogMeasurer", "Analog measurement engine"),
    ("AnalogRenderer", "Analog data rendering"),

    # Deserializers
    ("AnalogDeserializer", "Analog ADC data deserializer"),
    ("WaveformDeserializer", "MSO waveform data deserializer"),

    # Data types and collections
    ("AnalogDataCollection", "Collection of AnalogData objects"),
    ("AdvanceToNextValueLessThan",
     "Search for ADC value less than threshold"),
    ("AdvanceToNextValueGreaterThan",
     "Search for ADC value greater than threshold"),

    # Full-scale and auto-scale
    ("FullScaleKernel", "ADC full-scale calibration kernel"),
    ("VerticalScaleKernel", "Vertical scale adjustment"),
    ("HandleWaveform", "Waveform data handler (auto-scale)"),

    # Simulation
    ("AnalogSimulationGenerator", "Analog test data generation"),
    ("ModifyFakeData", "Simulated data format (reveals packet structure)"),
    ("FakeReadThread", "Simulation read thread"),
]

# String patterns that reveal analog capture data format
FORMAT_STRING_PATTERNS = [
    # ADC configuration
    "ADC Read",
    "ADC Write",
    "ADC Locked",
    "ADC Vendor ID",
    "ADC failed",
    "Configuring ADC",
    "Waiting for ADC calibration",
    "invalid ADC data",
    "Invalid adc pattern mode",
    "Invalid ADC vendor id",
    "Force ADC 1V mode",
    "HMCAD",
    "AD9637",
    "Ads528x",
    "bitslip",

    # AFE
    "attn1=",
    "coupling=",
    "afe_config=",
    "Configuring AFE",
    "Updating AFE",
    "AFE Settling Time",
    "Updating VGA",
    "ConfigureAfe",
    "vga_gain=",

    # Analog pipeline
    "Analog Fifo overflow",
    "analog fsm error",
    "AnalogDeserializer",
    "AnalogMeasurerV1",
    "AnalogMeasurerV2",
    "AnalogData reqires",
    "SamplingConfigurationData is required prior to VectorData<AnalogSample>",
    "Received analog data without a downsampler",
    "Received sampling config with a downsampler already initialized",
    "analog_decimation_upper",
    "DisableAnalogChannels",
    "DisableAnalogTrigger",

    # Sample rate and decimation
    "analog sample rate is not valid",
    "analogSampleRate",
    "analogDownsample",
    "analogLimit_MaxDownsampleRatio",
    "downsampleMode",
    "downsampleRatio",
    "DownsampleMode::MinMax",
    "Setting sampling config",

    # ADC data format
    "minAdcCode",
    "maxAdcCode",
    "adcToVoltageTransform",
    "adcToDacVoltageScale",
    "MissingVoltageTransform",
    "VoltageTransform",
    "voltageScale",
    "fullScaleGain",
    "fullScaleVoltageRanges",
    "hittiteAdcGainRegisterValue",

    # Packet format
    "maxAnalogSamplesPerPacket",
    "minAnalogSamplesPerPacket",
    "analogChannelCount",
    "analogChannels",
    "afeChannels",
    "analogStats",

    # JESD (Lattice ECP5)
    "JESD lock",
    "JESD reset",
    "Locked JESD",
    "Failed to reset JESD",
    "SERDES reset",

    # Test pattern
    "SALEAE_ANALOG_TEST_PATTERN",
    "Setting test data configuration",
]

REPORT_FILENAME = "analog-capture-trace.txt"


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


def try_extract_immediate_constants(program, func, max_instructions=200):
    """Extract immediate constants from a function that might be
    register addresses, ADC values, or buffer sizes."""
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
           mnemonic.startswith("SH"):
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
    lines.append("ANALOG CAPTURE DATA PIPELINE TRACE")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: Locate pipeline functions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: ANALOG CAPTURE PIPELINE FUNCTIONS")
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

                lines.append("")
        else:
            lines.append("  [-] {} -- NOT FOUND".format(search_name))
            lines.append("      Expected: {}".format(description))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 2: ADC and AFE configuration strings
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 2: ADC AND AFE CONFIGURATION STRINGS")
    lines.append("=" * 78)
    lines.append("")

    format_strings = find_strings_matching(program, FORMAT_STRING_PATTERNS)
    for addr, val, pattern in sorted(format_strings, key=lambda x: x[2]):
        lines.append("  [{:40s}] \"{}\"".format(pattern, val[:100]))
        lines.append("  {:42s} @ {}".format("", addr))

        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("  {:42s} Referenced by: {} @ {}".format(
                    "", func.getName(), func.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 3: ADC initialization constants
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 3: ADC INITIALIZATION CONSTANTS")
    lines.append("(Register addresses and values from ADC init functions)")
    lines.append("=" * 78)
    lines.append("")

    adc_funcs = [
        "InitHmcad1100", "WriteAd9637Register", "SetAd9637Enabled",
        "SetAds528xEnabled", "SetAds528xDrive", "GetAdcRegisterConfiguration",
        "SetAdcFullScaleGain", "SetTestPattenHmcad1100",
    ]
    for sf_name in adc_funcs:
        matches = find_functions_containing(program, sf_name)
        for m in matches:
            constants = try_extract_immediate_constants(program, m, 300)
            if constants:
                lines.append("  {} @ {}:".format(m.getName(),
                                                  m.getEntryPoint()))
                for val, addr, mnemonic, op in constants[:40]:
                    lines.append("    {} {} = {} (0x{:x}) @ {}".format(
                        mnemonic, op, val, val, addr))
                lines.append("")

    # ------------------------------------------------------------------
    # Part 4: AFE configuration constants
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 4: AFE CONFIGURATION CONSTANTS")
    lines.append("(VGA gain, attenuator, DAC values)")
    lines.append("=" * 78)
    lines.append("")

    afe_funcs = [
        "ConfigureAfe", "EncodeVgaConfig", "BuildVga",
        "ClampAndQuantizeDacVoltage", "AfeConfig_RawHardware",
        "ForConfigNumber",
    ]
    for sf_name in afe_funcs:
        matches = find_functions_containing(program, sf_name)
        for m in matches:
            constants = try_extract_immediate_constants(program, m, 300)
            if constants:
                lines.append("  {} @ {}:".format(m.getName(),
                                                  m.getEntryPoint()))
                for val, addr, mnemonic, op in constants[:30]:
                    lines.append("    {} {} = {} (0x{:x}) @ {}".format(
                        mnemonic, op, val, val, addr))
                lines.append("")

    # ------------------------------------------------------------------
    # Part 5: Analog data graph pipeline
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 5: ANALOG DATA GRAPH PIPELINE")
    lines.append("(Data flow through the processing graph)")
    lines.append("=" * 78)
    lines.append("")

    # Trace pipe handler registrations for analog data types
    pipe_handlers = find_functions_containing(program,
                                              "RegisterPipeHandler")

    # VectorData<int16_t> pipes (disaggregated analog ADC data)
    analog_vector_pipes = [f for f in pipe_handlers
                           if "VectorDataIsE" in f.getName()]
    lines.append("  VectorData<int16_t> pipe handlers (analog ADC samples):")
    for f in analog_vector_pipes:
        lines.append("    {} @ {}".format(f.getName()[:120],
                                           f.getEntryPoint()))
    lines.append("")

    # AnalogData pipes (processed analog data)
    analog_data_pipes = [f for f in pipe_handlers
                         if "AnalogData" in f.getName()]
    lines.append("  AnalogData pipe handlers (processed analog data):")
    for f in analog_data_pipes:
        lines.append("    {} @ {}".format(f.getName()[:120],
                                           f.getEntryPoint()))
    lines.append("")

    # SamplingConfigurationData pipes for analog nodes
    sampling_pipes = [f for f in pipe_handlers
                      if "SamplingConfiguration" in f.getName()
                      and ("Analog" in f.getName()
                           or "Calibration" in f.getName())]
    lines.append("  SamplingConfigurationData -> Analog node handlers:")
    for f in sampling_pipes:
        lines.append("    {} @ {}".format(f.getName()[:120],
                                           f.getEntryPoint()))
    lines.append("")

    # AnalogTriggerSettingsData pipes
    trigger_pipes = [f for f in pipe_handlers
                     if "AnalogTrigger" in f.getName()]
    lines.append("  AnalogTriggerSettingsData pipe handlers:")
    for f in trigger_pipes:
        lines.append("    {} @ {}".format(f.getName()[:120],
                                           f.getEntryPoint()))
    lines.append("")

    # Waveform (MSO) data pipes
    waveform_pipes = [f for f in pipe_handlers
                      if "Waveform" in f.getName()]
    lines.append("  Waveform (MSO) data pipe handlers:")
    for f in waveform_pipes:
        lines.append("    {} @ {}".format(f.getName()[:120],
                                           f.getEntryPoint()))
    lines.append("")

    # ------------------------------------------------------------------
    # Part 6: Analog calibration filter analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 6: ANALOG CALIBRATION FILTER")
    lines.append("=" * 78)
    lines.append("")

    calib_funcs = find_functions_containing(program,
                                            "AnalogCalibrationFilter")
    for f in calib_funcs:
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
            for c, addr in callees[:10]:
                lines.append("      -> {} @ {}".format(
                    c.getName()[:80], c.getEntryPoint()))

        str_refs = get_string_refs_in_function(program, f)
        if str_refs:
            lines.append("    String references:")
            for s in str_refs[:15]:
                lines.append("      \"{}\"".format(s[:80]))

        lines.append("")

    # SetCalibration action handlers
    set_cal = find_functions_containing(program, "SetCalibration")
    for f in set_cal:
        if "Analog" in f.getName():
            lines.append("  {} @ {}".format(f.getName()[:120],
                                             f.getEntryPoint()))
            constants = try_extract_immediate_constants(program, f, 200)
            if constants:
                lines.append("    Constants:")
                for val, addr, mnemonic, op in constants[:20]:
                    lines.append("      {} {} = {} (0x{:x}) @ {}".format(
                        mnemonic, op, val, val, addr))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 7: Downsampling / decimation
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 7: ANALOG DOWNSAMPLING / DECIMATION")
    lines.append("=" * 78)
    lines.append("")

    ds_funcs = find_functions_containing(program, "AnalogDownsampler")
    for f in ds_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        lines.append("    Size: {} bytes".format(
            f.getBody().getNumAddresses()))

        constants = try_extract_immediate_constants(program, f, 200)
        if constants:
            lines.append("    Constants (potential downsample ratios/limits):")
            for val, addr, mnemonic, op in constants[:20]:
                lines.append("      {} {} = {} (0x{:x}) @ {}".format(
                    mnemonic, op, val, val, addr))

        str_refs = get_string_refs_in_function(program, f)
        if str_refs:
            lines.append("    String references:")
            for s in str_refs[:10]:
                lines.append("      \"{}\"".format(s[:80]))
        lines.append("")

    # DownsampleMode enum values
    dm_funcs = find_functions_containing(program, "DownsampleMode")
    for f in dm_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        lines.append("")

    # DownsampleRatioFloor
    drf_funcs = find_functions_containing(program, "DownsampleRatioFloor")
    for f in drf_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        constants = try_extract_immediate_constants(program, f, 100)
        if constants:
            for val, addr, mnemonic, op in constants[:10]:
                lines.append("    {} {} = {} (0x{:x}) @ {}".format(
                    mnemonic, op, val, val, addr))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 8: ADC-to-voltage transform chain
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 8: ADC-TO-VOLTAGE TRANSFORM CHAIN")
    lines.append("=" * 78)
    lines.append("")

    transform_patterns = [
        "adcToVoltageTransform",
        "adcToDacVoltageScale",
        "VoltageTransform",
        "voltageScale",
        "mScale",
        "mOffset",
    ]
    transform_strings = find_strings_matching(program, transform_patterns)
    for addr, val, pattern in sorted(transform_strings, key=lambda x: x[2]):
        lines.append("  \"{}\" @ {}".format(val[:100], addr))
        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("    Referenced by: {} @ {}".format(
                    func.getName()[:80], func.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 9: JESD204B interface (Lattice ECP5 devices)
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 9: JESD204B INTERFACE (LATTICE ECP5)")
    lines.append("=" * 78)
    lines.append("")

    jesd_patterns = [
        "JESD lock",
        "JESD reset",
        "Locked JESD",
        "Failed to reset JESD",
        "SERDES reset",
        "bitslip",
        "Lattice PLL",
    ]
    jesd_strings = find_strings_matching(program, jesd_patterns)
    for addr, val, pattern in sorted(jesd_strings, key=lambda x: x[2]):
        lines.append("  \"{}\" @ {}".format(val[:120], addr))
        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("    Referenced by: {} @ {}".format(
                    func.getName()[:80], func.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 10: Analog sample rate and channel constraints
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 10: ANALOG SAMPLE RATE AND CHANNEL CONSTRAINTS")
    lines.append("=" * 78)
    lines.append("")

    rate_funcs = [
        "GetAnalogBasicPerformanceOptions",
        "GetAnalogAdvancedPerformanceOptions",
        "AnalogPhysicalSampleRateHz",
        "SetAnalogBasicPerformanceOption",
        "SetAnalogAdvancedPerformanceOption",
    ]
    for sf_name in rate_funcs:
        matches = find_functions_containing(program, sf_name)
        for m in matches:
            lines.append("  {} @ {}".format(m.getName(), m.getEntryPoint()))
            lines.append("    Size: {} bytes".format(
                m.getBody().getNumAddresses()))

            constants = try_extract_immediate_constants(program, m, 300)
            interesting = [(v, a, mn, o) for v, a, mn, o in constants
                           if v > 100]
            if interesting:
                lines.append("    Constants (potential sample rates/limits):")
                for val, addr, mnemonic, op in interesting[:20]:
                    lines.append("      {} {} = {} (0x{:x}) @ {}".format(
                        mnemonic, op, val, val, addr))
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
    println("Format strings found: {}".format(len(format_strings)))
    println("Calibration filter functions: {}".format(len(calib_funcs)))
    println("Downsampler functions: {}".format(len(ds_funcs)))
    println("JESD interface strings: {}".format(len(jesd_strings)))
    println("Report: {}".format(report_path))


# Entry point
if __name__ == "__main__":
    main()
else:
    main()
