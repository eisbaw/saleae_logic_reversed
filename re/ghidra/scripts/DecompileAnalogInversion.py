# Ghidra headless script: decompile analog inversion condition and Logic 8 transform.
# Output: /tmp/ghidra-analog-inversion.txt

from ghidra.app.decompiler import DecompInterface
from ghidra.app.cmd.function import CreateFunctionCmd
from ghidra.app.cmd.disassemble import DisassembleCommand
from ghidra.util.task import ConsoleTaskMonitor

TARGET_VAS = [
    # FUN_02eadc90 - condition check called by FUN_02eae7f0
    # Determines whether bit inversion is needed
    (0x2eadc90, "AnalogExtract_check_inversion"),

    # FUN_01bf1ff0 - Logic 8 transform called by FUN_01bf21b0
    # Transforms 8-byte word during Logic 8 deinterleaving
    (0x1bf1ff0, "Logic8_transform"),

    # FUN_01bf21b0 - Logic 8 deinterleaver (for context on FUN_01bf1ff0 usage)
    (0x1bf21b0, "Disaggregator_deinterleave_Logic8"),

    # Per-channel state initialization in DisaggregatorNode
    # The channel_type flag at this+0x1e0+ch*0x38 discriminates digital vs analog
    # Let's find where this is set up. The FillDisaggregatorSettings function:
    (0x1ceb730, "FillDisaggregatorSettings"),

    # The settings handler that receives DisaggregatorSettingsData
    # Let's get FUN_01be9570 which is the outer loop setting up per-channel state
    (0x1be9570, "DisaggregatorNode_setup_channels"),
]

monitor = ConsoleTaskMonitor()
decomp = DecompInterface()
decomp.openProgram(currentProgram)

af = currentProgram.getAddressFactory()
space = af.getDefaultAddressSpace()

output_lines = []
output_lines.append("=" * 78)
output_lines.append("ANALOG INVERSION / LOGIC 8 TRANSFORM DECOMPILATION")
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

    result = decomp.decompileFunction(func, 300, monitor)
    if result.decompileCompleted():
        c_code = result.getDecompiledFunction().getC()
        output_lines.append(c_code)
    else:
        output_lines.append("  DECOMPILATION FAILED: {}".format(result.getErrorMessage()))
    output_lines.append("")
    output_lines.append("")

decomp.dispose()

outpath = "/tmp/ghidra-analog-inversion.txt"
with open(outpath, "w") as f:
    f.write("\n".join(output_lines))

print("Output written to " + outpath)
