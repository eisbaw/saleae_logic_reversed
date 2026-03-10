# Ghidra headless script: decompile device-specific deinterleaving functions (round 5).
# Output: /tmp/ghidra-disaggregator-decompile5.txt

from ghidra.app.decompiler import DecompInterface
from ghidra.app.cmd.function import CreateFunctionCmd
from ghidra.app.cmd.disassemble import DisassembleCommand
from ghidra.util.task import ConsoleTaskMonitor

TARGET_VAS = [
    # FUN_01bf1190 - Pro device deinterleaving (device types 4,5,6)
    # This is the highest priority - Logic Pro 8 is our target device
    (0x1bf1190, "Disaggregator_deinterleave_Pro"),

    # FUN_01bf21b0 - device type 1 deinterleaving
    (0x1bf21b0, "Disaggregator_deinterleave_type1"),

    # FUN_01bf2a90 - device type 3 deinterleaving
    (0x1bf2a90, "Disaggregator_deinterleave_type3"),

    # FUN_01bf37e0 - device type 2 deinterleaving
    (0x1bf37e0, "Disaggregator_deinterleave_type2"),

    # FUN_01bef940 - digital channel data extraction (called per-channel)
    (0x1bef940, "Disaggregator_extract_digital"),

    # FUN_02eae7f0 - analog channel data extraction (called per-channel)
    (0x2eae7f0, "Disaggregator_extract_analog"),

    # Also get the settings handler that sets the has_settings flag (offset 0x130)
    # The DisaggregatorSettingsData pipe handler
    # Look for function that writes to this+0x130 = 1
    # From the data handler, settings must arrive first
    # The init/constructor should set up pipe handlers
    # Let's get the constructor/init more carefully
    # FUN_01be7c20 was a destructor. Let's try nearby addresses.
    # Actually, let's get the function at the vtable for settings handling
]

monitor = ConsoleTaskMonitor()
decomp = DecompInterface()
decomp.openProgram(currentProgram)

af = currentProgram.getAddressFactory()
space = af.getDefaultAddressSpace()

output_lines = []
output_lines.append("=" * 78)
output_lines.append("DISAGGREGATOR DECOMPILATION OUTPUT - ROUND 5")
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

outpath = "/tmp/ghidra-disaggregator-decompile5.txt"
with open(outpath, "w") as f:
    f.write("\n".join(output_lines))

print("Output written to " + outpath)
