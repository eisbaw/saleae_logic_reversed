// Decompile the madvise decommit helper
// @category Analysis
// @author claude

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;

import java.io.File;
import java.io.PrintWriter;

public class DecompileMadvise extends GhidraScript {
    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        FunctionManager funcMgr = currentProgram.getFunctionManager();
        Function func = funcMgr.getFunctionAt(toAddr(0x2e08360L));
        if (func == null) {
            println("No function at 0x2e08360");
            return;
        }
        DecompileResults results = decomp.decompileFunction(func, 60, monitor);
        if (results.decompileCompleted()) {
            println("ELF VA: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000L));
            println("Size: " + func.getBody().getNumAddresses());
            println(results.getDecompiledFunction().getC());
        }
        decomp.dispose();
    }
}
