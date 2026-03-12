// Decompile SIMD IIR filter constructors and process functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileCalibFilter2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[][] targets = new String[][] {
            // SimdIirFilterPrototype constructor (size 0x380)
            {"01bd7700", "SimdIirFilterPrototype_ctor"},
            // SimdPrototype_Static2By3IirFilter constructor (size 0x6e0)
            {"01bd9d30", "Static2By3IirFilter_ctor"},
            // Filter type determination
            {"01bd8970", "DetermineFilterType"},
            // The AnalogCalibrationFilter VectorData<short> handler
            // vtable[3] = 01bd21d0 -- this is likely the processing function
            {"01bd21d0", "AnalogCalibrationFilter_vtable3"},
            // Filter destroy
            {"01bd8420", "FilterDestroy"},
            // Also the SetCalibration handler -- find where calibration data
            // is parsed and stored
            // SetCalibration vtable at PTR_FUN_02f39348
            // calibrationSets deserializer
            {"02dd0a30", "CalibrationSets_deserialize"},
            // df1SosCoefficients deserializer
            // Let's find the xref to "/df1SosCoefficients" string
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        int tx = currentProgram.startTransaction("Create SIMD filter functions");
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
                            println("FAILED: " + t[0] + " (" + t[1] + ")");
                        }
                    }
                } else {
                    println("EXISTS: " + func.getName() + " @ " + t[0]);
                }
            }
        } finally {
            currentProgram.endTransaction(tx, true);
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
