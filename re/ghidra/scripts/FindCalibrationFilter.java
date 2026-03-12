// Find and decompile AnalogCalibrationFilter-related functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.data.*;
import ghidra.program.model.mem.*;

import java.util.*;

public class FindCalibrationFilter extends GhidraScript {
    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();

        // Step 1: Find all strings containing "calibrat" (case insensitive)
        println("=== STEP 1: Finding calibration-related strings ===");
        SymbolTable st = currentProgram.getSymbolTable();

        // Search for defined strings containing calibration keywords
        DataIterator di = currentProgram.getListing().getDefinedData(true);
        List<Address> calibStrings = new ArrayList<>();
        while (di.hasNext() && !monitor.isCancelled()) {
            Data d = di.next();
            if (d.getDataType() instanceof StringDataType ||
                d.getDataType().getName().contains("string") ||
                d.getDataType().getName().contains("char")) {
                Object val = d.getValue();
                if (val != null) {
                    String s = val.toString().toLowerCase();
                    if (s.contains("calibrat") || s.contains("df1sos") ||
                        s.contains("simdiir") || s.contains("simdprototype") ||
                        s.contains("biquad") || s.contains("2by3")) {
                        println("  STRING @ " + d.getAddress() + ": " + val.toString());
                        calibStrings.add(d.getAddress());
                    }
                }
            }
        }

        // Step 2: Find xrefs to these strings
        println("\n=== STEP 2: Finding xrefs to calibration strings ===");
        Set<Function> referencingFuncs = new LinkedHashSet<>();
        for (Address strAddr : calibStrings) {
            Reference[] refs = getReferencesTo(strAddr);
            for (Reference ref : refs) {
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func != null) {
                    println("  XREF from " + func.getName() + " @ " + func.getEntryPoint() +
                            " -> string @ " + strAddr);
                    referencingFuncs.add(func);
                } else {
                    println("  XREF from " + fromAddr + " (no function) -> string @ " + strAddr);
                }
            }
        }

        // Step 3: Also search for typeinfo/vtable symbols
        println("\n=== STEP 3: Finding AnalogCalibrationFilter symbols ===");
        SymbolIterator si = st.getAllSymbols(true);
        List<Function> namedFuncs = new ArrayList<>();
        while (si.hasNext() && !monitor.isCancelled()) {
            Symbol sym = si.next();
            String name = sym.getName();
            if (name.contains("AnalogCalibrationFilter") ||
                name.contains("SimdIirFilter") ||
                name.contains("SimdPrototype_Static2By3")) {
                Address symAddr = sym.getAddress();
                println("  SYMBOL: " + name + " @ " + symAddr + " type=" + sym.getSymbolType());
                Function func = fm.getFunctionAt(symAddr);
                if (func != null) {
                    namedFuncs.add(func);
                } else {
                    func = fm.getFunctionContaining(symAddr);
                    if (func != null) {
                        println("    (inside function " + func.getName() + " @ " + func.getEntryPoint() + ")");
                        namedFuncs.add(func);
                    }
                }
            }
        }

        // Step 4: Decompile all found functions
        println("\n=== STEP 4: Decompiling found functions ===");
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        Set<Function> allFuncs = new LinkedHashSet<>();
        allFuncs.addAll(referencingFuncs);
        allFuncs.addAll(namedFuncs);

        for (Function func : allFuncs) {
            if (monitor.isCancelled()) break;
            println("\n=== FUNCTION: " + func.getName() + " @ " + func.getEntryPoint() +
                    " (size=" + func.getBody().getNumAddresses() + ") ===");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                String c = res.getDecompiledFunction().getC();
                // Print full decompilation
                println(c);
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
        }

        decomp.dispose();
    }
}
