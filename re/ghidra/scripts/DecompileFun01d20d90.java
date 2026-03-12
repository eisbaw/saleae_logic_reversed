// Decompile FUN_01d20d90 (ADC interface init for AD9637)
// Ghidra VA 0x01d20d90 (ELF VA 0x1c20d90, rebased +0x100000)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;
import java.io.*;

public class DecompileFun01d20d90 extends GhidraScript {
    @Override
    public void run() throws Exception {
        String outPath = "/home/mpedersen/nvidia/saleae_reverse/re/ghidra/decompiled-fun-01d20d90.txt";
        PrintWriter pw = new PrintWriter(new FileWriter(outPath));

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Main target + any functions it calls that we want to also decompile
        String[][] targets = new String[][] {
            {"01d20d90", "FUN_01d20d90_ADC_interface_init"},
        };

        // Create functions where missing
        int tx = currentProgram.startTransaction("Create functions for decompile");
        try {
            for (String[] t : targets) {
                Address addr = af.getAddress(t[0]);
                Function func = fm.getFunctionAt(addr);
                if (func == null) {
                    Function containing = fm.getFunctionContaining(addr);
                    if (containing != null) {
                        String msg = "NOTE: " + t[0] + " (" + t[1] + ") inside " + containing.getName() + " @ " + containing.getEntryPoint();
                        println(msg);
                        pw.println(msg);
                    } else {
                        CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                        cmd.applyTo(currentProgram);
                        func = fm.getFunctionAt(addr);
                        if (func != null) {
                            func.setName(t[1], SourceType.USER_DEFINED);
                            String msg = "CREATED: " + t[1] + " @ " + t[0];
                            println(msg);
                            pw.println(msg);
                        } else {
                            String msg = "FAILED to create function at " + t[0];
                            println(msg);
                            pw.println(msg);
                        }
                    }
                } else {
                    String msg = "EXISTS: " + func.getName() + " @ " + t[0];
                    println(msg);
                    pw.println(msg);
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
                String msg = "=== NO FUNCTION AT " + t[0] + " (" + t[1] + ") ===";
                println(msg);
                pw.println(msg);
                continue;
            }
            String header = "=== " + func.getName() + " @ " + func.getEntryPoint() + " (expected: " + t[1] + ") ===";
            println(header);
            pw.println(header);

            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                String code = res.getDecompiledFunction().getC();
                println(code);
                pw.println(code);
            } else {
                String msg = "DECOMPILE FAILED: " + res.getErrorMessage();
                println(msg);
                pw.println(msg);
            }
            pw.println("");
        }

        // Also decompile any called functions we can find via references
        // Look at the main function's body for CALL references
        Address mainAddr = af.getAddress("01d20d90");
        Function mainFunc = fm.getFunctionAt(mainAddr);
        if (mainFunc == null) mainFunc = fm.getFunctionContaining(mainAddr);
        if (mainFunc != null) {
            pw.println("\n=== CALLED FUNCTIONS (auto-discovered) ===\n");
            println("\n=== CALLED FUNCTIONS (auto-discovered) ===\n");

            // Get all functions called from mainFunc
            java.util.Set<Function> calledFuncs = mainFunc.getCalledFunctions(monitor);
            for (Function called : calledFuncs) {
                String header = "--- " + called.getName() + " @ " + called.getEntryPoint() + " ---";
                println(header);
                pw.println(header);

                DecompileResults res = decomp.decompileFunction(called, 120, monitor);
                if (res.decompileCompleted()) {
                    String code = res.getDecompiledFunction().getC();
                    println(code);
                    pw.println(code);
                } else {
                    String msg = "DECOMPILE FAILED: " + res.getErrorMessage();
                    println(msg);
                    pw.println(msg);
                }
                pw.println("");
            }
        }

        decomp.dispose();
        pw.close();
        println("Output written to: " + outPath);
    }
}
