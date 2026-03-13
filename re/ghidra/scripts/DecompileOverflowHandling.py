# Ghidra headless script: decompile overflow handling functions.
# Targets the stats parser and overflow processing code.
#
# ELF VAs -> Ghidra VAs (+0x100000):
#   0x2d41510 -> 0x2e41510  stats parser entry (ParseMsoWaveformData)
#   0x2d416c0 -> 0x2e416c0  overflow_meta skip
#   0x2d41887 -> 0x2e41887  overflow_data processing
#   0x2d44b80 -> 0x2e44b80  HandleWaveformData (calls stats parser)
#   0x1c1fd00 -> 0x1d1fd00  PrintErrors (overflow detection)
#
# Usage:
#   ghidra-analyzeHeadless ghidra_proj2 sal_rev2 -process -noanalysis \
#     -scriptPath re/ghidra/scripts -postScript DecompileOverflowHandling.py

from ghidra.app.decompiler import DecompInterface
from ghidra.util.task import ConsoleTaskMonitor
from ghidra.app.cmd.function import CreateFunctionCmd
from ghidra.app.cmd.disassemble import DisassembleCommand

# (Ghidra VA, label) -- Ghidra VA = ELF VA + 0x100000
TARGET_VAS = [
    (0x2e41510, "ParseMsoWaveformData_entry"),
    (0x2e416c0, "overflow_meta_skip"),
    (0x2e41887, "overflow_data_processing"),
    (0x2e44b80, "HandleWaveformData"),
    (0x1d1fd00, "PrintErrors_overflow"),
]

OUTPUT_FILE = "re/ghidra/decompiled-overflow-handling.txt"

monitor = ConsoleTaskMonitor()
space = currentProgram.getAddressFactory().getDefaultAddressSpace()

# Ensure functions exist at target addresses
for va, name in TARGET_VAS:
    addr = space.getAddress(va)
    func = getFunctionAt(addr)
    if func is None:
        func = getFunctionContaining(addr)
    if func is None:
        # Try to disassemble and create function
        disCmd = DisassembleCommand(addr, None, True)
        disCmd.applyTo(currentProgram, monitor)
        cmd = CreateFunctionCmd(addr)
        cmd.applyTo(currentProgram, monitor)
        func = getFunctionAt(addr)
        if func is not None:
            print("Created function '{}' at {:#x}".format(func.getName(), va))
        else:
            print("Could not create function at {:#x} ({})".format(va, name))

# Decompile
decomp = DecompInterface()
decomp.openProgram(currentProgram)

output_lines = []
output_lines.append("// Decompiled overflow handling functions from libgraph_server_shared.so")
output_lines.append("// Ghidra addresses = ELF VA + 0x100000")
output_lines.append("// Target: overflow metadata (16 bytes) and per-block stats (124 bytes)")
output_lines.append("")

seen_funcs = set()

for va, name in TARGET_VAS:
    addr = space.getAddress(va)
    func = getFunctionAt(addr)
    if func is None:
        func = getFunctionContaining(addr)
    if func is None:
        output_lines.append("// === {} (Ghidra VA {:#x}, ELF VA {:#x}): NO FUNCTION ===".format(
            name, va, va - 0x100000))
        output_lines.append("")
        continue

    func_addr = func.getEntryPoint().getOffset()
    elf_va = func_addr - 0x100000

    # Avoid decompiling the same function twice
    if func_addr in seen_funcs:
        output_lines.append("// === {} (Ghidra VA {:#x}): same function as already decompiled (entry {:#x}) ===".format(
            name, va, func_addr))
        output_lines.append("")
        continue
    seen_funcs.add(func_addr)

    output_lines.append("// " + "=" * 72)
    output_lines.append("// Function: {}".format(func.getName()))
    output_lines.append("// Label: {}".format(name))
    output_lines.append("// Ghidra VA: {:#x}  (ELF VA: {:#x})".format(func_addr, elf_va))
    output_lines.append("// Size: {} bytes".format(func.getBody().getNumAddresses()))
    output_lines.append("// " + "=" * 72)
    output_lines.append("")

    result = decomp.decompileFunction(func, 180, monitor)
    if result.decompileCompleted():
        c_code = result.getDecompiledFunction().getC()
        output_lines.append(c_code)
    else:
        output_lines.append("// DECOMPILATION FAILED: {}".format(result.getErrorMessage()))
    output_lines.append("")

decomp.dispose()

with open(OUTPUT_FILE, "w") as f:
    f.write("\n".join(output_lines))

print("Output written to {}".format(OUTPUT_FILE))
