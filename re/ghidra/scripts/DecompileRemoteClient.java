// Decompile functions related to RemoteClient module
// Finds xrefs to key strings and decompiles referencing functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;

import java.util.*;

public class DecompileRemoteClient extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Key string addresses (Ghidra VA = ELF VA + 0x100000)
        // These are strings in .rodata that reference remote_client functionality
        String[][] stringAddrs = new String[][] {
            // {Ghidra VA, description}
            {"0x53b49d", "remote_client.cpp assertion path"},
            {"0x489250", "websocket transport not enabled"},
            {"0x462c92", "RemoteRequestHandler"},
            {"0x4e6f06", "EnqueueResponse assertion"},
            {"0x52db93", "EnqueueResponse signature string"},
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();

        Set<Address> decompiled = new HashSet<>();

        for (String[] entry : stringAddrs) {
            String addrStr = entry[0];
            String desc = entry[1];
            Address strAddr = af.getAddress(addrStr);

            println("=== Finding xrefs to string: " + desc + " @ " + addrStr + " ===");

            // Get all references TO this string address
            List<Reference> refs = new ArrayList<>();
            for (Reference r : rm.getReferencesTo(strAddr)) {
                refs.add(r);
            }
            if (refs.isEmpty()) {
                println("  No xrefs found for " + addrStr);
                // Try scanning nearby (sometimes strings get split)
                for (int offset = -4; offset <= 4; offset++) {
                    if (offset == 0) continue;
                    Address nearby = strAddr.add(offset);
                    for (Reference r : rm.getReferencesTo(nearby)) {
                        refs.add(r);
                    }
                    if (!refs.isEmpty()) {
                        println("  Found " + refs.size() + " refs at offset " + offset + " (" + nearby + ")");
                        break;
                    }
                }
            }

            println("  Found " + refs.size() + " xrefs");

            for (Reference ref : refs) {
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func == null) {
                    println("  Xref from " + fromAddr + " -- no containing function");
                    continue;
                }
                if (decompiled.contains(func.getEntryPoint())) {
                    println("  Xref from " + fromAddr + " in " + func.getName() + " -- already decompiled");
                    continue;
                }
                decompiled.add(func.getEntryPoint());

                // Convert back to ELF VA for documentation
                long elfVA = func.getEntryPoint().getOffset() - 0x100000;
                println("\n--- Decompiling: " + func.getName() + " @ " + func.getEntryPoint()
                        + " (ELF VA: 0x" + Long.toHexString(elfVA) + ") ---");
                println("  Referenced string: " + desc);

                DecompileResults res = decomp.decompileFunction(func, 120, monitor);
                if (res.decompileCompleted()) {
                    println(res.getDecompiledFunction().getC());
                } else {
                    println("DECOMPILE FAILED: " + res.getErrorMessage());
                }
                println("");
            }
        }

        decomp.dispose();
    }
}
