# TraceBitstreamDownload.py
# Ghidra script to trace the FPGA bitstream download protocol in
# libgraph_server_shared.so
#
# Finds key functions, maps embedded bitstreams, and traces the USB
# transfer sequence for FPGA programming.
#
# Usage: Run from Ghidra's Script Manager against the libgraph_server_shared.so
# binary imported into a Ghidra project.
#
# @category Saleae
# @author RE Project

from ghidra.program.util import DefinedDataIterator
from ghidra.program.model.symbol import SymbolType
from ghidra.app.decompiler import DecompInterface
import re

def find_string_refs(search_str):
    """Find all locations where a string is referenced."""
    results = []
    listing = currentProgram.getListing()
    mem = currentProgram.getMemory()
    addr_factory = currentProgram.getAddressFactory()

    # Search defined strings
    for data in DefinedDataIterator.definedStrings(currentProgram):
        val = data.getDefaultValueRepresentation()
        if search_str in val:
            addr = data.getAddress()
            refs = getReferencesTo(addr)
            results.append({
                'string': val,
                'string_addr': addr,
                'refs': [r.getFromAddress() for r in refs]
            })
    return results


def find_functions_by_name(name_pattern):
    """Find functions matching a name pattern."""
    results = []
    fm = currentProgram.getFunctionManager()
    for func in fm.getFunctions(True):
        if re.search(name_pattern, func.getName()):
            results.append(func)
    return results


def find_functions_containing_string(search_str):
    """Find functions that reference a given string."""
    results = []
    refs = find_string_refs(search_str)
    fm = currentProgram.getFunctionManager()
    for ref_info in refs:
        for from_addr in ref_info['refs']:
            func = fm.getFunctionContaining(from_addr)
            if func and func not in results:
                results.append(func)
    return results


def decompile_function(func, timeout=60):
    """Decompile a function and return the C code."""
    decomp = DecompInterface()
    decomp.openProgram(currentProgram)
    result = decomp.decompileFunction(func, timeout, monitor)
    if result and result.decompileCompleted():
        return result.getDecompiledFunction().getC()
    return None


def trace_callers(func, depth=3, visited=None):
    """Trace callers of a function up to a given depth."""
    if visited is None:
        visited = set()
    if depth <= 0 or func in visited:
        return []
    visited.add(func)

    callers = []
    refs = getReferencesTo(func.getEntryPoint())
    fm = currentProgram.getFunctionManager()
    for ref in refs:
        caller_func = fm.getFunctionContaining(ref.getFromAddress())
        if caller_func and caller_func != func:
            callers.append({
                'function': caller_func,
                'call_addr': ref.getFromAddress(),
                'depth': depth
            })
            # Recurse
            parent_callers = trace_callers(caller_func, depth - 1, visited)
            callers.extend(parent_callers)
    return callers


def trace_callees(func, depth=2, visited=None):
    """Trace functions called by a given function."""
    if visited is None:
        visited = set()
    if depth <= 0 or func in visited:
        return []
    visited.add(func)

    callees = []
    body = func.getBody()
    fm = currentProgram.getFunctionManager()
    for addr_range in body:
        inst = listing.getInstructionAt(addr_range.getMinAddress())
        while inst is not None and body.contains(inst.getAddress()):
            for ref in inst.getReferencesFrom():
                callee = fm.getFunctionAt(ref.getToAddress())
                if callee and callee != func:
                    callees.append({
                        'function': callee,
                        'call_addr': inst.getAddress(),
                        'depth': depth
                    })
            inst = inst.getNext()
    return callees


listing = currentProgram.getListing()

print("=" * 70)
print("FPGA Bitstream Download Protocol Trace")
print("=" * 70)

# --- Phase 1: Find key FPGA programming functions ---
print("\n--- Phase 1: Key Functions ---\n")

target_strings = [
    "ProgramFpgaBitstream",
    "DownloadBitstream",
    "LoadBitstream",
    "GetFpgaBitstreamVersion",
    "ResetFpga",
    "IsFpgaResponsive",
    "WriteConfigurationData",
    "CommonDeviceInit",
    "Programming FPGA bitstream",
    "Programming FPGA ",
    "Bitstream download took",
    "FPGA readback got",
    "FPGA Programmed?",
    "SALEAE_CHUNKED_FPGA_PROGRAMMING",
    "SALEAE_FPGA_IMAGE",
    "SALEAE_SKIP_PROGRAM_FPGA",
    "does not support lengths larger than 62/509",
    "Lattice PLL and SERDES reset complete",
    "FPGA HW Revision",
    "Skipping FPGA programming",
]

for s in target_strings:
    funcs = find_functions_containing_string(s)
    if funcs:
        for f in funcs:
            print("  String: %-50s -> %s @ %s" % (
                s[:50], f.getName(), f.getEntryPoint()))
    else:
        print("  String: %-50s -> (no function found)" % s[:50])

# --- Phase 2: Trace ProgramFpgaBitstream call chain ---
print("\n--- Phase 2: ProgramFpgaBitstream Call Chain ---\n")

program_funcs = find_functions_containing_string("Programming FPGA bitstream")
if program_funcs:
    for func in program_funcs:
        print("Function: %s @ %s" % (func.getName(), func.getEntryPoint()))

        # Trace callers
        print("  Callers (who calls this?):")
        callers = trace_callers(func, depth=4)
        for c in callers:
            indent = "    " * (5 - c['depth'])
            print("%s<- %s @ %s" % (indent, c['function'].getName(),
                                     c['function'].getEntryPoint()))

        # Try to decompile
        code = decompile_function(func)
        if code:
            print("\n  Decompiled (first 80 lines):")
            for i, line in enumerate(code.split('\n')[:80]):
                print("    %s" % line)
        print()

