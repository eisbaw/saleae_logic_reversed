// Ghidra script: DecompileMeasurers.java
// Find and decompile all measurement engine functions by tracing
// xrefs to assertion source-path strings and key measurement strings.
//
// Output goes to stdout (captured by headless runner).
//@category Saleae

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.mem.MemoryAccessException;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import ghidra.program.model.symbol.ReferenceManager;

import java.util.*;

public class DecompileMeasurers extends GhidraScript {

    // Ghidra rebases by +0x100000
    // These are Ghidra VAs for the assertion source path strings
    private static final long[] ASSERTION_STRING_GHIDRA_VAS = {
        0x482524L,  // analog_measurer.cpp
        0x4f502cL,  // digital_measurer.cpp
        0x5272cfL,  // range_measurer.cpp
        0x4d1c3cL,  // measurement_node.cpp
        0x4ef819L,  // measurer.cpp (core)
    };

    // Key measurement-related strings to search for (Ghidra VAs will be found dynamically)
    private static final String[] KEY_STRINGS = {
        "Analog store API must be set before measurements can be processed",
        "Digital store API must be set before measurements can be processed",
        "RangeMeasurer requires either an AnalogStoreApi or a DigitalStoreApi",
        "Measurer is in an error state",
        "Missing functions `measure_range` and `measure_point`, must implement at least one",
        "Unexpected measurer type",
        "starting measurement",
        "finished measurement",
        "Error while loading measurement",
        "Error while running measurement",
        "RunMeasurements",
        "return value of measurer should be a dictionary with string keys and numeric values",
        "Saleae::Graph::AnalogMeasurer",
        "Saleae::Graph::DigitalMeasurer",
        "Saleae::Graph::RangeMeasurer",
        "Saleae::Graph::MeasurementNode",
    };

    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        FunctionManager funcMgr = currentProgram.getFunctionManager();
        ReferenceManager refMgr = currentProgram.getReferenceManager();
        Memory memory = currentProgram.getMemory();

        Set<Address> functionsToDecompile = new LinkedHashSet<>();
        Map<Address, String> functionReasons = new HashMap<>();

        println("=== MEASUREMENT ENGINE DECOMPILATION ===");
        println("");

        // 1. Find functions referencing assertion source path strings
        println("--- Phase 1: Finding functions via assertion string xrefs ---");
        String[] sourceFiles = {
            "analog_measurer.cpp",
            "digital_measurer.cpp",
            "range_measurer.cpp",
            "measurement_node.cpp",
            "measurer.cpp (core)",
        };

        for (int i = 0; i < ASSERTION_STRING_GHIDRA_VAS.length; i++) {
            Address strAddr = toAddr(ASSERTION_STRING_GHIDRA_VAS[i]);
            println("Searching xrefs to " + sourceFiles[i] + " at " + strAddr);

            // Get all references to addresses in the range of this string
            // The assertion references the string start, so check xrefs to
            // the start and also nearby addresses (assertion structs often
            // reference the string via a pointer in .rodata)
            Set<Address> found = findXrefsToString(strAddr, refMgr, funcMgr, memory);
            for (Address funcAddr : found) {
                functionsToDecompile.add(funcAddr);
                String prev = functionReasons.getOrDefault(funcAddr, "");
                functionReasons.put(funcAddr, prev + sourceFiles[i] + " ");
            }
        }

        // 2. Search for key measurement strings in memory and find their xrefs
        println("");
        println("--- Phase 2: Finding functions via key measurement strings ---");
        for (String searchStr : KEY_STRINGS) {
            Address strAddr = findStringInMemory(searchStr, memory);
            if (strAddr != null) {
                println("Found '" + searchStr + "' at " + strAddr);
                Set<Address> found = findXrefsToString(strAddr, refMgr, funcMgr, memory);
                for (Address funcAddr : found) {
                    functionsToDecompile.add(funcAddr);
                    String prev = functionReasons.getOrDefault(funcAddr, "");
                    functionReasons.put(funcAddr, prev + "\"" + truncate(searchStr, 40) + "\" ");
                }
            } else {
                println("NOT FOUND: '" + searchStr + "'");
            }
        }

        println("");
        println("=== Found " + functionsToDecompile.size() + " unique functions to decompile ===");
        println("");

