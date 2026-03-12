// Decompile GraphServer::ProcessRequest and device-related handlers
// Key functions for TASK-0132: message type catalog
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;

import java.util.*;

public class DecompileProcessRequest extends GhidraScript {
    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setOptions(new DecompileOptions());

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Key functions (Ghidra VA = ELF VA + 0x100000)
        // ELF VAs from remote-client.md and graph-server.md
        String[][] functions = new String[][] {
            // ProcessRequest / RemoteRequestHandler
            {"0x1b040c0", "RemoteRequestHandler (messageType dispatch)"},
            // DriveRequest handler (processes graph actions)
            {"0x1b05a00", "DriveRequest handler (graph action dispatch)"},
            // PopResponse handler
            {"0x1b071b0", "PopResponse handler"},
            // HandleDeleteGraphRequest
            {"0x1b07af0", "HandleDeleteGraphRequest"},
            // EnqueueResponse
            {"0x1b07d20", "EnqueueResponse"},
            // GraphServer ProcessRequest entry
            {"0x1b01780", "CreateGraphServer"},
            // GraphServer constructor
            {"0x1b02560", "GraphServer constructor"},
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

            println("=== " + desc + " @ " + func.getEntryPoint() + " (size=" + func.getBody().getNumAddresses() + ") ===");

            DecompileResults results = decomp.decompileFunction(func, 120, monitor);
            if (results.decompileCompleted()) {
                println(results.getDecompiledFunction().getC());
            } else {
                println("  DECOMPILE FAILED: " + results.getErrorMessage());
            }
            println("");
        }

        // Now search for key strings to find additional dispatch functions
        println("\n\n=== STRING SEARCH: messageType-related strings ===\n");

        ReferenceManager rm = currentProgram.getReferenceManager();
        String[][] stringSearches = new String[][] {
            {"messageType", "messageType JSON key"},
            {"submitActions", "submitActions messageType"},
            {"sendGraphActions", "sendGraphActions messageType"},
            {"sendLogicDeviceRequest", "sendLogicDeviceRequest messageType"},
            {"createGraph", "createGraph messageType"},
            {"destroyGraph", "destroyGraph messageType"},
            {"addLogicDeviceNode", "addLogicDeviceNode messageType"},
            {"BAD_MESSAGE_TYPE", "error string"},
            {"WRONG_MESSAGE_TYPE", "error string"},
        };

        for (String[] entry : stringSearches) {
            String searchStr = entry[0];
            String desc = entry[1];
            println("--- Searching for string: \"" + searchStr + "\" (" + desc + ") ---");

            ghidra.program.model.mem.Memory mem = currentProgram.getMemory();
            Address startAddr = mem.getMinAddress();
            Address found = mem.findBytes(startAddr, searchStr.getBytes(), null, true, monitor);

            int count = 0;
            while (found != null && count < 5) {
                println("  Found at: " + found);

                for (Reference ref : rm.getReferencesTo(found)) {
                    Address fromAddr = ref.getFromAddress();
                    Function refFunc = fm.getFunctionContaining(fromAddr);
                    if (refFunc != null) {
                        println("    Referenced from: " + refFunc.getEntryPoint() +
                                " (" + refFunc.getName() + ") at " + fromAddr);
                    } else {
                        println("    Referenced from: " + fromAddr + " (no function)");
                    }
                }
                count++;
                found = mem.findBytes(found.add(1), searchStr.getBytes(), null, true, monitor);
            }
        }

        decomp.dispose();
    }
}
