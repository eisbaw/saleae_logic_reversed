// Decompile specific functions by Ghidra VA
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileSpecific extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] addrs = new String[] {
            // FUN_02eb2670 - called by many __fixunsdfti users, likely GraphTime constructor
            "0x02eb2670",
            // MakeGraphTime
            "0x01d89c40",
            // A few from the 0x1d8/0x1d9 cluster to identify them
            "0x01d8ac30",  // L2 caller of FUN_01d8eef0
            "0x01d93c40",  // L2 caller
            "0x01d93cd0",  // L2 caller
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        for (String addrStr : addrs) {
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("--- NO FUNCTION at " + addrStr + " ---");
                continue;
            }
            println("=== " + func.getName() + " @ " + func.getEntryPoint()
                    + " (ELF: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000) + ") ===");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
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
