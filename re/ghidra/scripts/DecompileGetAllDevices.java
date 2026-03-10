// Decompile GetAllDevices and CheckForConnectionThread
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileGetAllDevices extends GhidraScript {
    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Ghidra VA addresses (ELF VA + 0x100000)
        String[] targets = new String[] {
            "0x1c28240",   // GetAllDevices (ELF 0x1c18240)
            "0x1c282c1",   // Inside VID/PID dispatch
            "0x1c28440",   // CheckForConnectionThread (ELF 0x1c18440)
            "0x1c28500",   // Inside CheckForConnectionThread
            "0x1c28600",
            "0x1c28700",
            "0x1c28800",
        };

        java.util.Set<String> seen = new java.util.HashSet<>();

        for (String addrStr : targets) {
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("NO FUNCTION at Ghidra " + addrStr);
                // Try listing nearby functions
                FunctionIterator iter = fm.getFunctions(addr, true);
                if (iter.hasNext()) {
                    Function next = iter.next();
                    println("  Next function: " + next.getName() + " @ " + next.getEntryPoint());
                }
                FunctionIterator iter2 = fm.getFunctions(addr, false);
                if (iter2.hasNext()) {
                    Function prev = iter2.next();
                    println("  Prev function: " + prev.getName() + " @ " + prev.getEntryPoint());
                }
                continue;
            }

            String key = func.getEntryPoint().toString();
            if (seen.contains(key)) continue;
            seen.add(key);

            println("=== FUNCTION @ " + func.getEntryPoint() + " (name: " + func.getName() + ") ===");
            println("    ELF VA: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000));
            println("    Size: " + func.getBody().getNumAddresses() + " bytes");
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
