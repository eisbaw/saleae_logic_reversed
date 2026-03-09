# TraceAdcInit.py -- Ghidra script to trace ADC initialization sequences
# in libgraph_server_shared.so.
#
# This script focuses on three ADC families used by Saleae Logic devices:
#   1. HMCAD1100 (Logic 4/8, older Xilinx Spartan-6 hardware revisions)
#   2. AD9637 (Logic Pro 8/16, older Xilinx hardware revisions)
#   3. ADS5282/ADS5287 (all devices, newer Lattice ECP5 hardware revisions)
#
# It traces the ADC register access paths, extracts SPI register addresses
# and values written during initialization, maps the FPGA SPI bridge
# (register 0x0004) usage, and identifies the ADC selection logic based
# on board revision.
#
# Key targets:
#   - Logic2FpgaDevice::InitHmcad1100(Hmcad1100Settings)
#   - Logic2FpgaDevice::SetTestPattenHmcad1100(AdcTestPattern)
#   - Logic2FpgaDevice::SetAd9637Enabled(bool)
#   - Logic2FpgaDevice::WriteAd9637Register(U16, U16)
#   - Hmcad1100Settings::SetDefault(DeviceType)
#   - SetAds528xEnabled / SetAds528xDrive
#   - GetAdcRegisterConfiguration(MsoDeviceConstants)
#   - MsoFpga::WriteSpi / ReadSpi_7BitAddress / ReadSpi_15BitAddress
#   - HandleSetAdcFullScaleGain
#   - CommonDeviceInit (Logic2FpgaDevice)
#   - ParseBoardRevision / RevisionDetail
#
# Output: re/ghidra/reports/adc-init-trace.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceAdcInit.py
#
# @author  saleae_reverse project
# @category AdcInit
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

# ADC initialization functions to trace, organized by subsystem
ADC_INIT_FUNCTIONS = [
    # HMCAD1100 (Logic 4/8, Xilinx revisions)
    ("InitHmcad1100",
     "Full HMCAD1100 initialization sequence via FPGA SPI bridge"),
    ("SetTestPattenHmcad1100",
     "HMCAD1100 test pattern mode (AdcTestPattern enum)"),
    ("Hmcad1100Settings",
     "HMCAD1100 settings struct and SetDefault per DeviceType"),
    ("SetDefault",
     "SetDefault(DeviceType) -- per-device HMCAD1100 defaults"),

    # AD9637 (Logic Pro 8/16, Xilinx revisions)
    ("SetAd9637Enabled",
     "AD9637 enable/disable via FPGA SPI bridge"),
    ("WriteAd9637Register",
     "AD9637 SPI register write (U16 addr, U16 data)"),

    # ADS528x (all devices, Lattice ECP5 revisions)
    ("SetAds528xEnabled",
     "ADS528x enable via Lattice ECP5 FPGA"),
    ("SetAds528xDrive",
     "ADS528x drive strength configuration"),
    ("GetAdcRegisterConfiguration",
     "Static ADC register config table for MsoDeviceConstants"),

    # FPGA SPI bridge (register 0x0004)
    ("WriteSpi",
     "FPGA SPI master write to ADC (reg 0x0004)"),
    ("ReadSpi_7BitAddress",
     "FPGA SPI read with 7-bit address (HMCAD1100)"),
    ("ReadSpi_15BitAddress",
     "FPGA SPI read with 15-bit address (AD9637)"),

    # ADC gain and calibration
    ("SetAdcFullScaleGain",
     "ADC full-scale gain control"),
    ("HandleSetAdcFullScaleGain",
     "Handler for SetAdcFullScaleGain requests"),

    # Board revision and ADC type selection
    ("ParseBoardRevision",
     "Parse board revision to determine ADC type"),
    ("UsesNewCryptoInterface",
     "RevisionDetail method (indicates board revision struct)"),
    ("CommonDeviceInit",
     "Logic2FpgaDevice common init (calls ADC setup)"),

    # JESD204B (Lattice ECP5 ADC interface)
    ("ResetJesdPhyTransceivers",
     "JESD PHY transceiver reset for ADS528x link"),
    ("StartJesdBerTest",
     "JESD BER test start"),
    ("GetJesdBerTestResults",
     "JESD BER test results"),
    ("StopJesdBerTest",
     "JESD BER test stop"),

    # ADC temperature and monitoring (MSO path)
    ("BuildAdc",
     "MsoDeviceInterface ADC component builder"),
    ("BuildIoExpander_DcVgaAdcDac",
     "I2C IO expander for DC/VGA/ADC/DAC control"),

    # Analog front-end (tightly coupled to ADC init)
    ("ConfigureAfe",
     "Configure analog front-end channels"),
    ("SetBankPower",
     "Enable IO bank power (prerequisite for ADC)"),
]

