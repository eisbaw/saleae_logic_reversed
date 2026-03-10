# Ghidra headless script: decompile additional disaggregator functions (round 2).
# Output: /tmp/ghidra-disaggregator-decompile2.txt

from ghidra.app.decompiler import DecompInterface
from ghidra.app.cmd.function import CreateFunctionCmd
from ghidra.app.cmd.disassemble import DisassembleCommand
from ghidra.util.task import ConsoleTaskMonitor

TARGET_VAS = [
    # FUN_01ceb730 - fills the DisaggregatorSettingsData struct (0xa0 bytes)
    # Called from GetDisaggregatorSettings with param_2+0x3b0
    (0x1ceb730, "FillDisaggregatorSettings"),

    # Need to find the actual DisaggregatorNode data processing handler
    # The "requires DisaggregatorSettingsData" string ref was at ELF 0x1ae9b3d (Ghidra 0x1be9b3d)
    # Let me try the function containing that address
    (0x1be9b3d, "DisaggregatorNode_OwnedSpan_handler_body"),

    # The DisaggregatorNode's main processing function
    # Let me also try the pipe handler registration function area
    # The RegisterPipeHandler for OwnedSpan<uint8_t> on DisaggregatorNode
    # will contain or point to the lambda that processes raw bytes

    # FUN_01d47140 - GCD function called from DeviceCalculateMixRate
    (0x1d47140, "GCD_or_ratio_helper"),

    # The main FakeReadThread function - need to find the real entry
    # The assertion ref was just an error path. Let me search for the function
    # that the assertion string names: "void Logic2FpgaDevice::FakeReadThread(U32, U32)"
    # The string at 0x420801 has this name. The function is likely at a different address.
    # Let me look for the actual function by scanning for callers of the assertion stub.
    # The assertion stub is at Ghidra 0x1d28614. Who calls it?

    # The OnDataBuffer function - LogicDeviceNode::OnDataBuffer
    # lambda $_10 is referenced in the mangled name
    # Let me try to find by searching near the "OnDataBuffer" string at file offset 0x3fc491
    # That's a rodata string. Let me try to find references.

    # ModifyFakeData - the real function (not the helper)
    # ELF VA was 0x1c3c712 ref -> function at ~0x1c3c6f0
    (0x1d3c6f0, "ModifyFakeData_real"),

    # Also try the function that actually does the disaggregation loop
    # Looking at the DisaggregatorNode, its data handler processes OwnedSpan<uint8_t>
    # and must contain the main loop that splits channels.
    # The function at 0x1be7c20 was a destructor. Let me search more broadly.

    # The ContractReadData_caller function (0x1d3bca0) is actually OnWrite/OnDataBuffer
    # for LogicDevice. It calls:
    #   - The write callback at *(param_1 + 0x1d0)
    #   - ContractReadData/ExpandWriteData inline
    # So this IS the data path. But the disaggregator is further downstream.

    # Let me get the function referenced by the DisaggregatorNode pipe handler
    # The RTTI says the pipe handler for OwnedSpan is registered at addresses
    # containing ZN6Saleae5Graph17ConvenienceKernel19RegisterPipeHandlerINS0_9OwnedSpanIhEENS0_17DisaggregatorNodeEE
    # Let me look for the actual handler lambda

    # The "before any other data" error is at Ghidra 0x1be9b3d
    # Let me trace backward to find the handler function entry
    (0x1be9680, "DisaggregatorNode_handler_candidate_1"),
    (0x1be9570, "DisaggregatorNode_handler_candidate_2"),
]

monitor = ConsoleTaskMonitor()
decomp = DecompInterface()
decomp.openProgram(currentProgram)

af = currentProgram.getAddressFactory()
space = af.getDefaultAddressSpace()

output_lines = []
output_lines.append("=" * 78)
output_lines.append("DISAGGREGATOR DECOMPILATION OUTPUT - ROUND 2")
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

outpath = "/tmp/ghidra-disaggregator-decompile2.txt"
with open(outpath, "w") as f:
    f.write("\n".join(output_lines))

print("Output written to " + outpath)
