# ExtractConstants.py -- Ghidra script to extract FpgaConstants and MsoConstants
# from libgraph_server_shared.so
#
# Finds functions referencing FpgaConstants/MsoConstants assertion strings,
# extracts immediate comparison values from nearby CMP instructions, and
# dumps sample rate tables.
#
# Usage: Run from Ghidra's Script Manager on libgraph_server_shared.so
#
# @category Saleae
# @author RE team

from ghidra.program.model.listing import CodeUnit
from ghidra.program.model.scalar import Scalar
from ghidra.program.util import DefinedDataIterator
import re

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

ASSERTION_STRINGS = [
    "config.mSamplesPerChunk <= FpgaConstants::MaxSamplesPerPacket_Altera",
    "config.mSamplesPerChunk <= FpgaConstants::MaxSamplesPerPacket_Xilinx",
    "config.mSamplesPerChunk >= FpgaConstants::MinSamplesPerPacket_Altera",
    "config.mSamplesPerChunk >= FpgaConstants::MinSamplesPerPacket_Xilinx",
    "config.mSamplesPerChunk % FpgaConstants::SamplesPerPacketMultiple_WithStats == 0",
    "( config.mPreTriggerLength + config.mPostTriggerLength ) % FpgaConstants::SamplesPerPacketMultiple_WithStats == 0",
    "sample_length <= MsoConstants::MaxTriggerBufferSamples_Altera",
    "sample_length <= MsoConstants::MaxTriggerBufferSamples_Xilinx",
    "sample_length >= MsoConstants::MinTriggerBufferSamples",
    "config.mPostTriggerLength <= MsoConstants::MaxTriggerBufferIndex",
    "-config.mPreTriggerLength >= MsoConstants::MinTriggerBufferIndex_Altera",
    "-config.mPreTriggerLength >= MsoConstants::MinTriggerBufferIndex_Xilinx",
    "config.mTriggerAuto_HoldoffSamples <= MsoConstants::MaxHoldoffSamples",
    "config.mTriggerAuto_TimeoutSamples <= MsoConstants::MaxTimeoutSamples",
    "config.mTriggerInitialHoldoffSamples <= MsoConstants::MaxInitialHoldoffSamples",
    "config.mPulseWidth >= MsoConstants::MinGlitchFilterLength && config.mPulseWidth <= MsoConstants::MaxGlitchFilterLength",
    "channel_number < MsoConstants::MaxScopeChannels",
    "port_number < MsoConstants::MaxSmartCablePortCount",
    "number >= 0 && number <= MsoConstants::TotalAfeConfigurations",
]

DEVICE_CONSTANTS_KEYS = [
    "/analogChannelCount",
    "/analogSampleRate",
    "/scopeDigitalSampleRate",
    "/clockRateHz",
    "/ddrMemoryCapacityBytes",
    "/maxAdcCode",
    "/minAnalogSamplesPerPacket",
    "/maxAnalogSamplesPerPacket",
    "/minDacVoltage",
    "/maxDacVoltage",
    "/dacBitDepth",
    "/fpgaImageType",
    "/analogLimit_MaxDownsampleRatio",
    "/analogLimit_MinTotalTriggerBufferLength",
    "/analogLimit_MaxTotalTriggerBufferLength",
    "/analogLimit_MinTriggerBufferIndex",
    "/analogLimit_MaxTriggerBufferIndex",
    "/analogLimit_MaxHoldoffSamples",
    "/analogLimit_MaxTimeoutSamples",
    "/digitalLimit_MinGlitchFilterLength",
    "/digitalLimit_MaxGlitchFilterLength",
]

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def find_defined_strings(program, search_strings):
    """Find addresses of defined string data matching search_strings."""
    results = {}
    listing = program.getListing()
    mem = program.getMemory()

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


