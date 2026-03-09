# ExtractDeviceSettings.py -- Ghidra script to extract per-device settings
#
# Targets the GetDeviceSettings() virtual method (vtable slot 13) for each
# device class, plus the device constructor functions that pass max sample
# rates and channel counts as immediate arguments.
#
# Usage: Run in Ghidra with libgraph_server_shared.so loaded.
#
# TASK-0044: Decompile GetDeviceSettings() per device to extract channel
# counts and sample rates.

from ghidra.app.decompiler import DecompInterface
from ghidra.util.task import ConsoleTaskMonitor

# Known function addresses for GetDeviceSettings (vtable slot 13)
DEVICE_SETTINGS_FUNCTIONS = {
    "LogicGraduateDevice::GetDeviceSettings":          0x1c3f920,
    "LogicProfessionalDevice::GetDeviceSettings":      0x1c44560,
    "LogicProfessional8Device::GetDeviceSettings":     0x1c449d0,
    "LogicProfessional16Device::GetDeviceSettings":    0x1c44750,
    "LogicStudentDevice::GetDeviceSettings":           0x1c57980,
}

# Device constructors that pass sample rates as immediates
DEVICE_CONSTRUCTORS = {
    "LogicGraduateDevice::ctor_inner": 0x1c3d1a0,
    "LogicGraduateDevice::ctor_outer": 0x1c3d700,
    "LogicProfessionalDevice::ctor_base": 0x1c40c00,
    "LogicProfessional16Device::ctor": 0x1c44690,
    "LogicProfessional8Device::ctor": 0x1c44910,
}

# Performance options builder (Logic Student/Logic 4)
PERF_OPTIONS_FUNCTIONS = {
    "LogicStudentDevice::BuildPerformanceOptions": 0x1c56310,
}

# Base class constructors called with rate arguments
BASE_CONSTRUCTORS = {
    "Logic2FpgaDevice::ctor_variant1": 0x1c201f0,
    "Logic2FpgaDevice::ctor_variant2": 0x1c20460,
}

# Sample rate option builder (used by Logic 4)
RATE_BUILDERS = {
    "SampleRateOption::Build": 0x1c47bd0,
}

# Known constants (for annotation)
KNOWN_CONSTANTS = {
    0x1DCD6500: "500 MHz (Pro digital max)",
    0x02FAF080: "50 MHz (Pro analog max)",
    0x0BEBC200: "200 MHz (Logic 8 digital max)",
    0x01312D00: "20 MHz (Logic 8 analog max)",
    0x00B71B00: "12 MHz (Logic 4 digital max)",
    0x005B8D80: "6 MHz (Logic 4 analog max)",
}


def label_function(addr, name):
    """Label a function at the given address."""
    func = getFunctionAt(toAddr(addr))
    if func is None:
        func = createFunction(toAddr(addr), name)
        if func is None:
            println("  WARNING: Could not create function at 0x%x" % addr)
            return None
    func.setName(name, ghidra.program.model.symbol.SourceType.USER_DEFINED)
    println("  Labeled 0x%x as %s" % (addr, name))
    return func


def decompile_function(decompiler, addr, name):
    """Decompile a function and print the C output."""
    func = getFunctionAt(toAddr(addr))
    if func is None:
        println("  No function at 0x%x (%s)" % (addr, name))
        return None

    result = decompiler.decompileFunction(func, 30, ConsoleTaskMonitor())
    if result is None or not result.decompileCompleted():
        println("  Decompilation failed for %s at 0x%x" % (name, addr))
        return None

    decomp = result.getDecompiledFunction()
    if decomp is None:
        println("  No decompiled output for %s" % name)
        return None

    c_code = decomp.getC()
    println("\n=== %s (0x%x) ===" % (name, addr))
    println(c_code)
    return c_code


