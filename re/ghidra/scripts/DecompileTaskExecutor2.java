// Decompile specific TaskExecutor addresses and related functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.symbol.*;

import java.util.*;

public class DecompileTaskExecutor2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        Memory mem = currentProgram.getMemory();
        ReferenceManager refMgr = currentProgram.getReferenceManager();

        // Targets found from xref analysis (Ghidra VAs = ELF + 0x100000)
        String[][] targets = new String[][] {
            // FUN_02de0b10 - called from constructor, likely starts worker threads
            {"02de0b10", "TaskExecutor_StartThreads"},
            // FUN_02de0680 - constructor with SALEAE_THREAD_COUNT
            {"02de0680", "TaskExecutor_Constructor"},
            // FUN_02dac8c0 - called from Mailbox/Node init
            {"02dac8c0", "TaskExecutor_InitQueue"},
            // FUN_02ddee90 - called after queue push, likely wakes threads
            {"02ddee90", "TaskExecutor_WakeUpThread"},
            // FUN_02ddef70 - called for task processing
            {"02ddef70", "TaskExecutor_ScheduleOrRun"},
            // FUN_02dded40 - task related
            {"02dded40", "TaskExecutor_JoinOrCancel"},
            // FUN_02de4000 - task related
            {"02de4000", "TaskExecutor_CancelTask"},
        };

        // Also find functions referencing key strings
        String[] extraStrings = new String[] {
            "Waiting for idle (tasks: {}, pending tasks: {}, active threads: {})",
            "ThreadFunc",
            "GetThreadCount",
        };

        Set<Address> functionAddrs = new LinkedHashSet<>();
        Map<Address, String> funcNames = new LinkedHashMap<>();

        // Add explicit targets
        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            functionAddrs.add(addr);
            funcNames.put(addr, t[1]);
        }

        // Find functions from string xrefs
        for (String searchStr : extraStrings) {
            byte[] bytes = searchStr.getBytes("UTF-8");
            Address found = mem.findBytes(mem.getMinAddress(), bytes, null, true, monitor);
            if (found == null) {
                println("STRING NOT FOUND: " + searchStr);
                continue;
            }
            println("STRING @ " + found + ": " + searchStr);
            ReferenceIterator refIter = refMgr.getReferencesTo(found);
            while (refIter.hasNext()) {
                Reference ref = refIter.next();
                Function func = fm.getFunctionContaining(ref.getFromAddress());
                if (func != null) {
                    functionAddrs.add(func.getEntryPoint());
                    funcNames.put(func.getEntryPoint(), "xref_" + searchStr.substring(0, Math.min(30, searchStr.length())));
                    println("  -> Function: " + func.getName() + " @ " + func.getEntryPoint());
                }
            }
        }

        println("\n=== DECOMPILING " + functionAddrs.size() + " FUNCTIONS ===\n");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        for (Address addr : functionAddrs) {
            Function func = fm.getFunctionAt(addr);
            if (func == null) {
                // Try to create the function
                int tx = currentProgram.startTransaction("Create function");
                try {
                    CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                    cmd.applyTo(currentProgram);
                    func = fm.getFunctionAt(addr);
                } finally {
                    currentProgram.endTransaction(tx, true);
                }
            }

            if (func == null) {
                func = fm.getFunctionContaining(addr);
                if (func != null) {
                    println("=== " + addr + " inside " + func.getName() + " @ " + func.getEntryPoint() + " ===");
                } else {
                    println("=== NO FUNCTION at " + addr + " ===");
                    continue;
                }
            }

            long elfVA = func.getEntryPoint().getOffset() - 0x100000;
            String label = funcNames.getOrDefault(addr, "unknown");

            println("=== " + func.getName() + " @ Ghidra:" + func.getEntryPoint() +
                    " ELF:0x" + Long.toHexString(elfVA) +
                    " (" + label + ")" +
                    " (size:" + func.getBody().getNumAddresses() + ") ===");

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