# String patterns specific to ADC initialization
ADC_STRING_PATTERNS = [
    # ADC register access logging
    "ADC Read 0x",
    "ADC Write 0x",
    "ADC Locked",
    "ADC Vendor ID",
    "ADC failed",
    "Failed to lock ADC",
    "Configuring ADC",
    "set ADC range",
    "Force ADC 1V mode",

    # ADC calibration
    "Waiting for ADC calibration",
    "foreground calibration",
    "hittiteAdcGainRegisterValue",
    "fullScaleGain",

    # ADC data validation
    "invalid ADC data",
    "Invalid adc pattern mode",
    "Invalid ADC vendor id",
    "exceedes 8 bits",
    "ADC MISO",

    # Board revision and ADC type
    "ParseBoardRevision",
    "Unsupported board revision",
    "AdcType::AD9637",
    "Board Revision",

    # HMCAD1100 specific
    "InitHmcad1100",
    "Hmcad1100",
    "SetTestPatten",

    # AD9637 specific
    "SetAd9637",
    "WriteAd9637",
    "Ad9637",

    # ADS528x specific
    "Ads528x",
    "bitslip",
    "SERDES",

    # JESD204B
    "JESD lock",
    "JESD reset",
    "Locked JESD",
    "Failed to reset JESD",
    "JESD BER",

    # SPI bridge
    "WriteSpi",
    "ReadSpi",
    "Upper 8 bits",
    "SpiClock",
    "SpiWire",
    "SpiAddress",

    # Test pattern
    "SALEAE_ANALOG_TEST_PATTERN",
    "Setting test data",
    "testpattern=",

    # IO expander
    "Configuring IO Expander",
    "Dc/Vga/Adc/Dac",
]

# Known HMCAD1100 register addresses (from datasheet SBAS547C)
# These are the SPI register addresses for the HMCAD1100
HMCAD1100_KNOWN_REGISTERS = {
    0x00: "chip_id (read-only, should return 0x45 for HMCAD1100)",
    0x01: "reserved",
    0x03: "power_control (standby, power-down modes)",
    0x0F: "soft_reset (write 0x01 to reset)",
    0x11: "lvds_output_mode (output data format)",
    0x12: "lvds_output_phase",
    0x24: "input_config_1ch (1-channel mode input mux)",
    0x25: "input_config_2ch (2-channel mode input mux)",
    0x26: "input_config_4ch (4-channel mode input mux)",
    0x27: "channel_enable (per-channel enable bits)",
    0x2A: "coarse_gain_ch1",
    0x2B: "coarse_gain_ch2",
    0x2C: "coarse_gain_ch3",
    0x2D: "coarse_gain_ch4",
    0x30: "fine_gain_ch1",
    0x31: "fine_gain_ch2",
    0x32: "fine_gain_ch3",
    0x33: "fine_gain_ch4",
    0x34: "channel_mode (1ch/2ch/4ch selection)",
    0x3A: "test_pattern_mode",
    0x3B: "test_pattern_value_lsb",
    0x3C: "test_pattern_value_msb",
    0x42: "clk_divide (clock divider)",
    0x45: "full_scale_adj",
    0x50: "jitter_ctrl",
    0x51: "calibration_1",
    0x52: "calibration_2",
    0x53: "calibration_3",
    0x55: "resolution_select",
}

# Known AD9637 register addresses (from datasheet)
# AD9637 uses 13-bit SPI register addresses
AD9637_KNOWN_REGISTERS = {
    0x0000: "spi_port_config (MSB/LSB first, soft reset)",
    0x0001: "chip_id (read-only, 0x49 for AD9637)",
    0x0002: "chip_grade",
    0x0005: "device_index (channel A/B select for config)",
    0x0008: "power_modes (normal, standby, power-down, reset)",
    0x0009: "global_clock (duty cycle stabilizer, clock divide)",
    0x000B: "clock_divide_ratio",
    0x000D: "test_mode (output test patterns)",
    0x000E: "bist_enable",
    0x0010: "offset_adjust",
    0x0014: "output_mode (CMOS/LVDS, data format)",
    0x0015: "output_adjust (drive strength, output coding)",
    0x0016: "output_phase (clock-data phase)",
    0x0017: "output_delay",
    0x0018: "vref (internal voltage reference)",
    0x0019: "user_patt1_lsb",
    0x001A: "user_patt1_msb",
    0x001B: "user_patt2_lsb",
    0x001C: "user_patt2_msb",
    0x0021: "serial_control",
    0x0022: "serial_ch_stat",
    0x0100: "sample_rate",
    0x0101: "user_io_ctrl_1",
    0x0102: "user_io_ctrl_2",
    0x0103: "user_io_ctrl_3",
    0xFF01: "flex_channel_input",
    0xFF02: "flex_output_config",
}