# --- Phase 3: Trace WriteConfigurationData ---
print("\n--- Phase 3: WriteConfigurationData Analysis ---\n")

wcd_funcs = find_functions_containing_string("WriteConfigurationData")
for func in wcd_funcs:
    print("Function: %s @ %s" % (func.getName(), func.getEntryPoint()))
    code = decompile_function(func)
    if code:
        print("  Decompiled (first 60 lines):")
        for i, line in enumerate(code.split('\n')[:60]):
            print("    %s" % line)
    print()

# --- Phase 4: Trace ResetFpga ---
print("\n--- Phase 4: ResetFpga Analysis ---\n")

reset_funcs = find_functions_containing_string("ResetFpga response")
for func in reset_funcs:
    print("Function: %s @ %s" % (func.getName(), func.getEntryPoint()))
    code = decompile_function(func)
    if code:
        print("  Decompiled (first 40 lines):")
        for i, line in enumerate(code.split('\n')[:40]):
            print("    %s" % line)
    print()

# --- Phase 5: Map embedded bitstream locations ---
print("\n--- Phase 5: Embedded Bitstream Locations ---\n")

# Search for Xilinx sync word AA995566
xilinx_sync = bytes([0xAA, 0x99, 0x55, 0x66])
mem = currentProgram.getMemory()

# Xilinx bitstreams
print("Xilinx Bitstreams (sync word AA995566):")
xilinx_offsets = [
    0x007D9010, 0x00C48A20, 0x010FB860, 0x0114EC00,
    0x011A1E80, 0x011F5100, 0x01248380, 0x012B98D0,
    0x0132AE20, 0x0137E0A0, 0x013D1320,
]
for i, offset in enumerate(xilinx_offsets):
    addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(offset)
    refs = getReferencesTo(addr)
    ref_str = ", ".join([str(r.getFromAddress()) for r in refs]) if refs else "no refs"
    print("  X[%d] @ 0x%08X  refs: %s" % (i, offset, ref_str))

# Lattice bitstreams
print("\nLattice Bitstreams (header string):")
lattice_offsets = [
    0x01442862, 0x01477B02, 0x014B5E42, 0x0150D542, 0x01552D22,
    0x015A2AF2, 0x01610752, 0x01656C92, 0x016B5572, 0x016EA242,
]
lattice_crcs = [
    "0xD8EC", "0xCE24", "0x36E3", "0xF147", "0x25C9",
    "0x74EF", "0xF4ED", "0x6391", "0x80F3", "0xAC6D",
]
lattice_parts = [
    "LFE5UM-25F (Logic8)",
    "LFE5UM-25F (Pro8)",
    "LFE5UM-25F (Pro16)",
    "LFE5UM5G-45F (Logic8)",
    "LFE5UM5G-45F (Pro8)",
    "LFE5UM5G-45F (Pro16)",
    "LFE5U-45F (Logic8)",
    "LFE5U-45F (Pro16)",
    "LFE5U-12F (Logic8)",
    "LFE5U-12F (Pro8)",
]
for i, (offset, crc, part) in enumerate(zip(lattice_offsets, lattice_crcs, lattice_parts)):
    addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(offset)
    refs = getReferencesTo(addr)
    ref_str = ", ".join([str(r.getFromAddress()) for r in refs]) if refs else "no refs"
    print("  L[%d] @ 0x%08X  CRC=%s  %s  refs: %s" % (i, offset, crc, part, ref_str))

# --- Phase 6: Find GetFpgaImage function ---
print("\n--- Phase 6: GetFpgaImage (Bitstream Selection) ---\n")

fpga_image_funcs = find_functions_containing_string("SALEAE_FPGA_IMAGE")
for func in fpga_image_funcs:
    print("Function: %s @ %s" % (func.getName(), func.getEntryPoint()))
    code = decompile_function(func)
    if code:
        print("  Decompiled (first 80 lines):")
        for i, line in enumerate(code.split('\n')[:80]):
            print("    %s" % line)
    print()

# --- Phase 7: Find Revision Detection ---
print("\n--- Phase 7: Hardware Revision Detection ---\n")

rev_funcs = find_functions_containing_string("FPGA HW Revision")
for func in rev_funcs:
    print("Function: %s @ %s" % (func.getName(), func.getEntryPoint()))
    code = decompile_function(func)
    if code:
        print("  Decompiled (first 60 lines):")
        for i, line in enumerate(code.split('\n')[:60]):
            print("    %s" % line)
    print()

# --- Phase 8: Find libusb transfer calls in programming path ---
print("\n--- Phase 8: USB Transfer Functions ---\n")

usb_strings = [
    "libusb_bulk_transfer",
    "libusb_control_transfer",
    "submit_bulk_transfer",
    "submit_control_transfer",
]
for s in usb_strings:
    funcs = find_functions_by_name(s)
    if funcs:
        for f in funcs:
            print("  %s @ %s" % (f.getName(), f.getEntryPoint()))

print("\n" + "=" * 70)
print("Trace complete. Review decompiled output for USB request codes")
print("and bitstream selection logic.")
print("=" * 70)
