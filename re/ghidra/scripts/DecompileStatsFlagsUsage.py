# Ghidra headless script: trace stats header flags usage.
# Decompiles the stats parser and all its callers to understand
# what each flag bit does.
#
# Target: FUN_02e41510 (ParseMsoWaveformData) which parses the
# flags u16 into individual boolean fields.
#
# Also decompiles:
# - All xref callers of FUN_02e41510
# - FUN_02e42350 (the u16 reader called for flags)
# - Any functions that reference the "software" keyword near waveform handling
#
# Usage:
#   ghidra-analyzeHeadless ghidra_proj2 sal_rev2 -process -noanalysis \
#     -scriptPath re/ghidra/scripts -postScript DecompileStatsFlagsUsage.py

from ghidra.app.decompiler import DecompInterface
from ghidra.util.task import ConsoleTaskMonitor
from ghidra.app.cmd.function import CreateFunctionCmd
from ghidra.app.cmd.disassemble import DisassembleCommand

OUTPUT_FILE = "re/ghidra/decompiled-stats-flags.txt"

monitor = ConsoleTaskMonitor()
space = currentProgram.getAddressFactory().getDefaultAddressSpace()
refMgr = currentProgram.getReferenceManager()

# Targets to decompile: (Ghidra VA, label)
TARGETS = [
    (0x2e41510, "ParseMsoWaveformData"),
    (0x2e42350, "ReadU16_flags"),
]

# Find all callers of ParseMsoWaveformData
parser_addr = space.getAddress(0x2e41510)
parser_func = getFunctionAt(parser_addr)
if parser_func is None:
    parser_func = getFunctionContaining(parser_addr)

caller_addrs = []
if parser_func is not None:
    entry = parser_func.getEntryPoint()
    refs = getReferencesTo(entry)
    for ref in refs:
        from_addr = ref.getFromAddress()
        caller = getFunctionContaining(from_addr)
        if caller is not None:
            caller_va = caller.getEntryPoint().getOffset()
            caller_name = caller.getName()
            if caller_va not in [t[0] for t in TARGETS]:
                TARGETS.append((caller_va, "caller_of_parser__" + caller_name))
                caller_addrs.append(caller_va)
                print("Found caller: {} at {:#x}".format(caller_name, caller_va))

# Also look for the "PrintErrors" function that uses overflow flags
TARGETS.append((0x1d1fcb0, "PrintErrors"))
# The HandleWaveformData logging path
TARGETS.append((0x2e4cef0, "LogFunction_used_by_HandleWaveformData"))

# Ensure functions exist
for va, name in TARGETS:
    addr = space.getAddress(va)
    func = getFunctionAt(addr)
    if func is None:
        func = getFunctionContaining(addr)
    if func is None:
        disCmd = DisassembleCommand(addr, None, True)
        disCmd.applyTo(currentProgram, monitor)
        cmd = CreateFunctionCmd(addr)
        cmd.applyTo(currentProgram, monitor)
        func = getFunctionAt(addr)
        if func is not None:
            print("Created function at {:#x}".format(va))
        else:
            print("Could not create function at {:#x} ({})".format(va, name))

# Decompile
decomp = DecompInterface()
decomp.openProgram(currentProgram)

output_lines = []
output_lines.append("// Decompiled stats flags usage analysis")
output_lines.append("// Ghidra addresses = ELF VA + 0x100000")
output_lines.append("// Goal: identify semantics of flag bits 8, 11, 13, 14, 15")
output_lines.append("//")
output_lines.append("// Flag bit extraction from FUN_02e41510:")
output_lines.append("//   uVar6 = ReadU16()  // flags word")
output_lines.append("//   bVar4 = (byte)(uVar6 >> 8)  // high byte")
output_lines.append("//   param_1[0] = bVar4 >> 6 & 1  // bit 14")
output_lines.append("//   param_1[1] = bVar4 >> 7       // bit 15")
output_lines.append("//   param_1[2] = bVar4 >> 5 & 1  // bit 13")
output_lines.append("//   param_1[3] = bVar4 >> 4 & 1  // bit 12 (overflow_data)")
output_lines.append("//   param_1[4] = bVar4 >> 3 & 1  // bit 11")
output_lines.append("//   param_1[5] = bVar4 >> 2 & 1  // bit 10 (debug_log?)")
output_lines.append("//   param_1[6] = bVar4 >> 1 & 1  // bit 9  (overflow_meta)")
output_lines.append("//   param_1[7] = bVar4 & 1       // bit 8")
output_lines.append("//   param_1[8] = (byte)uVar6     // low byte (channel/status)")
output_lines.append("")

seen_funcs = set()

for va, name in TARGETS:
    addr = space.getAddress(va)
    func = getFunctionAt(addr)
    if func is None:
        func = getFunctionContaining(addr)
    if func is None:
        output_lines.append("// === {} (Ghidra VA {:#x}): NO FUNCTION ===".format(name, va))
        output_lines.append("")
        continue

    func_addr = func.getEntryPoint().getOffset()
    elf_va = func_addr - 0x100000

    if func_addr in seen_funcs:
        output_lines.append("// === {} (Ghidra VA {:#x}): same function as entry {:#x}, already decompiled ===".format(
            name, va, func_addr))
        output_lines.append("")
        continue
    seen_funcs.add(func_addr)

    func_size = func.getBody().getNumAddresses()
    output_lines.append("// " + "=" * 72)
    output_lines.append("// Function: {}".format(func.getName()))
    output_lines.append("// Label: {}".format(name))
    output_lines.append("// Ghidra VA: {:#x}  (ELF VA: {:#x})".format(func_addr, elf_va))
    output_lines.append("// Size: {} bytes".format(func_size))

    # List xrefs to this function
    entry = func.getEntryPoint()
    refs = getReferencesTo(entry)
    xref_count = 0
    xref_lines = []
    for ref in refs:
        from_addr = ref.getFromAddress()
        from_func = getFunctionContaining(from_addr)
        if from_func:
            xref_lines.append("//   called from {} at {:#x}".format(
                from_func.getName(), from_addr.getOffset()))
        else:
            xref_lines.append("//   called from unknown at {:#x}".format(from_addr.getOffset()))
        xref_count += 1
    output_lines.append("// Xrefs: {} callers".format(xref_count))
    for xl in xref_lines[:20]:  # limit to 20
        output_lines.append(xl)
    if xref_count > 20:
        output_lines.append("//   ... and {} more".format(xref_count - 20))
    output_lines.append("// " + "=" * 72)
    output_lines.append("")

    # Skip decompilation for very large functions (>10K bytes) unless they are key targets
    if func_size > 15000 and va not in [0x2e41510, 0x2e44b80, 0x2e42350, 0x1d1fcb0]:
        output_lines.append("// [SKIPPED: function too large ({} bytes)]".format(func_size))
        output_lines.append("")
        continue

    result = decomp.decompileFunction(func, 300, monitor)
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
print("Decompiled {} unique functions".format(len(seen_funcs)))
