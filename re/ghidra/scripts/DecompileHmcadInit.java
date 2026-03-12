// Decompile HMCAD1100 ADC init functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import java.util.*;

public class DecompileHmcadInit extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Ghidra VAs (ELF VA + 0x100000) for xrefs to HMCAD-related strings
        String[][] refs = new String[][] {
            // InitHmcad1100 assertion string xref
            {"0x1d20cc2", "InitHmcad1100 (xref to assertion string)"},
            // SetDefault(DeviceType) assertion string xrefs
            {"0x1d1f98b", "Hmcad1100Settings::SetDefault (xref 1)"},
            {"0x1d22861", "Hmcad1100Settings::SetDefault (xref 2)"},
            // SetTestPattenHmcad1100 assertion string xrefs
            {"0x1d20c72", "SetTestPattenHmcad1100 (xref 1)"},
            {"0x1d20d63", "SetTestPattenHmcad1100 (xref 2)"},
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        Set<String> decompiled = new HashSet<>();

        for (String[] ref : refs) {
            String addrStr = ref[0];
            String desc = ref[1];
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("--- NO FUNCTION containing " + addrStr + " (" + desc + ") ---");
                continue;
            }
            String entryStr = func.getEntryPoint().toString();
            if (decompiled.contains(entryStr)) {
                println("--- ALREADY DECOMPILED: " + func.getName() + " @ " + entryStr + " (from " + desc + ") ---");
                println("");
                continue;
            }
            decompiled.add(entryStr);

            println("=== " + func.getName() + " @ " + entryStr + " (found via: " + desc + ") ===");
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
