// Find all functions using 128-bit integer intrinsics
// These are the compiler builtins that boost::multiprecision generates for 128-bit ops
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class Find128BitOps extends GhidraScript {

    private List<Reference> refsToList(ReferenceIterator iter) {
        List<Reference> list = new ArrayList<>();
        while (iter.hasNext()) {
            list.add(iter.next());
        }
        return list;
    }

    @Override
    public void run() throws Exception {
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();
        SymbolTable st = currentProgram.getSymbolTable();

        // 128-bit compiler intrinsics that indicate multiprecision usage
        String[] intrinsicNames = new String[] {
            "__umodti3",      // unsigned 128-bit modulus
            "__udivti3",      // unsigned 128-bit division
            "__divti3",       // signed 128-bit division
            "__modti3",       // signed 128-bit modulus
            "__fixunsdfti",   // double -> unsigned 128-bit
            "__fixdfti",      // double -> signed 128-bit
            "__floatuntidf",  // unsigned 128-bit -> double
            "__floattidf",    // signed 128-bit -> double
            "__multi3",       // 128-bit multiply
        };

        println("=== PHASE 1: Find 128-bit intrinsic symbols ===");
        Map<String, Set<Function>> intrinsicCallers = new LinkedHashMap<>();

        for (String name : intrinsicNames) {
            SymbolIterator syms = st.getSymbols(name);
            while (syms.hasNext()) {
                Symbol sym = syms.next();
                Address addr = sym.getAddress();
                println("  Found intrinsic: " + name + " at " + addr);

                List<Reference> refs = refsToList(rm.getReferencesTo(addr));
                Set<Function> callers = new LinkedHashSet<>();
                for (Reference ref : refs) {
                    if (ref.getReferenceType().isCall()) {
                        Function caller = fm.getFunctionContaining(ref.getFromAddress());
                        if (caller != null) {
                            callers.add(caller);
                        }
                    }
                }
                intrinsicCallers.put(name, callers);
                println("    " + callers.size() + " callers");
            }
        }

        // Collect all unique caller functions
        Set<Function> allCallers = new LinkedHashSet<>();
        for (Map.Entry<String, Set<Function>> entry : intrinsicCallers.entrySet()) {
            for (Function f : entry.getValue()) {
                allCallers.add(f);
            }
        }

        println("\n=== PHASE 2: Summary of 128-bit intrinsic callers ===");
        println("Total unique functions using 128-bit ops: " + allCallers.size());
        for (Function f : allCallers) {
            List<String> usedIntrinsics = new ArrayList<>();
            for (Map.Entry<String, Set<Function>> entry : intrinsicCallers.entrySet()) {
                if (entry.getValue().contains(f)) {
                    usedIntrinsics.add(entry.getKey());
                }
            }
            println("  " + f.getName() + " @ " + f.getEntryPoint() + " uses: " + String.join(", ", usedIntrinsics));
        }

        // PHASE 3: For each caller function, find ITS callers (1 level up)
        println("\n=== PHASE 3: Who calls the 128-bit functions? ===");
        Set<Function> level2Callers = new LinkedHashSet<>();
        for (Function func : allCallers) {
            Address entry = func.getEntryPoint();
            List<Reference> refs = refsToList(rm.getReferencesTo(entry));
            List<String> callerNames = new ArrayList<>();
            for (Reference ref : refs) {
                if (ref.getReferenceType().isCall()) {
                    Function caller = fm.getFunctionContaining(ref.getFromAddress());
                    if (caller != null && !allCallers.contains(caller)) {
                        level2Callers.add(caller);
                        callerNames.add(caller.getName() + "@" + caller.getEntryPoint());
                    }
                }
            }
            if (!callerNames.isEmpty()) {
                println("  " + func.getName() + " @ " + func.getEntryPoint() + " called by:");
                for (String cn : callerNames) {
                    println("    -> " + cn);
                }
            }
        }

        // PHASE 4: Decompile all 128-bit functions
        println("\n=== PHASE 4: Decompile all 128-bit functions ===");
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        for (Function func : allCallers) {
            println("\n--- " + func.getName() + " @ " + func.getEntryPoint() + " (ELF: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000) + ") ---");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
        }

        // PHASE 5: Decompile level-2 callers
        println("\n=== PHASE 5: Decompile level-2 callers (" + level2Callers.size() + " total, max 40) ===");
        int count = 0;
        for (Function func : level2Callers) {
            if (count++ >= 40) {
                println("... truncated");
                break;
            }
            println("\n--- L2: " + func.getName() + " @ " + func.getEntryPoint() + " (ELF: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000) + ") ---");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
        }

        decomp.dispose();
        println("\n=== DONE ===");
    }
}
