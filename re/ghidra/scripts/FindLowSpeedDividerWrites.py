# Ghidra headless script: find all functions that write to field offset 0x19c
# (LowSpeedDivider in the LogicDevice object).
#
# Strategy: search for MOV instructions that write to [reg+0x19c] patterns.
# Also decompile the thin ContractReadData caller (FUN_01d3b4c0, 81 bytes).
#
# Usage: ghidra-analyzeHeadless ghidra_proj2 sal_rev2 \
#   -process libgraph_server_shared.so -noanalysis \
#   -scriptPath re/ghidra/scripts -postScript FindLowSpeedDividerWrites.py
#
# Output: re/ghidra/decompiled-lowspeed-divider.txt

from ghidra.app.decompiler import DecompInterface
from ghidra.app.cmd.function import CreateFunctionCmd
from ghidra.app.cmd.disassemble import DisassembleCommand
from ghidra.util.task import ConsoleTaskMonitor

monitor = ConsoleTaskMonitor()
decomp = DecompInterface()
decomp.openProgram(currentProgram)

af = currentProgram.getAddressFactory()
space = af.getDefaultAddressSpace()
listing = currentProgram.getListing()

output_lines = []
output_lines.append("=" * 78)
output_lines.append("LOWSPEED DIVIDER WRITE ANALYSIS")
output_lines.append("=" * 78)
output_lines.append("")

# -----------------------------------------------------------------------
# Part 1: Decompile the thin ContractReadData wrapper (FUN_01d3b4c0)
# -----------------------------------------------------------------------

output_lines.append("=== ContractReadData thin wrapper (FUN_01d3b4c0, 81 bytes) ===")
output_lines.append("")

addr = space.getAddress(0x1d3b4c0)
func = getFunctionAt(addr)
if func is None:
    disCmd = DisassembleCommand(addr, None, True)
    disCmd.applyTo(currentProgram, monitor)
    cmd = CreateFunctionCmd(addr)
    cmd.applyTo(currentProgram, monitor)
    func = getFunctionAt(addr)

if func:
    result = decomp.decompileFunction(func, 120, monitor)
    if result.decompileCompleted():
        output_lines.append(result.getDecompiledFunction().getC())
    else:
        output_lines.append("DECOMPILATION FAILED: {}".format(result.getErrorMessage()))
else:
    output_lines.append("NO FUNCTION at 0x1d3b4c0")

output_lines.append("")

# -----------------------------------------------------------------------
# Part 2: Search for instructions that reference offset 0x19c (410 decimal)
# Look for MOV [xxx+0x19c], yyy in .text section
# The key byte pattern is 9C 01 00 00 for 0x19c displacement
# -----------------------------------------------------------------------

output_lines.append("=" * 78)
output_lines.append("SEARCHING FOR INSTRUCTIONS REFERENCING OFFSET 0x19c")
output_lines.append("=" * 78)
output_lines.append("")

# Search in the .text section for the byte pattern 0x9c, 0x01, 0x00, 0x00
# This will match both reads and writes to offset 0x19c
# The text section starts around 0x100000 (after Ghidra rebase)

from ghidra.program.model.mem import MemoryAccessException

text_block = None
for block in currentProgram.getMemory().getBlocks():
    if block.getName() == ".text":
        text_block = block
        break

if text_block is None:
    output_lines.append("WARNING: .text block not found")
else:
    start = text_block.getStart()
    end = text_block.getEnd()
    output_lines.append("Scanning .text: {} to {} ({} bytes)".format(
        start, end, text_block.getSize()))
    output_lines.append("")

    # Use Ghidra's search for the 4-byte pattern 9c 01 00 00
    # which is the little-endian encoding of 0x19c
    from jarray import array as jarray
    search_bytes = jarray([0x9c - 256, 0x01, 0x00, 0x00], 'b')  # Java signed bytes
    mask = None

    addr = start
    found_count = 0
    funcs_with_19c = set()

    while addr is not None and addr.compareTo(end) < 0:
        # Search for next occurrence
        addr = currentProgram.getMemory().findBytes(addr, end,
            search_bytes, mask, True, monitor)
        if addr is None:
            break

        # Check if this is within an instruction
        instr = listing.getInstructionContaining(addr)
        if instr is not None:
            mnemonic = instr.getMnemonicString()
            instr_str = str(instr)

            # Only interested in MOV-like instructions that write to [reg+0x19c]
            # or that reference 0x19c as a displacement
            if "0x19c" in instr_str or "+0x19c" in instr_str:
                func = getFunctionContaining(addr)
                func_name = func.getName() if func else "???"
                func_addr = func.getEntryPoint() if func else "???"

                output_lines.append("  {} @ {} in {} ({})".format(
                    instr_str, instr.getAddress(), func_name, func_addr))

                if func and "MOV" in mnemonic.upper():
                    funcs_with_19c.add((func.getEntryPoint().getOffset(), func.getName(),
                                       func.getBody().getNumAddresses()))
                found_count += 1

        addr = addr.add(1)

    output_lines.append("")
    output_lines.append("Total instructions referencing 0x19c: {}".format(found_count))
    output_lines.append("")

    # -----------------------------------------------------------------------
    # Part 3: Decompile functions that MOV to 0x19c (write to the field)
    # -----------------------------------------------------------------------

    if funcs_with_19c:
        output_lines.append("=" * 78)
        output_lines.append("FUNCTIONS WITH MOV TO 0x19c")
        output_lines.append("=" * 78)
        output_lines.append("")

        for func_va, func_name, func_size in sorted(funcs_with_19c):
            output_lines.append("--- {} at {:#x} (size={}) ---".format(func_name, func_va, func_size))
            addr = space.getAddress(func_va)
            func = getFunctionAt(addr)
            if func:
                result = decomp.decompileFunction(func, 300, monitor)
                if result.decompileCompleted():
                    c_code = result.getDecompiledFunction().getC()
                    # Only include first 200 lines to keep output manageable
                    lines = c_code.split('\n')
                    if len(lines) > 200:
                        output_lines.extend(lines[:200])
                        output_lines.append("... [truncated at 200 lines, {} total] ...".format(len(lines)))
                    else:
                        output_lines.append(c_code)
                else:
                    output_lines.append("DECOMPILATION FAILED")
            output_lines.append("")

decomp.dispose()

outpath = "/home/mpedersen/nvidia/saleae_reverse/re/ghidra/decompiled-lowspeed-divider.txt"
with open(outpath, "w") as f:
    f.write("\n".join(output_lines))

print("Output written to " + outpath)
