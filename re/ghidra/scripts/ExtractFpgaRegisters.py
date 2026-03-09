# ExtractFpgaRegisters.py -- Ghidra script to extract FPGA register addresses
# from libgraph_server_shared.so
#
# Locates the "FPGA WriteRegister" and "FPGA ReadRegister" format strings,
# finds all code references to them, and extracts nearby register address
# immediates (movw $imm16 instructions) and label strings (LEA to .rodata).
#
# Also traces the CalculateDigitalConfigurationAddress functions to extract
# the per-channel register address formulas.
#
# Usage: Run from Ghidra's Script Manager on libgraph_server_shared.so
#
# @category Saleae
# @author RE team

from ghidra.program.model.listing import CodeUnit
from ghidra.program.util import DefinedDataIterator

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Format strings used in the FPGA register logging code
WRITE_FORMAT = "FPGA WriteRegister 0x{:04x} = 0x{:08x} ({})"
READ_FORMAT = "FPGA ReadRegister 0x{:04X} = 0x{:08X}"
ADC_WRITE_FORMAT = "ADC Write 0x{:x} = 0x{:x}"
ADC_READ_FORMAT = "ADC Read 0x{:x} = 0x{:x}"

# Function signatures that contain register access code
FPGA_FUNCTIONS = [
    "void Saleae::MsoFpga::SetAnalogChannelConfiguration",
    "void Saleae::MsoFpga::SetAnalogTriggerConfiguration",
    "void Saleae::MsoFpga::InternalSetDigitalConfiguration",
    "void Saleae::MsoFpga::SetTestDataConfiguration",
    "void Saleae::MsoFpga::ResetJesdPhyTransceivers",
    "void Saleae::MsoFpga::WriteSpi",
    "void Saleae::MsoFpga::SetEnabledSmartCableDigitalChannels",
    "uint16_t Saleae::FpgaRegisters::CalculateDigitalConfigurationAddress_DigitalDerived",
    "uint16_t Saleae::FpgaRegisters::CalculateDigitalConfigurationAddress_SmartCable",
    "void Saleae::SetDigitalTestPattern",
]

