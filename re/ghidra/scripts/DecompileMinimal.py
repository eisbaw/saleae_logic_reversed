# Ghidra headless script: decompile specific functions by VA.
# Designed to run with -noanalysis + minimal manual analysis
# to avoid the 30+ minute full analysis of a 48MB binary.

from ghidra.app.decompiler import DecompInterface
from ghidra.util.task import ConsoleTaskMonitor
from ghidra.program.model.listing import CodeUnit

TARGET_VAS = [
    (0x2d99290, "advance_seed"),
    (0x2d987b0, "SetControlByte"),
    (0x2d989e0, "XorEncrypt"),
    (0x2d99310, "SimpleEncryption_Read"),
    (0x2d97ec0, "SimpleEncryption_Write"),
    (0x2d91950, "ReseedDevice"),
    (0x2d99c10, "prepare_buffer"),
]

monitor = ConsoleTaskMonitor()
listing = currentProgram.getListing()
af = currentProgram.getAddressFactory()
space = af.getDefaultAddressSpace()

# First, create functions at the target addresses
from ghidra.app.cmd.function import CreateFunctionCmd

for va, name in TARGET_VAS:
    addr = space.getAddress(va)
    func = getFunctionAt(addr)
    if func is None:
        # Try to disassemble first
        from ghidra.app.cmd.disassemble import DisassembleCommand
        disCmd = DisassembleCommand(addr, None, True)
        disCmd.applyTo(currentProgram, monitor)
        # Then create function
        cmd = CreateFunctionCmd(addr)
        cmd.applyTo(currentProgram, monitor)
        func = getFunctionAt(addr)
        if func is not None:
            print("Created function '{}' at {:#x}".format(func.getName(), va))
        else:
            print("FAILED to create function at {:#x} ({})".format(va, name))

# Now decompile
decomp = DecompInterface()
decomp.openProgram(currentProgram)

output_lines = []

for va, name in TARGET_VAS:
    addr = space.getAddress(va)
    func = getFunctionAt(addr)
    if func is None:
        func = getFunctionContaining(addr)
    if func is None:
        output_lines.append("=== {} (VA {:#x}): NO FUNCTION ===\n".format(name, va))
        continue

    output_lines.append("=== {} (VA {:#x}): '{}' size={} ===\n".format(
        name, va, func.getName(), func.getBody().getNumAddresses()))

    result = decomp.decompileFunction(func, 120, monitor)
    if result.decompileCompleted():
        c_code = result.getDecompiledFunction().getC()
        output_lines.append(c_code)
    else:
        output_lines.append("  DECOMPILATION FAILED: {}\n".format(result.getErrorMessage()))
    output_lines.append("\n")

decomp.dispose()

with open("/tmp/ghidra-decompile-output.txt", "w") as f:
    f.write("\n".join(output_lines))

print("Output written to /tmp/ghidra-decompile-output.txt")