        // 3. Decompile all found functions
        println("--- Phase 3: Decompiling functions ---");
        int count = 0;
        for (Address funcAddr : functionsToDecompile) {
            Function func = funcMgr.getFunctionContaining(funcAddr);
            if (func == null) {
                // Try to create a function here
                func = funcMgr.getFunctionAt(funcAddr);
            }
            if (func == null) {
                println("// WARNING: No function at " + funcAddr + " (reason: " + functionReasons.get(funcAddr) + ")");
                continue;
            }

            Address entryPoint = func.getEntryPoint();
            // Convert Ghidra VA to ELF VA for documentation
            long elfVA = entryPoint.getOffset() - 0x100000L;

            println("");
            println("// ============================================================");
            println("// Function: " + func.getName());
            println("// Ghidra VA: " + entryPoint);
            println("// ELF VA: 0x" + Long.toHexString(elfVA));
            println("// Size: " + func.getBody().getNumAddresses() + " bytes");
            println("// Referenced by: " + functionReasons.getOrDefault(funcAddr, "unknown"));
            println("// ============================================================");

            try {
                DecompileResults results = decomp.decompileFunction(func, 120, monitor);
                if (results.decompileCompleted()) {
                    String code = results.getDecompiledFunction().getC();
                    println(code);
                } else {
                    println("// DECOMPILATION FAILED: " + results.getErrorMessage());
                }
            } catch (Exception e) {
                println("// DECOMPILATION ERROR: " + e.getMessage());
            }
            count++;
        }

        println("");
        println("=== Decompiled " + count + " functions total ===");

        decomp.dispose();
    }

    private Set<Address> findXrefsToString(Address strAddr, ReferenceManager refMgr,
                                            FunctionManager funcMgr, Memory memory) {
        Set<Address> result = new LinkedHashSet<>();

        // Direct xrefs to the string address
        ReferenceIterator refs = refMgr.getReferencesTo(strAddr);
        while (refs.hasNext()) {
            Reference ref = refs.next();
            Address fromAddr = ref.getFromAddress();
            Function func = funcMgr.getFunctionContaining(fromAddr);
            if (func != null) {
                result.add(func.getEntryPoint());
                println("  xref from " + func.getName() + " at " + fromAddr);
            }
        }

        // Also check for pointer references (rodata often has pointers to strings
        // that are then referenced by code)
        // Search for 8-byte LE pointer value in .rodata
        long strAddrLong = strAddr.getOffset();
        byte[] ptrBytes = new byte[8];
        for (int i = 0; i < 8; i++) {
            ptrBytes[i] = (byte) ((strAddrLong >> (i * 8)) & 0xFF);
        }

        // Search in a reasonable range
        try {
            Address searchStart = toAddr(0x34a580L + 0x100000L); // .rodata start (Ghidra VA)
            Address searchEnd = toAddr(0x172ea20L + 0x100000L);  // .rodata end approx
            Address found = memory.findBytes(searchStart, searchEnd, ptrBytes, null, true, monitor);
            int ptrCount = 0;
            while (found != null && ptrCount < 20) {
                // This address contains a pointer to our string
                // Find xrefs to THIS address
                ReferenceIterator ptrRefs = refMgr.getReferencesTo(found);
                while (ptrRefs.hasNext()) {
                    Reference ref = ptrRefs.next();
                    Address fromAddr = ref.getFromAddress();
                    Function func = funcMgr.getFunctionContaining(fromAddr);
                    if (func != null) {
                        result.add(func.getEntryPoint());
                        println("  indirect xref via " + found + " from " + func.getName() + " at " + fromAddr);
                    }
                }
                ptrCount++;
                found = memory.findBytes(toAddr(found.getOffset() + 8), searchEnd, ptrBytes, null, true, monitor);
            }
        } catch (Exception e) {
            // Ignore search errors
        }

        return result;
    }

    private Address findStringInMemory(String searchStr, Memory memory) {
        try {
            byte[] searchBytes = searchStr.getBytes("ASCII");
            Address rodataStart = toAddr(0x34a580L + 0x100000L);
            Address rodataEnd = toAddr(0x172ea20L + 0x100000L);
            Address found = memory.findBytes(rodataStart, rodataEnd, searchBytes, null, true, monitor);
            return found;
        } catch (Exception e) {
            return null;
        }
    }

    private String truncate(String s, int maxLen) {
        if (s.length() <= maxLen) return s;
        return s.substring(0, maxLen) + "...";
    }
}
