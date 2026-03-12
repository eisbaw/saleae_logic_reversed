// Decompile specific calibration filter functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileCalibFilter extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Ghidra addresses (ELF + 0x100000)
        String[][] targets = new String[][] {
            // AnalogCalibrationFilter constructor
            {"01bd2020", "AnalogCalibrationFilter_ctor"},
            // SamplingConfigurationData pipe handler (the big one - 881 bytes)
            {"01bd2aa0", "AnalogCalibrationFilter_OnSamplingConfig"},
            // VectorData<short> pipe handler (processes actual samples)
            // Need to find this -- look at vtable PTR_FUN_02f38f80
            // SetCalibration request handler
            {"01bd60d0", "SetCalibration_ctor"},
            // The filter setup functions called from the pipe handler
            {"02db6ef0", "FilterSetup_from_coefficients"},
            {"01bd8240", "CreateSimdFilter"},
            // Calibration data deserializer
            {"02dd0200", "CalibrationData_deserialize"},
            // SimdIirFilterPrototype / SimdPrototype_Static2By3IirFilter
            // Let's find the vtable at PTR_FUN_02f38f80 first
            // The main process function - let's find it from vtable
            {"02eb1c40", "ComputeOutputSampleRate"},
            {"02db06c0", "CreateOutputBuffer"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Create functions where missing
        int tx = currentProgram.startTransaction("Create calibration filter functions");
        try {
            for (String[] t : targets) {
                Address addr = af.getAddress(t[0]);
                Function func = fm.getFunctionAt(addr);
                if (func == null) {
                    Function containing = fm.getFunctionContaining(addr);
                    if (containing != null) {
                        println("NOTE: " + t[0] + " (" + t[1] + ") inside " + containing.getName() + " @ " + containing.getEntryPoint());
                    } else {
                        CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                        cmd.applyTo(currentProgram);
                        func = fm.getFunctionAt(addr);
                        if (func != null) {
                            func.setName(t[1], SourceType.USER_DEFINED);
                            println("CREATED: " + t[1] + " @ " + t[0]);
                        } else {
                            println("FAILED to create function at: " + t[0] + " (" + t[1] + ")");
                        }
                    }
                } else {
                    println("EXISTS: " + func.getName() + " @ " + t[0]);
                }
            }
        } finally {
            currentProgram.endTransaction(tx, true);
        }

        // Also look at the vtable to find the virtual methods
        println("\n=== VTABLE at PTR_FUN_02f38f80 ===");
        Address vtableAddr = af.getAddress("02f38f80");
        for (int i = 0; i < 20; i++) {
            Address slot = vtableAddr.add(i * 8);
            long val = currentProgram.getMemory().getLong(slot);
            Address targetAddr = af.getDefaultAddressSpace().getAddress(val);
            Function func = fm.getFunctionAt(targetAddr);
            String name = (func != null) ? func.getName() : "???";
            println("  vtable[" + i + "] = " + targetAddr + " (" + name + ")");
        }

        // Also look at the vtable for PTR_FUN_02f38648 (the output buffer?)
        println("\n=== VTABLE at PTR_FUN_02f38648 ===");
        vtableAddr = af.getAddress("02f38648");
        for (int i = 0; i < 10; i++) {
            Address slot = vtableAddr.add(i * 8);
            long val = currentProgram.getMemory().getLong(slot);
            Address targetAddr = af.getDefaultAddressSpace().getAddress(val);
            Function func = fm.getFunctionAt(targetAddr);
            String name = (func != null) ? func.getName() : "???";
            println("  vtable[" + i + "] = " + targetAddr + " (" + name + ")");
        }

        // Decompile
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            Function func = fm.getFunctionAt(addr);
            if (func == null) func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("=== NO FUNCTION: " + t[0] + " (" + t[1] + ") ===\n");
                continue;
            }
            println("\n=== " + func.getName() + " @ " + func.getEntryPoint() + " (" + t[1] + ") ===");
            println("Size: " + func.getBody().getNumAddresses() + " bytes");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
        }

        decomp.dispose();
    }
}
