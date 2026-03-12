// Decompile FUN_01d22af0 (Ghidra VA) - ADC init helper function
// ELF VA: 0x1c22af0, Ghidra VA: 0x1d22af0 (rebased +0x100000)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import java.io.*;

public class DecompileAdcHelper extends GhidraScript {
    @Override
    public void run() throws Exception {
        String targetAddr = "0x01d22af0";
        String outPath = "/home/mpedersen/nvidia/saleae_reverse/re/ghidra/decompiled-fun-01d22af0.txt";

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        Address addr = af.getAddress(targetAddr);
        Function func = fm.getFunctionContaining(addr);
        if (func == null) {
            println("NO FUNCTION at " + targetAddr);
            return;
        }

        println("=== " + func.getName() + " @ " + func.getEntryPoint()
                + " (ELF: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000) + ") ===");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        DecompileResults res = decomp.decompileFunction(func, 120, monitor);
        if (res.decompileCompleted()) {
            String code = res.getDecompiledFunction().getC();
            println(code);

            // Also write to file
            PrintWriter pw = new PrintWriter(new FileWriter(outPath));
            pw.println("=== " + func.getName() + " @ " + func.getEntryPoint()
                    + " (ELF: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000) + ") ===");
            pw.println();
            pw.println(code);

            // Also decompile any called functions within a small range to understand context
            // Get all called functions
            java.util.Set<Function> called = func.getCalledFunctions(monitor);
            if (!called.isEmpty()) {
                pw.println("\n\n========== CALLED FUNCTIONS ==========\n");
                for (Function callee : called) {
                    long offset = callee.getEntryPoint().getOffset();
                    // Skip library/thunk functions (typically at very high or very low addresses)
                    if (offset < 0x100000) continue;
                    pw.println("--- " + callee.getName() + " @ " + callee.getEntryPoint()
                            + " (ELF: 0x" + Long.toHexString(offset - 0x100000) + ") ---");
                    DecompileResults calleeRes = decomp.decompileFunction(callee, 120, monitor);
                    if (calleeRes.decompileCompleted()) {
                        pw.println(calleeRes.getDecompiledFunction().getC());
                    } else {
                        pw.println("DECOMPILE FAILED: " + calleeRes.getErrorMessage());
                    }
                    pw.println();
                }
            }

            pw.close();
            println("Output written to: " + outPath);
        } else {
            println("DECOMPILE FAILED: " + res.getErrorMessage());
        }

        decomp.dispose();
    }
}
