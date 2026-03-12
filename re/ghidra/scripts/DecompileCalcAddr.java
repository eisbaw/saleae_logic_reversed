// Decompile CalculateDigitalConfigurationAddress_* functions and search
// for other Calculate* address functions we might be missing.
// Addresses are Ghidra-rebased (ELF VA + 0x100000)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import ghidra.program.util.DefinedDataIterator;
import ghidra.program.model.data.*;

import java.util.*;

public class DecompileCalcAddr extends GhidraScript {

    private Address findString(String target) {
        for (Data data : DefinedDataIterator.definedStrings(currentProgram)) {
            Object val = data.getValue();
            if (val != null && val.toString().contains(target)) {
                return data.getAddress();
            }
        }
        return null;
    }

    private List<Address> getCodeXrefs(Address dataAddr) {
        List<Address> results = new ArrayList<>();
        ReferenceManager rm = currentProgram.getReferenceManager();
        for (Reference ref : rm.getReferencesTo(dataAddr)) {
            results.add(ref.getFromAddress());
        }
        return results;
    }

    @Override
    public void run() throws Exception {
        println("=== DecompileCalcAddr: Focused decompilation of Calculate*Address functions ===");
        println("");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        FunctionManager fm = currentProgram.getFunctionManager();
        AddressFactory af = currentProgram.getAddressFactory();

        // =====================================================================
        // Part 1: Decompile at known ELF VAs (rebased +0x100000)
        // =====================================================================
        // DigitalDerived: ELF VA 0x2d66870 -> Ghidra 0x2e66870
        // SmartCable:     ELF VA 0x2d66380 -> Ghidra 0x2e66380
        String[][] knownFuncs = {
            {"02e66870", "CalculateDigitalConfigurationAddress_DigitalDerived", "0x2d66870"},
            {"02e66380", "CalculateDigitalConfigurationAddress_SmartCable", "0x2d66380"},
        };

        println("--- Part 1: Decompiling at known addresses ---");
        for (String[] entry : knownFuncs) {
            Address addr = af.getAddress(entry[0]);
            if (addr == null) {
                println("ADDR PARSE FAILED: " + entry[0]);
                continue;
            }
            Function func = fm.getFunctionAt(addr);
            if (func == null) {
                func = fm.getFunctionContaining(addr);
            }
            if (func == null) {
                println("NO FUNCTION at Ghidra addr " + entry[0] + " (ELF VA " + entry[2] + ") for " + entry[1]);
                continue;
            }

            println("=== " + entry[1] + " @ " + func.getEntryPoint()
                + " (ELF VA " + entry[2] + ", size: " + func.getBody().getNumAddresses() + " bytes) ===");

            DecompileResults res = decomp.decompileFunction(func, 300, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("");
        }

        // =====================================================================
        // Part 2: Search for assertion strings containing "Calculate"
        // =====================================================================
        println("--- Part 2: Searching for ALL 'Calculate' assertion strings ---");
        int calcCount = 0;
        for (Data data : DefinedDataIterator.definedStrings(currentProgram)) {
            Object val = data.getValue();
            if (val != null && val.toString().contains("Calculate")) {
                println("  STRING: \"" + val.toString() + "\" @ " + data.getAddress());
                calcCount++;

                // Find xrefs to this string
                List<Address> xrefs = getCodeXrefs(data.getAddress());
                for (Address xref : xrefs) {
                    Function func = fm.getFunctionContaining(xref);
                    if (func != null) {
                        println("    -> used in: " + func.getName() + " @ " + func.getEntryPoint());
                    }
                }
            }
        }
        println("  Total 'Calculate' strings found: " + calcCount);
        println("");

        // =====================================================================
        // Part 3: Search for assertion strings containing "ConfigurationAddress"
        // =====================================================================
        println("--- Part 3: Searching for ALL 'ConfigurationAddress' strings ---");
        int configAddrCount = 0;
        for (Data data : DefinedDataIterator.definedStrings(currentProgram)) {
            Object val = data.getValue();
            if (val != null && val.toString().contains("ConfigurationAddress")) {
                println("  STRING: \"" + val.toString() + "\" @ " + data.getAddress());
                configAddrCount++;

                List<Address> xrefs = getCodeXrefs(data.getAddress());
                for (Address xref : xrefs) {
                    Function func = fm.getFunctionContaining(xref);
                    if (func != null && !func.getName().startsWith("FUN_")) {
                        println("    -> used in: " + func.getName() + " @ " + func.getEntryPoint());
                    } else if (func != null) {
                        // Decompile unnamed functions that reference ConfigurationAddress
                        println("    -> used in UNNAMED: " + func.getName() + " @ " + func.getEntryPoint());
                        DecompileResults res = decomp.decompileFunction(func, 300, monitor);
                        if (res.decompileCompleted()) {
                            println(res.getDecompiledFunction().getC());
                        }
                    }
                }
            }
        }
        println("  Total 'ConfigurationAddress' strings found: " + configAddrCount);

        // =====================================================================
        // Part 4: Search for "CalculateAnalog" or other address variants
        // =====================================================================
        println("");
        println("--- Part 4: Searching for 'CalculateAnalog' strings ---");
        for (Data data : DefinedDataIterator.definedStrings(currentProgram)) {
            Object val = data.getValue();
            if (val != null && val.toString().contains("CalculateAnalog")) {
                println("  STRING: \"" + val.toString() + "\" @ " + data.getAddress());
            }
        }

        println("");
        println("--- Part 5: Searching for 'CalculateScope' strings ---");
        for (Data data : DefinedDataIterator.definedStrings(currentProgram)) {
            Object val = data.getValue();
            if (val != null && val.toString().contains("CalculateScope")) {
                println("  STRING: \"" + val.toString() + "\" @ " + data.getAddress());
            }
        }

        decomp.dispose();
        println("");
        println("=== DecompileCalcAddr complete ===");
    }
}
