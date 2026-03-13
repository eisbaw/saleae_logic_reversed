# Ghidra headless script: decompile GPIF width selection and ContractReadData logic.
# Usage: ghidra-analyzeHeadless ghidra_proj2 sal_rev2 \
#   -process libgraph_server_shared.so -noanalysis \
#   -scriptPath re/ghidra/scripts -postScript DecompileGpifContract.py
#
# Output: re/ghidra/decompiled-gpif-contract.txt

from ghidra.app.decompiler import DecompInterface
from ghidra.app.cmd.function import CreateFunctionCmd
from ghidra.app.cmd.disassemble import DisassembleCommand
from ghidra.util.task import ConsoleTaskMonitor

# Ghidra VAs = ELF VA + 0x100000
TARGET_VAS = [
    # ContractReadData - already partially decompiled, but get clean output
    # ELF VA 0x1c3bad0, source: LogicDevice.cpp line 0x16e
    (0x1d3bad0, "ContractReadData"),

    # ContractReadData caller / OnDataBuffer path
    # ELF VA 0x1c3bca0
    (0x1d3bca0, "ContractReadData_caller"),

    # Logic2FpgaDevice::StartSampling caller of DeviceCalculateMixRate
    # ELF VA 0x1c24fb9 is mid-function, find the containing function
    (0x1d24fb9, "StartSampling_caller1_of_MixRate"),

    # Second caller in Logic2FpgaDevice.cpp
    # ELF VA 0x1c26c80
    (0x1d26c80, "StartSampling_caller2_of_MixRate"),

    # DeviceSettings caller of DeviceCalculateMixRate
    # ELF VA 0x1c4a6f9
    (0x1d4a6f9, "DeviceSettings_caller_of_MixRate"),

    # DeviceCalculateMixRate itself for clean reference
    # ELF VA 0x1c19fa0
    (0x1d19fa0, "DeviceCalculateMixRate"),

    # Constructor that sets this+0x19c = 1 (from decompiled-eeprom.txt, around 0x1d399f1)
    # Let's find the function containing this address
    (0x1d399f1, "Constructor_sets_LowSpeedDivider"),

    # ExpandWriteData (inverse of ContractReadData)
    # ELF VA 0x1c3bef0
    (0x1d3bef0, "ExpandWriteData"),
]

monitor = ConsoleTaskMonitor()
decomp = DecompInterface()
decomp.openProgram(currentProgram)
# Timeout set per-decompile call (300s)

af = currentProgram.getAddressFactory()
space = af.getDefaultAddressSpace()
fm = currentProgram.getFunctionManager()
refMgr = currentProgram.getReferenceManager()

output_lines = []
output_lines.append("=" * 78)
output_lines.append("GPIF WIDTH SELECTION + CONTRACTREADDATA DECOMPILATION")
output_lines.append("Binary: libgraph_server_shared.so v2.4.40")
output_lines.append("=" * 78)
output_lines.append("")

# -----------------------------------------------------------------------
# Part 1: Decompile target functions
# -----------------------------------------------------------------------

for va, name in TARGET_VAS:
    addr = space.getAddress(va)

    # Try to get function at exact address
    func = getFunctionAt(addr)
    if func is None:
        # Try to disassemble and create function
        disCmd = DisassembleCommand(addr, None, True)
        disCmd.applyTo(currentProgram, monitor)
        cmd = CreateFunctionCmd(addr)
        cmd.applyTo(currentProgram, monitor)
        func = getFunctionAt(addr)

    if func is None:
        # Try function containing this address
        func = getFunctionContaining(addr)
        if func is None:
            output_lines.append("=== {} (Ghidra VA {:#x}): NO FUNCTION ===".format(name, va))
            output_lines.append("")
            continue
        output_lines.append("=== {} (Ghidra VA {:#x}): in function '{}' at {:#x}, size={} ===".format(
            name, va, func.getName(), func.getEntryPoint().getOffset(),
            func.getBody().getNumAddresses()))
    else:
        output_lines.append("=== {} (Ghidra VA {:#x}): function '{}', size={} ===".format(
            name, va, func.getName(), func.getBody().getNumAddresses()))

    result = decomp.decompileFunction(func, 300, monitor)
    if result.decompileCompleted():
        c_code = result.getDecompiledFunction().getC()
        output_lines.append(c_code)
    else:
        output_lines.append("  DECOMPILATION FAILED: {}".format(result.getErrorMessage()))
    output_lines.append("")
    output_lines.append("")

# -----------------------------------------------------------------------
# Part 2: Find all xrefs to "_LowSpeedDivider" string
# -----------------------------------------------------------------------

output_lines.append("=" * 78)
output_lines.append("XREFS TO _LowSpeedDivider STRING")
output_lines.append("=" * 78)
output_lines.append("")

# The string is at file offset 0x477af7 -> VA = 0x577af7 (+ 0x100000 load base)
# Actually, let's search for it properly
from ghidra.program.model.listing import Data
import ghidra.program.util as putil

# Search for the string in defined data
listing = currentProgram.getListing()
data_iter = listing.getDefinedData(True)
lowspeed_addrs = []
for d in data_iter:
    if d.hasStringValue():
        val = d.getValue()
        if val is not None and "_LowSpeedDivider" in str(val):
            lowspeed_addrs.append(d.getAddress())
            output_lines.append("Found string '{}' at {}".format(val, d.getAddress()))

