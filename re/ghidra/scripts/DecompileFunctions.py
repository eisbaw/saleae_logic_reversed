# Ghidra headless script: decompile specific functions by VA.
# Usage: analyzeHeadless ... -postScript DecompileFunctions.py
#
# Decompiles the key SimpleEncryption/LFSR functions and writes output
# to /tmp/ghidra-decompile-output.txt

from ghidra.app.decompiler import DecompInterface
from ghidra.util.task import ConsoleTaskMonitor

TARGET_VAS = [
    (0x2d99290, "advance_seed"),
    (0x2d987b0, "SetControlByte"),
    (0x2d989e0, "XorEncrypt"),
    (0x2d99310, "SimpleEncryption_Read"),
    (0x2d97ec0, "SimpleEncryption_Write"),
    (0x2d91950, "ReseedDevice"),
    (0x2d99c10, "prepare_buffer"),
    (0x2d91e80, "LinuxUsbDevice_Write"),
]

decomp = DecompInterface()
decomp.openProgram(currentProgram)
monitor = ConsoleTaskMonitor()

output_lines = []

for va, name in TARGET_VAS:
    addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(va)
    func = getFunctionAt(addr)
    if func is None:
        # Try to find function containing this address
        func = getFunctionContaining(addr)
        if func is None:
            output_lines.append("=== {} (VA {:#x}): NO FUNCTION FOUND ===\n".format(name, va))
            continue
        output_lines.append("=== {} (VA {:#x}): found containing function '{}' at {:#x} ===\n".format(
            name, va, func.getName(), func.getEntryPoint().getOffset()))
    else:
        output_lines.append("=== {} (VA {:#x}): function '{}' ===\n".format(
            name, va, func.getName()))

    result = decomp.decompileFunction(func, 120, monitor)
    if result.depiledFunction():
        c_code = result.getDecompiledFunction().getC()
        output_lines.append(c_code)
    else:
        output_lines.append("  DECOMPILATION FAILED: {}\n".format(result.getErrorMessage()))
    output_lines.append("\n")

with open("/tmp/ghidra-decompile-output.txt", "w") as f:
    f.write("\n".join(output_lines))

print("Decompilation output written to /tmp/ghidra-decompile-output.txt")
