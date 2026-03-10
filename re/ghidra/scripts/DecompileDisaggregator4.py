# Ghidra headless script: decompile the core disaggregation function (round 4).
# Output: /tmp/ghidra-disaggregator-decompile4.txt

from ghidra.app.decompiler import DecompInterface
from ghidra.app.cmd.function import CreateFunctionCmd
from ghidra.app.cmd.disassemble import DisassembleCommand
from ghidra.util.task import ConsoleTaskMonitor

TARGET_VAS = [
    # FUN_01bea5d0 - the core disaggregation processing function
    # Called from the data handler with (param_1=DisaggregatorNode_this, param_2=data_chunk)
    (0x1bea5d0, "DisaggregatorNode_ProcessChunk"),

    # FUN_02db8a00 - appends to internal queue
    (0x2db8a00, "Queue_append"),

    # FUN_02db8b40 - dequeue item
    (0x2db8b40, "Queue_dequeue"),

    # FUN_02db8770 - get from queue
    (0x2db8770, "Queue_get"),

    # FUN_02db92d0 - check if queue has items
    (0x2db92d0, "Queue_has_items"),

    # The settings handler for DisaggregatorSettingsData
    # When settings arrive (param_1+0x130 should become 1)
    # Before data, settings must arrive. Let's find where param_1+0x130 is set.
    # The check is: if (*(int *)(param_1 + 0x130) == 1) { process data }
    # else: throw "requires settings"
    # So there must be a handler that sets param_1+0x130 = 1 when settings arrive
    # This would be the DisaggregatorSettingsData pipe handler

    # Also get FUN_01bede30 - called from stats_setup for each unmatched channel
    (0x1bede30, "Disaggregator_add_new_channel"),
]

monitor = ConsoleTaskMonitor()
decomp = DecompInterface()
decomp.openProgram(currentProgram)

af = currentProgram.getAddressFactory()
space = af.getDefaultAddressSpace()

output_lines = []
output_lines.append("=" * 78)
output_lines.append("DISAGGREGATOR DECOMPILATION OUTPUT - ROUND 4")
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

outpath = "/tmp/ghidra-disaggregator-decompile4.txt"
with open(outpath, "w") as f:
    f.write("\n".join(output_lines))

print("Output written to " + outpath)