REPORT_FILENAME = "adc-init-trace.txt"


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
                    strings_found.append(
                        (str(data.getValue()), inst.getAddress()))
    return strings_found


def extract_immediate_constants(program, func, max_instructions=500):
    """Extract immediate constants from a function that could be
    ADC register addresses, SPI data values, or configuration constants."""
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
        # Track MOV, CMP, AND, OR, SHL, SHR, TEST instructions
        if mnemonic.startswith("MOV") or mnemonic.startswith("CMP") or \
           mnemonic.startswith("AND") or mnemonic.startswith("OR") or \
           mnemonic.startswith("SH") or mnemonic.startswith("TEST") or \
           mnemonic.startswith("XOR"):
            for i in range(inst.getNumOperands()):
                op = inst.getDefaultOperandRepresentation(i)
                try:
                    if op.startswith("0x"):
                        val = int(op, 16)
                        constants.append(
                            (val, inst.getAddress(), mnemonic, op,
                             inst.toString()))
                    elif op.startswith("-0x"):
                        val = int(op, 16)
                        constants.append(
                            (val, inst.getAddress(), mnemonic, op,
                             inst.toString()))
                except (ValueError, AttributeError):
                    pass
    return constants


def classify_spi_constant(val):
    """Try to classify a constant as an HMCAD1100 or AD9637 register."""
    annotations = []
    # Check HMCAD1100 register addresses (7-bit, 0x00-0x55)
    if val in HMCAD1100_KNOWN_REGISTERS:
        annotations.append("HMCAD1100 reg 0x{:02x}: {}".format(
            val, HMCAD1100_KNOWN_REGISTERS[val]))
    # Check AD9637 register addresses (13-bit)
    if val in AD9637_KNOWN_REGISTERS:
        annotations.append("AD9637 reg 0x{:04x}: {}".format(
            val, AD9637_KNOWN_REGISTERS[val]))
    # Check for SPI data register address
    if val == 0x0004:
        annotations.append("FPGA SPI_DATA register")
    if val == 0x0008:
        annotations.append("FPGA STATUS_DDR_JESD register")
    if val == 0x0040:
        annotations.append("FPGA CAPTURE_CONTROL register")
    if val == 0x01C0:
        annotations.append("FPGA COMMIT_CONTROL register")
    if val == 0x00E0:
        annotations.append("FPGA INIT_CONFIG register")
    # Check for bit masks commonly used in SPI framing
    if val == 0x7FFFFF:
        annotations.append("SPI 23-bit data mask (bits [22:0])")
    if val == 0x800000:
        annotations.append("SPI R/W bit (bit 23)")
    if val == 0xFF000000:
        annotations.append("SPI upper 8 bits mask (must be zero)")
    if val == 0x7F:
        annotations.append("7-bit address mask")
    if val == 0x7FFF:
        annotations.append("15-bit address mask")
    return annotations


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


# ---------------------------------------------------------------------------
# Main script
# ---------------------------------------------------------------------------

