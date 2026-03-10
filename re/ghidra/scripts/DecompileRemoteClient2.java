// Decompile RemoteClient constructor and WebSocket setup functions
// Phase 2: find xrefs to ^/saleae path and the GraphServer constructor
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;

import java.util.*;

public class DecompileRemoteClient2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Ghidra VAs to search for xrefs
        String[][] stringAddrs = new String[][] {
            {"0x4bdbeb", "^/saleae/?$ WebSocket path regex"},
            {"0x4555c5", "^/analyzer/?$ WebSocket path regex"},
        };

        // Also decompile specific functions found in phase 1
        String[] directAddrs = new String[] {
            // FUN_01b02560 = GraphServer constructor body (called from CreateGraphServer)
            "0x1b02560",
            // FUN_01b3d740 = send over websocket (called from EnqueueResponse when ws active)
            "0x1b3d740",
            // FUN_01b3d750 = get ws port (called from CreateGraphServer after ws init)
            "0x1b3d750",
            // FUN_01b05a00 = called with "DriveRequest" type match
            "0x1b05a00",
            // FUN_01b071b0 = called with "PopResponse" type match
            "0x1b071b0",
            // FUN_01b07af0 = called with another type match
            "0x1b07af0",
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();

        Set<Address> decompiled = new HashSet<>();

        // Phase A: xrefs to strings
        for (String[] entry : stringAddrs) {
            String addrStr = entry[0];
            String desc = entry[1];
            Address strAddr = af.getAddress(addrStr);

            println("=== Finding xrefs to string: " + desc + " @ " + addrStr + " ===");

            List<Reference> refs = new ArrayList<>();
            for (Reference r : rm.getReferencesTo(strAddr)) {
                refs.add(r);
            }
            if (refs.isEmpty()) {
                for (int offset = -4; offset <= 4; offset++) {
                    if (offset == 0) continue;
                    Address nearby = strAddr.add(offset);
                    for (Reference r : rm.getReferencesTo(nearby)) {
                        refs.add(r);
                    }
                    if (!refs.isEmpty()) {
                        println("  Found " + refs.size() + " refs at offset " + offset);
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
                    println("  Already decompiled: " + func.getName());
                    continue;
                }
                decompiled.add(func.getEntryPoint());

                long elfVA = func.getEntryPoint().getOffset() - 0x100000;
                println("\n--- Decompiling: " + func.getName() + " @ " + func.getEntryPoint()
                        + " (ELF VA: 0x" + Long.toHexString(elfVA) + ") ---");

                DecompileResults res = decomp.decompileFunction(func, 120, monitor);
                if (res.decompileCompleted()) {
                    println(res.getDecompiledFunction().getC());
                } else {
                    println("DECOMPILE FAILED: " + res.getErrorMessage());
                }
            }
        }

        // Phase B: direct address decompilation
        println("\n\n=== Direct function decompilation ===\n");
        for (String addrStr : directAddrs) {
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("--- NO FUNCTION at " + addrStr + " ---");
                continue;
            }
            if (decompiled.contains(func.getEntryPoint())) {
                println("--- Already decompiled: " + func.getName() + " at " + addrStr + " ---");
                continue;
            }
            decompiled.add(func.getEntryPoint());

            long elfVA = func.getEntryPoint().getOffset() - 0x100000;
            println("\n--- Decompiling: " + func.getName() + " @ " + func.getEntryPoint()
                    + " (ELF VA: 0x" + Long.toHexString(elfVA) + ") ---");

            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
        }

        decomp.dispose();
    }
}
