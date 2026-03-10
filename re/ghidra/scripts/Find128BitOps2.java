// Find all functions using 128-bit integer intrinsics (via thunk resolution)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class Find128BitOps2 extends GhidraScript {

    private List<Reference> refsToList(ReferenceIterator iter) {
        List<Reference> list = new ArrayList<>();
        while (iter.hasNext()) {
            list.add(iter.next());
        }
        return list;
    }

    @Override
    public void run() throws Exception {
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();
        SymbolTable st = currentProgram.getSymbolTable();

        // 128-bit compiler intrinsics
        String[] intrinsicNames = new String[] {
            "__umodti3",
            "__udivti3",
            "__divti3",
            "__modti3",
            "__fixunsdfti",
            "__fixdfti",
            "__floatuntidf",
            "__floattidf",
            "__multi3",
        };

        println("=== PHASE 1: Find 128-bit intrinsic thunks and their callers ===");
        Set<Function> allCallers = new LinkedHashSet<>();
        Map<Function, Set<String>> callerIntrinsics = new LinkedHashMap<>();

        for (String name : intrinsicNames) {
            // Try to find by function name (includes thunks)
            FunctionIterator fiter = fm.getFunctions(true);
            List<Function> matchingFuncs = new ArrayList<>();
            while (fiter.hasNext()) {
                Function f = fiter.next();
                if (f.getName().equals(name)) {
                    matchingFuncs.add(f);
                }
            }

            // Also search symbols
            SymbolIterator syms = st.getSymbols(name);
            List<Address> symAddrs = new ArrayList<>();
            while (syms.hasNext()) {
                Symbol sym = syms.next();
                symAddrs.add(sym.getAddress());
            }

            if (matchingFuncs.isEmpty() && symAddrs.isEmpty()) {
                continue;
            }

            println("  " + name + ": " + matchingFuncs.size() + " functions, " + symAddrs.size() + " symbols");

            // Find callers via function references
            for (Function f : matchingFuncs) {
                println("    Function at " + f.getEntryPoint() + " (thunk=" + f.isThunk() + ")");
                // Get references to this function's entry point
                List<Reference> refs = refsToList(rm.getReferencesTo(f.getEntryPoint()));
                println("      " + refs.size() + " direct references");
                for (Reference ref : refs) {
                    Function caller = fm.getFunctionContaining(ref.getFromAddress());
                    if (caller != null) {
                        allCallers.add(caller);
                        callerIntrinsics.computeIfAbsent(caller, k -> new LinkedHashSet<>()).add(name);
                    }
                }

                // If thunk, also get the thunked function's references
                if (f.isThunk()) {
                    Function thunked = f.getThunkedFunction(false);
                    if (thunked != null) {
                        println("      Thunked to: " + thunked.getName() + " @ " + thunked.getEntryPoint());
                    }
                }
            }

            // Try getting references to symbol addresses too
            for (Address addr : symAddrs) {
                List<Reference> refs = refsToList(rm.getReferencesTo(addr));
                if (refs.size() > 0) {
                    println("    Symbol at " + addr + ": " + refs.size() + " refs");
                }
                for (Reference ref : refs) {
                    Function caller = fm.getFunctionContaining(ref.getFromAddress());
                    if (caller != null) {
                        allCallers.add(caller);
                        callerIntrinsics.computeIfAbsent(caller, k -> new LinkedHashSet<>()).add(name);
                    }
                }
            }
        }

        println("\n=== PHASE 2: Summary ===");
        println("Total unique caller functions: " + allCallers.size());
        for (Map.Entry<Function, Set<String>> entry : callerIntrinsics.entrySet()) {
            Function f = entry.getKey();
            println("  " + f.getName() + " @ " + f.getEntryPoint()
                    + " (ELF: 0x" + Long.toHexString(f.getEntryPoint().getOffset() - 0x100000) + ")"
                    + " uses: " + String.join(", ", entry.getValue()));
        }

        // PHASE 3: Find callers of these functions (L2)
        println("\n=== PHASE 3: Level-2 callers ===");
        Set<Function> level2Callers = new LinkedHashSet<>();
        for (Function func : allCallers) {
            List<Reference> refs = refsToList(rm.getReferencesTo(func.getEntryPoint()));
            for (Reference ref : refs) {
                if (ref.getReferenceType().isCall()) {
                    Function caller = fm.getFunctionContaining(ref.getFromAddress());
                    if (caller != null && !allCallers.contains(caller)) {
                        level2Callers.add(caller);
                        println("  " + func.getName() + " @ " + func.getEntryPoint()
                                + " called by " + caller.getName() + " @ " + caller.getEntryPoint()
                                + " (ELF: 0x" + Long.toHexString(caller.getEntryPoint().getOffset() - 0x100000) + ")");
                    }
                }
            }
        }

        // PHASE 4: Decompile all L1 functions
        println("\n=== PHASE 4: Decompile L1 functions ===");
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        for (Function func : allCallers) {
            println("\n--- " + func.getName() + " @ " + func.getEntryPoint()
                    + " (ELF: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000) + ") ---");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED");
            }
        }

        // PHASE 5: Decompile L2 functions
        println("\n=== PHASE 5: Decompile L2 functions (" + level2Callers.size() + " total, max 40) ===");
        int count = 0;
        for (Function func : level2Callers) {
            if (count++ >= 40) {
                println("... truncated");
                break;
            }
            println("\n--- L2: " + func.getName() + " @ " + func.getEntryPoint()
                    + " (ELF: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000) + ") ---");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED");
            }
        }

        decomp.dispose();
        println("\n=== DONE ===");
    }
}
