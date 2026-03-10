# Ghidra headless script: decompile the disaggregator data handler (round 3).
# Output: /tmp/ghidra-disaggregator-decompile3.txt

from ghidra.app.decompiler import DecompInterface
from ghidra.app.cmd.function import CreateFunctionCmd
from ghidra.app.cmd.disassemble import DisassembleCommand
from ghidra.util.task import ConsoleTaskMonitor

TARGET_VAS = [
    # The DisaggregatorNode OwnedSpan<uint8_t> pipe handler
    # The function that processes incoming raw USB data
    # ELF VA 0x1ae9a70, Ghidra VA 0x1be9a70
    (0x1be9a70, "DisaggregatorNode_data_handler"),

    # The larger function containing the disaggregator loop
    # FUN_01be9570 was already decompiled but is the CALLER of the data handler
    # Let me also get the function that dispatches to per-channel processing
    # The loop body function: FUN_01bee130 (called from the loop)
    (0x1bee130, "Disaggregator_emit_digital"),

    # FUN_01bee240 (also called from the loop - analog path)
    (0x1bee240, "Disaggregator_emit_analog"),

    # The constructor at LogicDeviceNode+0x3b0 that builds the channel layout
    # GetDisaggregatorSettings gets data from param_2 + 0x3b0
    # param_2 is the LogicDeviceNode object
    # Let me find who writes to those offsets

    # FUN_01beda50 - called at end of disaggregator loop (sets up stats)
    (0x1beda50, "Disaggregator_stats_setup"),

    # FUN_02d91580 - called after the main loop
    (0x2d91580, "Disaggregator_finalize"),

    # FUN_01bebf90 - cleanup
    (0x1bebf90, "Disaggregator_cleanup"),

    # The actual FakeReadThread function
    # The assertion string "void Logic2FpgaDevice::FakeReadThread(U32, U32)" at 0x420801
    # suggests the function exists but we found only the assertion stub.
    # Let me try to find the real function by looking for code that references
    # the FakeReadThread string but also has actual data generation logic.
    # Try a larger decompilation around the Logic2FpgaDevice area.

    # Let me also get the OnWrite function (0x1d3bca0) - already have it as ContractReadData_caller
    # And the GPIF width combo check function
    # The "No valid analog and gpif width combo" at 0x34ed19 is referenced from ELF 0x1c1a0dd
    # What function is that in?
    (0x1d1a0dd, "NoValidGpifCombo_context"),
]

monitor = ConsoleTaskMonitor()
decomp = DecompInterface()
decomp.openProgram(currentProgram)

af = currentProgram.getAddressFactory()
space = af.getDefaultAddressSpace()

output_lines = []
output_lines.append("=" * 78)
output_lines.append("DISAGGREGATOR DECOMPILATION OUTPUT - ROUND 3")
output_lines.append("=" * 78)
output_lines.append("")

for va, name in TARGET_VAS:
    addr = space.getAddress(va)
    func = getFunctionAt(addr)
    if func is None:
        disCmd = DisassembleCommand(addr, None, True)
        disCmd.applyTo(currentProgram, monitor)
        cmd = CreateFunctionCmd(addr)
        cmd.applyTo(currentProgram, monitor)
        func = getFunctionAt(addr)
    if func is None:
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

    result = decomp.decompileFunction(func, 180, monitor)
    if result.decompileCompleted():
        c_code = result.getDecompiledFunction().getC()
        output_lines.append(c_code)
    else:
        output_lines.append("  DECOMPILATION FAILED: {}".format(result.getErrorMessage()))
    output_lines.append("")
    output_lines.append("")

decomp.dispose()

outpath = "/tmp/ghidra-disaggregator-decompile3.txt"
with open(outpath, "w") as f:
    f.write("\n".join(output_lines))

print("Output written to " + outpath)