# Get xrefs to these string addresses
for str_addr in lowspeed_addrs:
    refs = getReferencesTo(str_addr)
    for ref in refs:
        from_addr = ref.getFromAddress()
        func = getFunctionContaining(from_addr)
        if func:
            output_lines.append("  Referenced from {} in function {} at {}".format(
                from_addr, func.getName(), func.getEntryPoint()))
        else:
            output_lines.append("  Referenced from {} (no function)".format(from_addr))

output_lines.append("")

# -----------------------------------------------------------------------
# Part 3: Find functions that write to offset 0x19c on their this pointer
# This is harder to do generically. Let's try finding references to
# the ContractReadData function to find its callers, which will show
# how LowSpeedDivider is used.
# -----------------------------------------------------------------------

output_lines.append("=" * 78)
output_lines.append("CALLERS OF ContractReadData (0x1d3bad0)")
output_lines.append("=" * 78)
output_lines.append("")

contract_addr = space.getAddress(0x1d3bad0)
contract_func = getFunctionAt(contract_addr)
if contract_func:
    refs = getReferencesTo(contract_addr)
    for ref in refs:
        from_addr = ref.getFromAddress()
        caller = getFunctionContaining(from_addr)
        if caller:
            output_lines.append("Called from {} in function {} at {} (size={})".format(
                from_addr, caller.getName(), caller.getEntryPoint(),
                caller.getBody().getNumAddresses()))
        else:
            output_lines.append("Called from {} (no function)".format(from_addr))

output_lines.append("")

# -----------------------------------------------------------------------
# Part 4: Search for "No valid analog and gpif width combo" string xrefs
# -----------------------------------------------------------------------

output_lines.append("=" * 78)
output_lines.append("XREFS TO 'No valid analog and gpif width combo' STRING")
output_lines.append("=" * 78)
output_lines.append("")

data_iter = listing.getDefinedData(True)
combo_addrs = []
for d in data_iter:
    if d.hasStringValue():
        val = d.getValue()
        if val is not None and "No valid analog" in str(val):
            combo_addrs.append(d.getAddress())
            output_lines.append("Found string '{}' at {}".format(val, d.getAddress()))

for str_addr in combo_addrs:
    refs = getReferencesTo(str_addr)
    for ref in refs:
        from_addr = ref.getFromAddress()
        func = getFunctionContaining(from_addr)
        if func:
            output_lines.append("  Referenced from {} in function {} at {}".format(
                from_addr, func.getName(), func.getEntryPoint()))
        else:
            output_lines.append("  Referenced from {} (no function)".format(from_addr))

output_lines.append("")

# -----------------------------------------------------------------------
# Part 5: Look for writes to offset 0x19c in the capture path
# Find the function(s) that set up capture parameters.
# Search for FUN_01d3bca0 callers (the ContractReadData_caller)
# -----------------------------------------------------------------------

output_lines.append("=" * 78)
output_lines.append("CALLERS OF ContractReadData_caller (0x1d3bca0)")
output_lines.append("=" * 78)
output_lines.append("")

caller_addr = space.getAddress(0x1d3bca0)
caller_func = getFunctionAt(caller_addr)
if caller_func is None:
    caller_func = getFunctionContaining(caller_addr)

if caller_func:
    refs = getReferencesTo(caller_func.getEntryPoint())
    for ref in refs:
        from_addr = ref.getFromAddress()
        outer = getFunctionContaining(from_addr)
        if outer:
            output_lines.append("Called from {} in function {} at {} (size={})".format(
                from_addr, outer.getName(), outer.getEntryPoint(),
                outer.getBody().getNumAddresses()))
        else:
            output_lines.append("Called from {} (no function)".format(from_addr))
else:
    output_lines.append("Could not find function at/containing 0x1d3bca0")

output_lines.append("")

# -----------------------------------------------------------------------
# Part 6: Try to find where 0x19c is written with values other than 1
# Search the vtable of the object that owns ContractReadData
# The constructor at decompiled-eeprom.txt set PTR_FUN_02f483e0 as vtable
# Let's look at functions referenced through that vtable
# -----------------------------------------------------------------------

output_lines.append("=" * 78)
output_lines.append("VTABLE AT PTR_FUN_02f483e0 (LogicDevice vtable)")
output_lines.append("=" * 78)
output_lines.append("")

vtable_addr = space.getAddress(0x2f483e0)
# Read first 40 entries (pointers) from the vtable
for i in range(40):
    entry_addr = vtable_addr.add(i * 8)
    ptr_val = getLong(entry_addr)
    if ptr_val != 0:
        target = space.getAddress(ptr_val)
        func = getFunctionAt(target)
        if func:
            output_lines.append("  vtable[{}] = {} -> {} (size={})".format(
                i, target, func.getName(), func.getBody().getNumAddresses()))
        else:
            output_lines.append("  vtable[{}] = {}".format(i, target))

output_lines.append("")

decomp.dispose()

outpath = "/home/mpedersen/nvidia/saleae_reverse/re/ghidra/decompiled-gpif-contract.txt"
with open(outpath, "w") as f:
    f.write("\n".join(output_lines))

print("Output written to " + outpath)
