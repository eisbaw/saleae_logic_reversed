# Ghidra headless script: decompile disaggregator and data interleaving functions.
# Usage: ghidra-analyzeHeadless ghidra_proj2 sal_rev2 \
#   -process libgraph_server_shared.so -noanalysis \
#   -postScript DecompileDisaggregator.py
#
# Output: /tmp/ghidra-disaggregator-decompile.txt

from ghidra.app.decompiler import DecompInterface
from ghidra.app.cmd.function import CreateFunctionCmd
from ghidra.app.cmd.disassemble import DisassembleCommand
from ghidra.util.task import ConsoleTaskMonitor

# Ghidra VAs = ELF VA + 0x100000
# These are Ghidra-rebased addresses
TARGET_VAS = [
    # ContractReadData - removes padding bytes from FPGA data
    # ELF VA 0x1c3bad0, uses offset 0x19c (gpif width) and 0x1a0 (buffer size)
    (0x1d3bad0, "ContractReadData"),

    # ExpandWriteData - adds padding bytes for FPGA writes
    # ELF VA 0x1c3bef0
    (0x1d3bef0, "ExpandWriteData"),

    # GetDisaggregatorSettings - constructs DisaggregatorSettingsData struct
    # ELF VA 0x1be6e80
    (0x1ce6e80, "GetDisaggregatorSettings"),

    # DisaggregatorNode constructor or initializer (after int3 padding at 0x1ae7c1f)
    # ELF VA 0x1ae7c20 - sets up vtable
    (0x1be7c20, "DisaggregatorNode_ctor_or_init"),

    # OnDataBuffer - the callback that receives raw USB data
    # Need to find this from the lambda reference
    # The lambda is at the mangled name containing "OnDataBuffer"
    # Let's try a few candidates around the data handler

    # ModifyFakeData - reveals interleaving format
    # ELF VA 0x1c3b840
    (0x1d3b840, "ModifyFakeData_helper"),

    # The main ContractReadData caller function
    # ELF VA 0x1c3bca0 - takes rdi=this, rsi=buffer, rdx=size, rcx=flag
    (0x1d3bca0, "ContractReadData_caller"),

    # FakeReadThread - Logic2FpgaDevice simulation
    # The assertion ref is at ELF 0x1c28614, need to find function start
    # Let me try searching the area. ELF 0x1c27b40 to 0x1c28620
    # We'll let Ghidra find the function containing 0x1c28614
    (0x1d28614, "FakeReadThread_body"),

    # LogicStudentDevice::ModifyFakeData
    # ELF VA near 0x1c580ac (ref) - function likely starts earlier
    (0x1d580ac, "StudentModifyFakeData_body"),

    # FUN_01d19fa0 - packet configuration validator
    # Called during AnalogAdvanced with interleave slot count
    # Ghidra VA = 0x1d19fa0 (ELF 0x1c19fa0)
    (0x1d19fa0, "ValidatePacketConfig"),
]

monitor = ConsoleTaskMonitor()
decomp = DecompInterface()
decomp.openProgram(currentProgram)

af = currentProgram.getAddressFactory()
space = af.getDefaultAddressSpace()

output_lines = []
output_lines.append("=" * 78)
output_lines.append("DISAGGREGATOR DECOMPILATION OUTPUT")
output_lines.append("Binary: libgraph_server_shared.so")
output_lines.append("=" * 78)
output_lines.append("")

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

    result = decomp.decompileFunction(func, 180, monitor)
    if result.decompileCompleted():
        c_code = result.getDecompiledFunction().getC()
        output_lines.append(c_code)
    else:
        output_lines.append("  DECOMPILATION FAILED: {}".format(result.getErrorMessage()))
    output_lines.append("")
    output_lines.append("")

decomp.dispose()

outpath = "/tmp/ghidra-disaggregator-decompile.txt"
with open(outpath, "w") as f:
    f.write("\n".join(output_lines))

print("Output written to " + outpath)
