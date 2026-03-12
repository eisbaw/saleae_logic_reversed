// Decompile helper functions used by HMCAD1100 init
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import java.util.*;

public class DecompileHmcadHelpers extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Direct function entry point addresses (Ghidra VAs = ELF VA + 0x100000)
        String[][] funcs = new String[][] {
            {"0x1d207c0", "WriteHmcadRegister (FUN_01d207c0, called by InitHmcad1100)"},
            {"0x1d1f9b0", "FUN_01d1f9b0 (called from InitHmcad1100 for coarse gain)"},
            {"0x1d20f60", "FUN_01d20f60 (called from CommonDeviceInit for HMCAD)"},
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        for (String[] f : funcs) {
            String addrStr = f[0];
            String desc = f[1];
            Address addr = af.getAddress(addrStr);

            // Try exact match first
            Function func = fm.getFunctionAt(addr);
            if (func == null) {
                func = fm.getFunctionContaining(addr);
            }
            if (func == null) {
                println("--- NO FUNCTION at/containing " + addrStr + " (" + desc + ") ---");
                continue;
            }

            println("=== " + func.getName() + " @ " + func.getEntryPoint() + " (" + desc + ") ===");
            println("ELF VA: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000));
            println("Function size: " + func.getBody().getNumAddresses() + " bytes");
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