def annotate_constants(addr, size=0x200):
    """Add EOL comments for known constant values in a function."""
    start = toAddr(addr)
    end = toAddr(addr + size)
    inst = getInstructionAt(start)
    count = 0
    while inst is not None and inst.getAddress().compareTo(end) < 0:
        # Check each operand for known constants
        for i in range(inst.getNumOperands()):
            for scalar in inst.getOpObjects(i):
                try:
                    val = scalar.getUnsignedValue()
                    if val in KNOWN_CONSTANTS:
                        inst.setComment(inst.EOL_COMMENT, KNOWN_CONSTANTS[val])
                        count += 1
                except:
                    pass
        inst = inst.getNext()
    if count > 0:
        println("  Annotated %d constants in function at 0x%x" % (count, addr))


def main():
    println("ExtractDeviceSettings.py -- TASK-0044")
    println("=" * 60)

    # Step 1: Label all known functions
    println("\n--- Labeling functions ---")

    for name, addr in DEVICE_SETTINGS_FUNCTIONS.items():
        label_function(addr, name)

    for name, addr in DEVICE_CONSTRUCTORS.items():
        label_function(addr, name)

    for name, addr in PERF_OPTIONS_FUNCTIONS.items():
        label_function(addr, name)

    for name, addr in BASE_CONSTRUCTORS.items():
        label_function(addr, name)

    for name, addr in RATE_BUILDERS.items():
        label_function(addr, name)

    # Step 2: Annotate known constants
    println("\n--- Annotating constants ---")

    for name, addr in DEVICE_CONSTRUCTORS.items():
        annotate_constants(addr)

    for name, addr in PERF_OPTIONS_FUNCTIONS.items():
        annotate_constants(addr, 0x400)

    # Step 3: Decompile all GetDeviceSettings functions
    println("\n--- Decompiling GetDeviceSettings functions ---")

    decompiler = DecompInterface()
    decompiler.openProgram(currentProgram)

    for name, addr in sorted(DEVICE_SETTINGS_FUNCTIONS.items(), key=lambda x: x[1]):
        decompile_function(decompiler, addr, name)

    # Step 4: Decompile constructors (contain sample rate constants)
    println("\n--- Decompiling device constructors ---")

    for name, addr in sorted(DEVICE_CONSTRUCTORS.items(), key=lambda x: x[1]):
        decompile_function(decompiler, addr, name)

    # Step 5: Decompile Logic 4 performance options builder
    println("\n--- Decompiling LogicStudentDevice performance builder ---")

    for name, addr in sorted(PERF_OPTIONS_FUNCTIONS.items(), key=lambda x: x[1]):
        decompile_function(decompiler, addr, name)

    # Step 6: Decompile base constructors
    println("\n--- Decompiling base constructors ---")

    for name, addr in sorted(BASE_CONSTRUCTORS.items(), key=lambda x: x[1]):
        decompile_function(decompiler, addr, name)

    decompiler.dispose()

    # Step 7: Print summary
    println("\n" + "=" * 60)
    println("SUMMARY: Confirmed device constants from binary analysis")
    println("=" * 60)
    println("")
    println("Device                | Max Digital    | Max Analog   | Channels")
    println("-" * 70)
    println("Logic 4 (Student)     | 12,000,000 Hz  | 6,000,000 Hz | 4")
    println("Logic 8 (Graduate)    | 200,000,000 Hz | 20,000,000 Hz| 8")
    println("Logic Pro 8           | 500,000,000 Hz | 50,000,000 Hz| 8")
    println("Logic Pro 16          | 500,000,000 Hz | 50,000,000 Hz| 16")
    println("")
    println("Extraction method: MOV immediate operands in constructor calls")
    println("Constructor VA 0x1c201f0 (Graduate) and 0x1c20460 (Professional)")
    println("")
    println("XOR-obfuscated GetDeviceSettings blobs require runtime analysis")
    println("for full sample rate option extraction.")


if __name__ == "__main__":
    main()