def main():
    monitor = ConsoleTaskMonitor()
    program = currentProgram
    report_path = get_report_path()

    lines = []
    lines.append("=" * 78)
    lines.append("ADC INITIALIZATION TRACE")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")
    lines.append("Three ADC families:")
    lines.append("  1. HMCAD1100 -- Logic 4/8 (Xilinx Spartan-6 revisions)")
    lines.append("  2. AD9637   -- Logic Pro 8/16 (Xilinx revisions)")
    lines.append("  3. ADS528x  -- All devices (Lattice ECP5 revisions)")
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: Locate ADC init functions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: ADC INITIALIZATION FUNCTIONS")
    lines.append("=" * 78)
    lines.append("")

    found_functions = {}

    for search_name, description in ADC_INIT_FUNCTIONS:
        matches = find_functions_containing(program, search_name)
        if matches:
            found_functions[search_name] = []
            for m in matches:
                found_functions[search_name].append(m)
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
    # Part 2: ADC register access strings and their code locations
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 2: ADC REGISTER ACCESS STRINGS")
    lines.append("=" * 78)
    lines.append("")

    format_strings = find_strings_matching(program, ADC_STRING_PATTERNS)
    for addr, val, pattern in sorted(format_strings, key=lambda x: x[2]):
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
    # Part 3: HMCAD1100 initialization constants
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 3: HMCAD1100 INITIALIZATION CONSTANTS")
    lines.append("(Register addresses and values from InitHmcad1100)")
    lines.append("")
    lines.append("HMCAD1100 known register map (from datasheet SBAS547C):")
    for addr, name in sorted(HMCAD1100_KNOWN_REGISTERS.items()):
        lines.append("  0x{:02x} = {}".format(addr, name))
    lines.append("=" * 78)
    lines.append("")

    hmcad_funcs = ["InitHmcad1100", "SetTestPattenHmcad1100",
                   "SetDefault", "Hmcad1100Settings"]
    for sf_name in hmcad_funcs:
        matches = find_functions_containing(program, sf_name)
        for m in matches:
            constants = extract_immediate_constants(program, m, 500)
            if constants:
                lines.append("  {} @ {}:".format(
                    m.getName()[:100], m.getEntryPoint()))

                for val, addr, mnemonic, op, full_inst in constants:
                    annotations = classify_spi_constant(val)
                    annotation_str = ""
                    if annotations:
                        annotation_str = "  <-- " + "; ".join(annotations)
                    lines.append("    0x{:08x}: {} {}{}".format(
                        addr.getOffset(), full_inst[:60], " " * max(
                            0, 60 - len(full_inst[:60])),
                        annotation_str))

                lines.append("")

            # Also show string references for context
            str_refs = get_string_refs_in_function(program, m)
            if str_refs:
                lines.append("  String refs in {}:".format(
                    m.getName()[:60]))
                for s, saddr in str_refs[:20]:
                    lines.append("    @ {}: \"{}\"".format(
                        saddr, s[:100]))
                lines.append("")

    # ------------------------------------------------------------------
    # Part 4: AD9637 initialization constants
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 4: AD9637 INITIALIZATION CONSTANTS")
    lines.append("(Register addresses and values from WriteAd9637Register)")
    lines.append("")
    lines.append("AD9637 known register map (from datasheet):")
    for addr, name in sorted(AD9637_KNOWN_REGISTERS.items()):
        lines.append("  0x{:04x} = {}".format(addr, name))
    lines.append("=" * 78)
    lines.append("")

    ad9637_funcs = ["WriteAd9637Register", "SetAd9637Enabled"]
    for sf_name in ad9637_funcs:
        matches = find_functions_containing(program, sf_name)
        for m in matches:
            constants = extract_immediate_constants(program, m, 500)
            if constants:
                lines.append("  {} @ {}:".format(
                    m.getName()[:100], m.getEntryPoint()))

                for val, addr, mnemonic, op, full_inst in constants:
                    annotations = classify_spi_constant(val)
                    annotation_str = ""
                    if annotations:
                        annotation_str = "  <-- " + "; ".join(annotations)
                    lines.append("    0x{:08x}: {} {}{}".format(
                        addr.getOffset(), full_inst[:60], " " * max(
                            0, 60 - len(full_inst[:60])),
                        annotation_str))

                lines.append("")

            str_refs = get_string_refs_in_function(program, m)
            if str_refs:
                lines.append("  String refs in {}:".format(
                    m.getName()[:60]))
                for s, saddr in str_refs[:20]:
                    lines.append("    @ {}: \"{}\"".format(
                        saddr, s[:100]))
                lines.append("")

    # ------------------------------------------------------------------
    # Part 5: ADS528x initialization and JESD204B link
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 5: ADS528x INITIALIZATION AND JESD204B LINK")
    lines.append("=" * 78)
    lines.append("")

    ads528_funcs = ["SetAds528xEnabled", "SetAds528xDrive",
                    "GetAdcRegisterConfiguration",
                    "ResetJesdPhyTransceivers"]
    for sf_name in ads528_funcs:
        matches = find_functions_containing(program, sf_name)
        for m in matches:
            lines.append("  {} @ {}:".format(
                m.getName()[:100], m.getEntryPoint()))
            lines.append("    Size: {} bytes".format(
                m.getBody().getNumAddresses()))

            constants = extract_immediate_constants(program, m, 500)
            if constants:
                lines.append("    Constants:")
                for val, addr, mnemonic, op, full_inst in constants:
                    annotations = classify_spi_constant(val)
                    annotation_str = ""
                    if annotations:
                        annotation_str = "  <-- " + "; ".join(annotations)
                    lines.append("      0x{:08x}: {} {}{}".format(
                        addr.getOffset(), full_inst[:60], " " * max(
                            0, 60 - len(full_inst[:60])),
                        annotation_str))

            str_refs = get_string_refs_in_function(program, m)
            if str_refs:
                lines.append("    String refs:")
                for s, saddr in str_refs[:20]:
                    lines.append("      @ {}: \"{}\"".format(
                        saddr, s[:100]))

            callers = get_callers(program, m)
            if callers:
                lines.append("    Callers ({}):" .format(len(callers)))
                for c, caddr in callers[:10]:
                    lines.append("      <- {} @ {}".format(
                        c.getName()[:80], c.getEntryPoint()))

            lines.append("")

    # ------------------------------------------------------------------
    # Part 6: FPGA SPI bridge analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 6: FPGA SPI BRIDGE ANALYSIS")
    lines.append("(Register 0x0004 -- SPI master to ADC)")
    lines.append("")
    lines.append("SPI frame format:")
    lines.append("  Bits [22:0]:  Address + data (format depends on"
                 " SpiAddressDataFormat)")
    lines.append("  Bit  23:      R/W flag (0=write, 1=read)")
    lines.append("  Bits [31:24]: Must be zero for writes")
    lines.append("")
    lines.append("SPI configurations:")
    lines.append("  SpiClockEdge:        clock polarity select")
    lines.append("  SpiWireConfiguration: 3-wire or 4-wire SPI")
    lines.append("  SpiAddressDataFormat: bit packing of addr+data")
    lines.append("=" * 78)
    lines.append("")

    spi_funcs = ["WriteSpi", "ReadSpi_7Bit", "ReadSpi_15Bit"]
    for sf_name in spi_funcs:
        matches = find_functions_containing(program, sf_name)
        for m in matches:
            lines.append("  {} @ {}:".format(
                m.getName()[:120], m.getEntryPoint()))
            lines.append("    Size: {} bytes".format(
                m.getBody().getNumAddresses()))

            constants = extract_immediate_constants(program, m, 300)
            if constants:
                lines.append("    Constants (SPI framing):")
                for val, addr, mnemonic, op, full_inst in constants:
                    annotations = classify_spi_constant(val)
                    annotation_str = ""
                    if annotations:
                        annotation_str = "  <-- " + "; ".join(annotations)
                    lines.append("      0x{:08x}: {} {}{}".format(
                        addr.getOffset(), full_inst[:60], " " * max(
                            0, 60 - len(full_inst[:60])),
                        annotation_str))

            str_refs = get_string_refs_in_function(program, m)
            if str_refs:
                lines.append("    String refs:")
                for s, saddr in str_refs[:20]:
                    lines.append("      @ {}: \"{}\"".format(
                        saddr, s[:100]))

            callers = get_callers(program, m)
            if callers:
                lines.append("    Callers ({}):" .format(len(callers)))
                for c, caddr in callers[:10]:
                    lines.append("      <- {} @ {}".format(
                        c.getName()[:80], c.getEntryPoint()))

            lines.append("")

    # SetField for SPI enums
    setfield_funcs = find_functions_containing(program, "SetField")
    spi_setfields = [f for f in setfield_funcs
                     if "Spi" in f.getName() or "MsoFpga" in f.getName()]
    if spi_setfields:
        lines.append("  SPI-related SetField instantiations:")
        for f in spi_setfields:
            lines.append("    {} @ {}".format(
                f.getName()[:120], f.getEntryPoint()))
            constants = extract_immediate_constants(program, f, 100)
            if constants:
                for val, addr, mnemonic, op, full_inst in constants:
                    lines.append("      {} @ {}".format(
                        full_inst[:80], addr))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 7: Board revision and ADC type selection
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 7: BOARD REVISION AND ADC TYPE SELECTION")
    lines.append("=" * 78)
    lines.append("")

    rev_funcs = ["ParseBoardRevision", "UsesNewCryptoInterface",
                 "CommonDeviceInit"]
    for sf_name in rev_funcs:
        matches = find_functions_containing(program, sf_name)
        for m in matches:
            lines.append("  {} @ {}:".format(
                m.getName()[:100], m.getEntryPoint()))
            lines.append("    Size: {} bytes".format(
                m.getBody().getNumAddresses()))

            constants = extract_immediate_constants(program, m, 500)
            if constants:
                lines.append("    Constants:")
                for val, addr, mnemonic, op, full_inst in constants[:30]:
                    lines.append("      0x{:08x}: {}".format(
                        addr.getOffset(), full_inst[:80]))

            str_refs = get_string_refs_in_function(program, m)
            if str_refs:
                lines.append("    String refs:")
                for s, saddr in str_refs[:20]:
                    lines.append("      @ {}: \"{}\"".format(
                        saddr, s[:100]))

            callees = get_callees(program, m)
            if callees:
                lines.append("    Callees ({}):" .format(len(callees)))
                for c, caddr in callees[:15]:
                    lines.append("      -> {} @ {}".format(
                        c.getName()[:80], c.getEntryPoint()))

            lines.append("")

    # ------------------------------------------------------------------
    # Part 8: ADC gain and calibration
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 8: ADC GAIN AND CALIBRATION")
    lines.append("=" * 78)
    lines.append("")

    gain_funcs = ["SetAdcFullScaleGain", "HandleSetAdcFullScaleGain",
                  "FullScaleKernel"]
    for sf_name in gain_funcs:
        matches = find_functions_containing(program, sf_name)
        for m in matches:
            lines.append("  {} @ {}:".format(
                m.getName()[:100], m.getEntryPoint()))
            lines.append("    Size: {} bytes".format(
                m.getBody().getNumAddresses()))

            constants = extract_immediate_constants(program, m, 300)
            if constants:
                interesting = [(v, a, mn, o, fi) for v, a, mn, o, fi
                               in constants if v > 0]
                if interesting:
                    lines.append("    Constants:")
                    for val, addr, mnemonic, op, full_inst in \
                            interesting[:20]:
                        lines.append("      0x{:08x}: {}".format(
                            addr.getOffset(), full_inst[:80]))

            str_refs = get_string_refs_in_function(program, m)
            if str_refs:
                lines.append("    String refs:")
                for s, saddr in str_refs[:15]:
                    lines.append("      @ {}: \"{}\"".format(
                        saddr, s[:100]))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 9: Call graph -- who calls what during ADC init
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 9: ADC INIT CALL GRAPH")
    lines.append("(Starting from CommonDeviceInit, trace the ADC setup path)")
    lines.append("=" * 78)
    lines.append("")

    # Build a call graph starting from CommonDeviceInit
    roots = find_functions_containing(program, "CommonDeviceInit")
    if not roots:
        # Fallback: look for functions that call InitHmcad1100
        hmcad_inits = find_functions_containing(program, "InitHmcad1100")
        for h in hmcad_inits:
            callers = get_callers(program, h)
            for c, _ in callers:
                roots.append(c)
                lines.append("  Root (caller of InitHmcad1100): {} @ {}".format(
                    c.getName()[:80], c.getEntryPoint()))

    for root in roots:
        lines.append("  ROOT: {} @ {}".format(
            root.getName()[:80], root.getEntryPoint()))

        # First level callees
        callees_l1 = get_callees(program, root)
        for c1, addr1 in callees_l1:
            name1 = c1.getName()
            # Filter to ADC-related callees
            adc_keywords = ["ADC", "Adc", "adc", "Hmcad", "Ad9637",
                            "Ads528", "Spi", "SPI", "Jesd", "JESD",
                            "Register", "Init", "Enable", "Power",
                            "Bank", "Revision", "Board", "Config",
                            "Calibrat", "Test", "bitslip"]
            is_adc_related = any(kw in name1 for kw in adc_keywords)

            if is_adc_related or len(name1) < 40:
                marker = "  " if not is_adc_related else "**"
                lines.append("  {}  -> {} @ {} (from {})".format(
                    marker, name1[:80], c1.getEntryPoint(), addr1))

                # Second level for ADC-related functions
                if is_adc_related:
                    callees_l2 = get_callees(program, c1)
                    for c2, addr2 in callees_l2[:8]:
                        lines.append("          -> {} @ {}".format(
                            c2.getName()[:70], c2.getEntryPoint()))

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
    println("ADC init functions found: {}/{}".format(
        len(found_functions), len(ADC_INIT_FUNCTIONS)))
    println("ADC-related strings found: {}".format(len(format_strings)))
    println("Report: {}".format(report_path))


# Entry point
if __name__ == "__main__":
    main()
else:
    main()
