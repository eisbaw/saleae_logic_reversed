// Decompile the actual SIMD filter processing functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileCalibFilter5 extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[][] targets = new String[][] {
            // Case 1: SimdIirFilterPrototype process (SSE)
            {"01bd7ed0", "SimdIirFilter_process_SSE"},
            // Case 2: Static2By3 SSE (no FMA)
            {"01bda550", "Static2By3_process_SSE"},
            // Case 3: Static2By3 FMA
            {"01bda8f0", "Static2By3_process_FMA"},
            // Case 4: Static2By3 AVX
            {"01bdac00", "Static2By3_process_AVX"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        int tx = currentProgram.startTransaction("Create filter process functions");
        try {
            for (String[] t : targets) {
                Address addr = af.getAddress(t[0]);
                Function func = fm.getFunctionAt(addr);
                if (func == null) {
                    Function containing = fm.getFunctionContaining(addr);
                    if (containing != null) {
                        println("NOTE: " + t[0] + " (" + t[1] + ") inside " + containing.getName());
                    } else {
                        CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                        cmd.applyTo(currentProgram);
                        func = fm.getFunctionAt(addr);
                        if (func != null) {
                            func.setName(t[1], SourceType.USER_DEFINED);
                            println("CREATED: " + t[1] + " @ " + t[0]);
                        }
                    }
                } else {
                    println("EXISTS: " + func.getName() + " @ " + t[0]);
                }
            }
        } finally {
            currentProgram.endTransaction(tx, true);
        }

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