# Known register addresses extracted from binary analysis
KNOWN_REGISTERS = {
    0x0004: "SPI_DATA",
    0x0008: "STATUS_DDR_JESD_CONTROL",
    0x0020: "GPIF_ENABLE",
    0x0024: "GPIF_FLUSH_TIMEOUT",
    0x0030: "TEST_DATA_CONFIG_0",
    0x0034: "TEST_DATA_CONFIG_1",
    0x0040: "CAPTURE_CONTROL",
    0x0044: "SCOPE_CONFIG_ID_0",
    0x0048: "SCOPE_CONFIG_ID_1",
    0x004C: "SCOPE_CONFIG_ID_2",
    0x0050: "SCOPE_CONFIG_ID_3",
    0x0054: "ANALOG_CH_CONFIG_0",
    0x0058: "ANALOG_CH_CONFIG_1",
    0x005C: "ANALOG_CH_CONFIG_2",
    0x0060: "ANALOG_CH_CONFIG_3",
    0x0064: "ANALOG_CH_CONFIG_4",
    0x0068: "ANALOG_CH_CONFIG_5",
    0x0070: "TRIGGER_CONTROL",
    0x0074: "TRIGGER_CONFIG_0",
    0x0078: "TRIGGER_CONFIG_1",
    0x007C: "TRIGGER_CONFIG_2",
    0x0080: "TRIGGER_HOLDOFF_LO",
    0x0084: "TRIGGER_HOLDOFF_HI",
    0x0088: "TRIGGER_TIMEOUT_LO",
    0x008C: "TRIGGER_TIMEOUT_HI",
    0x0090: "TRIGGER_CONFIG_3",
    0x0094: "TRIGGER_TIMESTAMP_SIZE",
    0x0098: "SW_TRIGGER_LO",
    0x009C: "SW_TRIGGER_HI",
    0x00E0: "INIT_CONFIG",
    0x00FC: "JESD_BER_TEST",
    0x0104: "DIGITAL_VOLTAGE_THRESHOLD",
    0x0170: "DIGITAL_CH_CONFIG_0",
    0x0174: "DIGITAL_CH_CONFIG_1",
    0x0178: "DIGITAL_CH_CONFIG_2",
    0x017C: "DIGITAL_CH_CONFIG_3",
    0x01C0: "COMMIT_BANK_CONTROL",
    0x01C4: "DIGITAL_TEST_PATTERN",
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def find_defined_strings(program, search_strings):
    """Find addresses of defined string data matching search_strings."""
    results = {}
    for data in DefinedDataIterator.definedStrings(program):
        val = data.getValue()
        if val is None:
            continue
        s = str(val)
        for target in search_strings:
            if target in s:
                results.setdefault(target, []).append(data.getAddress())
    return results


def get_references_to(program, addr):
    """Get all references pointing to an address."""
    ref_mgr = program.getReferenceManager()
    return ref_mgr.getReferencesTo(addr)


def get_function_containing(program, addr):
    """Get the function containing a given address."""
    fm = program.getFunctionManager()
    return fm.getFunctionContaining(addr)


def extract_movw_immediates(program, start_addr, window=256):
    """Extract all MOVW immediate values from instructions in a window.

    These are 16-bit immediate-to-memory moves that typically set up
    the register address argument before a WriteRegisters call.
    """
    listing = program.getListing()
    end_addr = start_addr.add(window)
    results = []

    inst = listing.getInstructionAt(start_addr)
    if inst is None:
        inst = listing.getInstructionAfter(start_addr)

    while inst is not None and inst.getAddress().compareTo(end_addr) < 0:
        mnemonic = inst.getMnemonicString().upper()
        if mnemonic == "MOV" or mnemonic == "MOVW":
            num_ops = inst.getNumOperands()
            for i in range(num_ops):
                op_type = inst.getOperandType(i)
                if (op_type & CodeUnit.SCALAR_OPERAND) != 0:
                    scalar = inst.getScalar(i)
                    if scalar is not None:
                        uval = scalar.getUnsignedValue()
                        # Filter to plausible 16-bit register addresses
                        if 0 < uval < 0x200:
                            results.append({
                                'addr': inst.getAddress(),
                                'value': uval,
                                'instruction': str(inst),
                            })
        inst = inst.getNext()

    return results


def find_string_refs_near(program, code_addr, window=128):
    """Find LEA instructions loading .rodata strings near code_addr.

    Returns list of (instruction_addr, string_addr, string_text) tuples.
    """
    listing = program.getListing()
    results = []
    start = code_addr.subtract(window)
    end = code_addr.add(16)

    inst = listing.getInstructionAt(start)
    if inst is None:
        inst = listing.getInstructionAfter(start)

    while inst is not None and inst.getAddress().compareTo(end) < 0:
        mnemonic = inst.getMnemonicString().upper()
        if mnemonic == "LEA":
            # Check if any operand references a data location
            for i in range(inst.getNumOperands()):
                refs = inst.getOperandReferences(i)
                for ref in refs:
                    to_addr = ref.getToAddress()
                    data = listing.getDataAt(to_addr)
                    if data is not None and data.hasStringValue():
                        text = str(data.getValue())
                        if len(text) > 2 and text.isprintable():
                            results.append((inst.getAddress(), to_addr, text))
        inst = inst.getNext()

    return results


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    program = currentProgram
    println("=" * 70)
    println("ExtractFpgaRegisters.py -- FPGA Register Map Extractor")
    println("=" * 70)

    # Step 1: Find format strings
    println("\n[1] Searching for FPGA register format strings...")
    targets = [WRITE_FORMAT, READ_FORMAT, ADC_WRITE_FORMAT, ADC_READ_FORMAT]
    string_addrs = find_defined_strings(program, targets)

    for s, addrs in sorted(string_addrs.items()):
        println("  FOUND: '{}' at {}".format(
            s[:60] + "..." if len(s) > 60 else s,
            ", ".join(str(a) for a in addrs)))

    not_found = [s for s in targets if s not in string_addrs]
    for s in not_found:
        println("  MISSING: '{}'".format(s[:60]))

    # Step 2: Find all code references to WriteRegister format string
    println("\n[2] Finding code references to WriteRegister format string...")
    write_refs = []
    if WRITE_FORMAT in string_addrs:
        for str_addr in string_addrs[WRITE_FORMAT]:
            refs = get_references_to(program, str_addr)
            for ref in refs:
                from_addr = ref.getFromAddress()
                func = get_function_containing(program, from_addr)
                func_name = func.getName() if func else "unknown"
                write_refs.append((from_addr, func_name))
                println("  xref at {} in {}".format(from_addr, func_name))

    println("  Total WriteRegister xrefs: {}".format(len(write_refs)))

    # Step 3: Extract register addresses near each xref
    println("\n[3] Extracting register addresses near WriteRegister calls...")
    discovered_regs = {}

    for xref_addr, func_name in write_refs:
        # Look backward from xref for movw $imm instructions
        search_start = xref_addr.subtract(128)
        movws = extract_movw_immediates(program, search_start, window=128)

        # The last movw before the xref is most likely the register address
        if movws:
            last = movws[-1]
            reg_addr = last['value']
            name = KNOWN_REGISTERS.get(reg_addr, "UNKNOWN")
            discovered_regs.setdefault(reg_addr, []).append(func_name)
            println("  xref 0x{}: reg 0x{:04X} ({}) in {}".format(
                xref_addr, reg_addr, name, func_name))

        # Also find nearby label strings
        nearby = find_string_refs_near(program, xref_addr, window=96)
        for inst_addr, str_addr, text in nearby:
            if "FPGA" not in text and "WriteRegister" not in text:
                if len(text) < 80:
                    println("    label: '{}'".format(text))

    # Step 4: Find FPGA function signatures
    println("\n[4] Searching for FPGA function signatures...")
    func_addrs = find_defined_strings(program, FPGA_FUNCTIONS)
    for sig, addrs in sorted(func_addrs.items()):
        println("  FOUND: {} at {}".format(
            sig[:70] + "..." if len(sig) > 70 else sig,
            ", ".join(str(a) for a in addrs)))

    # Step 5: Print known register map
    println("\n" + "=" * 70)
    println("KNOWN FPGA REGISTER MAP")
    println("=" * 70)
    println("")
    println("Protocol: 16-bit addresses, 32-bit values, big-endian on wire")
    println("Transport: USB bulk EP1 OUT/IN via FX3 microcontroller")
    println("")

    for addr in sorted(KNOWN_REGISTERS.keys()):
        name = KNOWN_REGISTERS[addr]
        funcs = discovered_regs.get(addr, [])
        func_str = ", ".join(set(funcs)) if funcs else ""
        println("  0x{:04X}  {:36s}  {}".format(addr, name, func_str))

    println("")
    println("Total mapped registers: {}".format(len(KNOWN_REGISTERS)))

    # Step 6: Print register ranges
    println("")
    println("Register Address Ranges:")
    println("  0x0004-0x0008  SPI / Status / JESD control")
    println("  0x0020-0x0024  GPIF (FX3 data interface) control")
    println("  0x0030-0x0034  Test data generation")
    println("  0x0040-0x0068  Capture control, scope config IDs, analog ch config")
    println("  0x0070-0x009C  Trigger configuration and software trigger")
    println("  0x00E0         Post-programming initialization")
    println("  0x00FC         JESD BER test")
    println("  0x0104         Digital voltage threshold")
    println("  0x0170-0x017C  Per-channel digital config (addr = 0x170 + ch*4)")
    println("  0x0180-0x01BF  Smart cable digital config (MSO only)")
    println("  0x01C0-0x01C4  Commit/bank control, digital test pattern")

    println("")
    println("Key bit fields (from disassembly):")
    println("  Reg 0x0008 bits [4:3]: DDR ready (locked=bit4, calibrated=bit3)")
    println("  Reg 0x0008 bit  [4]:   OR'd with read value during JESD init")
    println("  Digital channel config: glitch filter pulse width, enable bits")
    println("  Trigger: type, mode packed via SetField<TriggerType>, SetField<TriggerMode>")
    println("")

    println("CalculateDigitalConfigurationAddress_DigitalDerived(channel):")
    println("  address = 0x0170 + channel * 4   (channel 0..3)")
    println("")
    println("CalculateDigitalConfigurationAddress_SmartCable(port, channel):")
    println("  base around 0x0180 (MSO only, exact formula needs Ghidra decompilation)")

    println("\n" + "=" * 70)
    println("Done.")


main()
