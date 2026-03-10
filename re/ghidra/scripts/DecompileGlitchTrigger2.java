// Decompile remaining MsoFpga functions for TASK-0091
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileGlitchTrigger2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Remaining functions from the MsoFpga region not yet decompiled
        String[][] targets = new String[][] {
            // Functions not yet decompiled from the function list
            {"02e66380", "MsoFpga_unknown_109bytes"},
            {"02e66870", "MsoFpga_unknown_106bytes"},
            {"02e668e0", "MsoFpga_unknown_304bytes"},
            {"02e66c60", "MsoFpga_unknown_196bytes"},
            {"02e66d30", "MsoFpga_unknown_48bytes"},
            {"02e66f30", "MsoFpga_unknown_652bytes"},
            {"02e671c0", "MsoFpga_unknown_108bytes"},
            {"02e67230", "MsoFpga_unknown_783bytes"},
            {"02e65420", "MsoFpga_unknown_122bytes"},
            {"02e650e0", "MsoFpga_unknown_401bytes_a"},
            {"02e65280", "MsoFpga_unknown_401bytes_b"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        java.util.Set<String> decompiled = new java.util.HashSet<>();

        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("=== NO FUNCTION containing " + t[0] + " (" + t[1] + ") ===");
                continue;
            }

            String funcKey = func.getEntryPoint().toString();
            if (decompiled.contains(funcKey)) {
                println("=== ALREADY DECOMPILED: " + func.getName() + " @ " + funcKey + " (for " + t[1] + ") ===");
                continue;
            }
            decompiled.add(funcKey);

            println("=== " + func.getName() + " @ " + func.getEntryPoint() + " (via " + t[1] + ") ===");
            println("Size: " + func.getBody().getNumAddresses() + " bytes");
            DecompileResults res = decomp.decompileFunction(func, 180, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("");
        }
        decomp.dispose();
    }
}
