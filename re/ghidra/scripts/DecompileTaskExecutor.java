// Decompile TaskExecutor, Mailbox, and threading infrastructure
// Finds functions by string cross-references in .rodata
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.symbol.*;

import java.util.*;

public class DecompileTaskExecutor extends GhidraScript {
    @Override
    public void run() throws Exception {
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        Memory mem = currentProgram.getMemory();
        ReferenceManager refMgr = currentProgram.getReferenceManager();

        String[] searchStrings = new String[] {
            "SALEAE_THREAD_COUNT used to set thread count",
            "locking mTaskQueueMutex",
            "mMailbox.IsOnMailboxThread()",
            "task_executor/src/task_executor.cpp",
            "task_executor/include/mailbox.h",
            "task handle destroyed",
            "Task (Mailbox)",
            "ProcessMailbox",
            "~TaskHandleInternals",
            "HandleMailbox",
            "RegisterCancelCallback",
        };

        Set<Address> functionAddrs = new LinkedHashSet<>();
        Map<Address, String> funcReasons = new LinkedHashMap<>();

        for (String searchStr : searchStrings) {
            byte[] bytes = searchStr.getBytes("UTF-8");
            Address found = mem.findBytes(mem.getMinAddress(), bytes, null, true, monitor);
            if (found == null) {
                println("STRING NOT FOUND: " + searchStr);
                continue;
            }
            println("STRING @ " + found + ": " + searchStr);

            // Find all references TO this string address
            ReferenceIterator refIter = refMgr.getReferencesTo(found);
            boolean gotRef = false;
            while (refIter.hasNext()) {
                Reference ref = refIter.next();
                gotRef = true;
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func != null) {
                    functionAddrs.add(func.getEntryPoint());
                    String existing = funcReasons.get(func.getEntryPoint());
                    String reason = "refs \"" + (searchStr.length() > 50 ? searchStr.substring(0, 50) + "..." : searchStr) + "\"";
                    funcReasons.put(func.getEntryPoint(), existing == null ? reason : existing + "; " + reason);
                    println("  -> Function: " + func.getName() + " @ " + func.getEntryPoint());
                } else {
                    println("  -> No function at " + fromAddr);
                }
            }
            if (!gotRef) {
                println("  No refs found for this string");
            }
        }

        // Also search for SALEAE_THREAD_COUNT as standalone env var string
        println("\n=== ENV VAR SEARCH ===");
        byte[] envBytes = "SALEAE_THREAD_COUNT".getBytes("UTF-8");
        Address searchAddr = mem.getMinAddress();
        while (searchAddr != null) {
            Address envAddr = mem.findBytes(searchAddr, envBytes, null, true, monitor);
            if (envAddr == null) break;
            println("ENV STRING @ " + envAddr);
            ReferenceIterator refIter = refMgr.getReferencesTo(envAddr);
            while (refIter.hasNext()) {
                Reference ref = refIter.next();
                Function func = fm.getFunctionContaining(ref.getFromAddress());
                if (func != null) {
                    functionAddrs.add(func.getEntryPoint());
                    funcReasons.put(func.getEntryPoint(),
                        funcReasons.getOrDefault(func.getEntryPoint(), "") + "; SALEAE_THREAD_COUNT env");
                    println("  -> Function: " + func.getName() + " @ " + func.getEntryPoint());
                }
            }
            searchAddr = envAddr.add(envBytes.length);
        }

        // Search for RTTI typeinfo strings
        println("\n=== RTTI TYPEINFO SEARCH ===");
        String[] rttiStrings = new String[] {
            "N6Saleae5Tasks12TaskExecutorE",
            "N6Saleae5Tasks6Detail21TaskExecutorInternalsE",
            "N6Saleae5Tasks6Detail19TaskHandleInternalsE",
            "N6Saleae5Graph27DefaultTaskExecutorProviderE",
            "N6Saleae5Graph20TaskExecutorProviderE",
        };

        for (String rtti : rttiStrings) {
            byte[] bytes = rtti.getBytes("UTF-8");
            Address found = mem.findBytes(mem.getMinAddress(), bytes, null, true, monitor);
            if (found != null) {
                println("RTTI @ " + found + ": " + rtti);
                ReferenceIterator refIter = refMgr.getReferencesTo(found);
                while (refIter.hasNext()) {
                    Reference ref = refIter.next();
                    Address from = ref.getFromAddress();
                    println("  typeinfo ref from " + from);
                    // Follow one more level: refs to the typeinfo struct -> constructors
                    ReferenceIterator vtRefIter = refMgr.getReferencesTo(from);
                    while (vtRefIter.hasNext()) {
                        Reference vr = vtRefIter.next();
                        Function func = fm.getFunctionContaining(vr.getFromAddress());
                        if (func != null) {
                            functionAddrs.add(func.getEntryPoint());
                            funcReasons.put(func.getEntryPoint(),
                                funcReasons.getOrDefault(func.getEntryPoint(), "") + "; RTTI:" + rtti);
                            println("    -> Function: " + func.getName() + " @ " + func.getEntryPoint());
                        }
                    }
                }
            } else {
                println("RTTI NOT FOUND: " + rtti);
            }
        }

        println("\n=== DECOMPILING " + functionAddrs.size() + " FUNCTIONS ===\n");

        // Decompile all found functions
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        for (Address addr : functionAddrs) {
            Function func = fm.getFunctionAt(addr);
            if (func == null) continue;

            long elfVA = addr.getOffset() - 0x100000;
            String reason = funcReasons.getOrDefault(addr, "unknown");

            println("=== " + func.getName() + " @ Ghidra:" + addr +
                    " ELF:0x" + Long.toHexString(elfVA) +
                    " (size:" + func.getBody().getNumAddresses() + ") ===");
            println("  Reason: " + reason);

            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("");
        }

        decomp.dispose();
        println("=== DONE ===");
    }
}
