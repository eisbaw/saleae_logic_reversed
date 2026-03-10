// Find boost::multiprecision usage by tracing string references and decompiling callers
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class FindMultiprecision extends GhidraScript {

    private List<Reference> refsToList(ReferenceIterator iter) {
        List<Reference> list = new ArrayList<>();
        while (iter.hasNext()) {
            list.add(iter.next());
        }
        return list;
    }

    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();
        Listing listing = currentProgram.getListing();

        // Search strings in the program for multiprecision-related content
        println("=== PHASE 1: Search for multiprecision-related strings ===");

        String[] searchStrings = new String[] {
            "divide.hpp",
            "cpp_int",
            "multiprecision",
            "checked_cpp_int",
            "eval_divide",
            "eval_multiply",
            "eval_add",
            "eval_subtract",
            "divide_unsigned_helper",
        };

        // Scan all defined strings in the program
        DataIterator dataIter = listing.getDefinedData(true);
        Set<Address> stringAddrs = new HashSet<>();

        while (dataIter.hasNext() && !monitor.isCancelled()) {
            Data data = dataIter.next();
            if (data.hasStringValue()) {
                String val = (String) data.getValue();
                if (val != null) {
                    for (String s : searchStrings) {
                        if (val.contains(s)) {
                            println("  Found string: '" + val + "' at " + data.getAddress());
                            stringAddrs.add(data.getAddress());
                            break;
                        }
                    }
                }
            }
        }

        println("\nFound " + stringAddrs.size() + " matching strings");

        // For each string, find references to it
        println("\n=== PHASE 2: Find references to matching strings ===");
        Set<Function> callerFunctions = new LinkedHashSet<>();

        for (Address strAddr : stringAddrs) {
            List<Reference> refs = refsToList(rm.getReferencesTo(strAddr));
            println("  String at " + strAddr + " has " + refs.size() + " references:");
            for (Reference ref : refs) {
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func != null) {
                    println("    -> referenced from " + func.getName() + " @ " + func.getEntryPoint() + " (ref at " + fromAddr + ")");
                    callerFunctions.add(func);
                } else {
                    println("    -> referenced from " + fromAddr + " (no function)");
                }
            }
        }

        // Also try the direct address for divide.hpp
        // ELF VA 0x426c50 + 0x100000 = Ghidra 0x526c50
        Address divideHppAddr = af.getAddress("0x526c50");
        if (divideHppAddr != null) {
            List<Reference> refs = refsToList(rm.getReferencesTo(divideHppAddr));
            println("\n  Direct check divide.hpp at Ghidra 0x526c50: " + refs.size() + " refs");
            for (Reference ref : refs) {
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func != null) {
                    println("    -> " + func.getName() + " @ " + func.getEntryPoint());
                    callerFunctions.add(func);
                } else {
                    println("    -> from " + fromAddr + " (no function)");
                }
            }
        }

        // PHASE 3: Decompile all caller functions
        println("\n=== PHASE 3: Decompile caller functions (" + callerFunctions.size() + " total) ===");
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        for (Function func : callerFunctions) {
            println("\n--- Decompiling " + func.getName() + " @ " + func.getEntryPoint() + " ---");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                String code = res.getDecompiledFunction().getC();
                println(code);
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
        }

        // PHASE 4: Look for callers of the multiprecision functions (2 levels up)
        println("\n=== PHASE 4: Callers of multiprecision functions (2nd level) ===");
        Set<Function> level2Callers = new LinkedHashSet<>();
        for (Function func : callerFunctions) {
            Address entry = func.getEntryPoint();
            List<Reference> refs = refsToList(rm.getReferencesTo(entry));
            for (Reference ref : refs) {
                if (ref.getReferenceType().isCall()) {
                    Function caller = fm.getFunctionContaining(ref.getFromAddress());
                    if (caller != null && !callerFunctions.contains(caller)) {
                        level2Callers.add(caller);
                        println("  " + func.getName() + " called by " + caller.getName() + " @ " + caller.getEntryPoint());
                    }
                }
            }
        }

        // Decompile level 2 callers (limit to 30)
        println("\n=== PHASE 5: Decompile level-2 callers (" + level2Callers.size() + " total, max 30) ===");
        int count = 0;
        for (Function func : level2Callers) {
            if (count++ >= 30) {
                println("... truncated (more than 30 level-2 callers)");
                break;
            }
            println("\n--- Decompiling L2: " + func.getName() + " @ " + func.getEntryPoint() + " ---");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                String code = res.getDecompiledFunction().getC();
                println(code);
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
        }

        decomp.dispose();
        println("\n=== DONE ===");
    }
}
