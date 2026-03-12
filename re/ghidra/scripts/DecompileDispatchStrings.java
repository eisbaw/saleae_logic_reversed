// Decompile the string-returning functions used in ProcessRequest dispatch
// These small functions return the messageType string constants
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileDispatchStrings extends GhidraScript {
    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setOptions(new DecompileOptions());

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // String-returning functions called in RemoteRequestHandler dispatch chain
        // Ghidra VA = ELF VA + 0x100000
        String[][] functions = new String[][] {
            // First comparison - matched -> FUN_01b05a00 (DriveRequest)
            {"0x1b36610", "dispatch string 1 (-> DriveRequest/submitActions)"},
            // Second comparison - matched -> FUN_01b071b0 (PopResponse)
            {"0x1b36760", "dispatch string 2 (-> PopResponse)"},
            // Third comparison - matched -> delete graph data
            {"0x1b36810", "dispatch string 3 (-> DeleteGraphData)"},
            // Fourth comparison - matched -> CauseCrash
            {"0x1b36980", "dispatch string 4 (-> CauseCrash)"},
            // Fifth comparison - matched -> start tracing
            {"0x1b3b950", "dispatch string 5 (-> startTracing)"},
            // Sixth comparison - matched -> stop tracing
            {"0x1b3b9c0", "dispatch string 6 (-> stopTracing)"},
            // Helper functions
            {"0x1b36770", "helper after PopResponse match"},
            {"0x1b36940", "helper after CauseCrash match"},
            {"0x1b3b980", "helper after startTracing match"},
            {"0x1b3b9d0", "helper used in tracing"},
            {"0x1b3b8c0", "helper in tracing setup"},
            {"0x1b3ba10", "helper after PopResponse"},
            // Also decompile the DriveRequest handler for action dispatch
            {"0x1b05a00", "DriveRequest handler (graph action dispatch)"},
        };

        for (String[] entry : functions) {
            String addrStr = entry[0];
            String desc = entry[1];
            Address addr = af.getAddress(addrStr);

            Function func = fm.getFunctionAt(addr);
            if (func == null) {
                func = fm.getFunctionContaining(addr);
            }

            if (func == null) {
                println("=== NO FUNCTION at " + addrStr + " (" + desc + ") ===");
                continue;
            }

            long size = func.getBody().getNumAddresses();
            println("=== " + desc + " @ " + func.getEntryPoint() + " (size=" + size + ") ===");

            DecompileResults results = decomp.decompileFunction(func, 120, monitor);
            if (results.decompileCompleted()) {
                String code = results.getDecompiledFunction().getC();
                // For small functions, print full. For large, print first 200 lines
                String[] lines = code.split("\n");
                if (lines.length > 200) {
                    StringBuilder sb = new StringBuilder();
                    for (int i = 0; i < 200; i++) {
                        sb.append(lines[i]).append("\n");
                    }
                    sb.append("... (truncated, ").append(lines.length).append(" total lines)\n");
                    println(sb.toString());
                } else {
                    println(code);
                }
            } else {
                println("  DECOMPILE FAILED: " + results.getErrorMessage());
            }
            println("");
        }

        decomp.dispose();
    }
}