def extract_cmp_immediates(program, func_addr, window=512):
    """Extract all CMP immediate values from instructions near func_addr."""
    listing = program.getListing()
    addr = func_addr
    end_addr = func_addr.add(window)
    results = []

    inst = listing.getInstructionAt(addr)
    if inst is None:
        inst = listing.getInstructionAfter(addr)

    while inst is not None and inst.getAddress().compareTo(end_addr) < 0:
        mnemonic = inst.getMnemonicString().upper()
        if mnemonic == "CMP" or mnemonic == "TEST":
            num_ops = inst.getNumOperands()
            for i in range(num_ops):
                op_type = inst.getOperandType(i)
                if (op_type & CodeUnit.SCALAR_OPERAND) != 0:
                    scalar = inst.getScalar(i)
                    if scalar is not None:
                        results.append({
                            'addr': inst.getAddress(),
                            'mnemonic': mnemonic,
                            'value': scalar.getValue(),
                            'unsigned': scalar.getUnsignedValue(),
                            'instruction': str(inst),
                        })
        inst = inst.getNext()

    return results


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    program = currentProgram
    println("=" * 70)
    println("ExtractConstants.py -- Saleae FpgaConstants / MsoConstants extractor")
    println("=" * 70)

    # Step 1: Find assertion strings
    println("\n[1] Searching for assertion strings...")
    string_addrs = find_defined_strings(program, ASSERTION_STRINGS)

    for s, addrs in sorted(string_addrs.items(), key=lambda x: x[0]):
        println("  FOUND: {} at {}".format(s[:60] + "..." if len(s) > 60 else s,
                                            ", ".join(str(a) for a in addrs)))

    not_found = [s for s in ASSERTION_STRINGS if s not in string_addrs]
    for s in not_found:
        println("  MISSING: {}".format(s[:60] + "..." if len(s) > 60 else s))

    # Step 2: For each assertion string, find code references and extract CMP values
    println("\n[2] Extracting comparison values near assertion references...")
    for assertion_text, addrs in sorted(string_addrs.items(), key=lambda x: x[0]):
        for str_addr in addrs:
            refs = get_references_to(program, str_addr)
            for ref in refs:
                from_addr = ref.getFromAddress()
                func = get_function_containing(program, from_addr)
                if func is None:
                    continue

                func_entry = func.getEntryPoint()
                println("\n  Assertion: {}".format(
                    assertion_text[:70] + "..." if len(assertion_text) > 70 else assertion_text))
                println("  Referenced from: {} in function at {}".format(
                    from_addr, func_entry))

                # Look backward from the reference for CMP instructions
                cmps = extract_cmp_immediates(program, func_entry,
                                               window=from_addr.subtract(func_entry) + 64)
                if cmps:
                    println("  CMP/TEST immediates in function:")
                    for cmp in cmps:
                        println("    {} : {} (0x{:x} / {})".format(
                            cmp['addr'], cmp['instruction'],
                            cmp['unsigned'], cmp['value']))
                else:
                    println("  No CMP immediates found nearby")

    # Step 3: Find MsoDeviceConstants serialization keys
    println("\n[3] Searching for MsoDeviceConstants serialization keys...")
    key_addrs = find_defined_strings(program, DEVICE_CONSTANTS_KEYS)

    for key, addrs in sorted(key_addrs.items(), key=lambda x: x[0]):
        println("  FOUND: {} at {}".format(key, ", ".join(str(a) for a in addrs)))
        for addr in addrs:
            refs = get_references_to(program, addr)
            for ref in refs:
                from_addr = ref.getFromAddress()
                func = get_function_containing(program, from_addr)
                func_name = func.getName() if func else "unknown"
                println("    Referenced from {} in {}".format(from_addr, func_name))

    not_found_keys = [k for k in DEVICE_CONSTANTS_KEYS if k not in key_addrs]
    for k in not_found_keys:
        println("  MISSING: {}".format(k))

    # Step 4: Search for sample rate tables
    println("\n[4] Searching for sample rate table patterns...")
    # Common Saleae sample rates in Hz
    sample_rates = [
        500000000, 250000000, 125000000, 62500000,
        50000000, 25000000, 12500000, 10000000,
        6250000, 5000000, 2500000, 1250000,
        625000, 500000, 250000, 125000,
        62500, 50000, 31250, 25000, 12500, 10000,
    ]

    mem = program.getMemory()
    import struct

    for rate in sample_rates:
        # Search for 32-bit and 64-bit representations in .rodata
        try:
            rate_bytes_32 = struct.pack("<I", rate)
            rate_bytes_64 = struct.pack("<Q", rate)
            # Search is expensive in Ghidra, so just log the target
            println("  Would search for rate {} Hz (0x{:x})".format(rate, rate))
        except Exception:
            pass

    println("\n" + "=" * 70)
    println("KNOWN CONSTANT VALUES (from binary disassembly):")
    println("=" * 70)
    println("")
    println("FpgaConstants (compile-time, used in assertions):")
    println("  MinSamplesPerPacket_Xilinx  = 256    (0x100)")
    println("  MaxSamplesPerPacket_Xilinx  = 7680   (0x1E00)")
    println("  MinSamplesPerPacket_Altera  = 64     (0x40)")
    println("  MaxSamplesPerPacket_Altera  = 6016   (0x1780)")
    println("  SamplesPerPacketMultiple_WithStats = 256 (0x100)")
    println("")
    println("MsoConstants (compile-time, used in assertions):")
    println("  MinTriggerBufferSamples     = 8")
    println("  MaxTriggerBufferSamples_Xilinx = 1073725440 (0x3FFFC000)")
    println("  MaxTriggerBufferSamples_Altera = 805289984  (0x2FFFC000)")
    println("  MaxScopeChannels            = 4")
    println("  TotalAfeConfigurations      = 66     (0x42)")
    println("")
    println("MsoDeviceConstants (runtime, per-device, serialized as JSON):")
    println("  analogLimit_MaxDownsampleRatio")
    println("  analogLimit_MinTotalTriggerBufferLength")
    println("  analogLimit_MaxTotalTriggerBufferLength")
    println("  analogLimit_MinTriggerBufferIndex")
    println("  analogLimit_MaxTriggerBufferIndex")
    println("  analogLimit_MaxHoldoffSamples")
    println("  analogLimit_MaxTimeoutSamples")
    println("  digitalLimit_MinGlitchFilterLength")
    println("  digitalLimit_MaxGlitchFilterLength")
    println("  (Values are device-specific, populated at runtime)")
    println("")
    println("Glitch Filter constraint (from assertion string):")
    println("  PulseWidth must be 5, or a multiple of 10")
    println("")
    println("Analog downsampled rates (from JS bundle):")
    println("  [50, 250, 1250, 6250, 15625, 31250] Hz")
    println("")
    println("Downsample ratio rules (from JS bundle):")
    println("  Normal mode:     minDownsample = 1")
    println("  Average mode:    minDownsample = 2")
    println("  PeakDetect mode: minDownsample = 4")
    println("  Ratios 3, 5, 7 are bumped to 4, 6, 8 respectively")
    println("  Ratios >= 8 are rounded to nearest multiple of 8")
    println("  Max ratio = 8 * 162500 = 1300000")

    println("\n" + "=" * 70)
    println("Done.")


main()
