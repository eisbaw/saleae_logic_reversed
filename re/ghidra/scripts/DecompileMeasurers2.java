// Ghidra script: DecompileMeasurers2.java
// Phase 2: Decompile specific measurement functions by finding xrefs to
// assertion FUNCTION NAME strings (which identify exact functions).
// Also decompile the Python measurement runner functions.
//@category Saleae

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.mem.Memory;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import ghidra.program.model.symbol.ReferenceManager;

import java.util.*;

public class DecompileMeasurers2 extends GhidraScript {

    // Ghidra VAs of key assertion function-name strings.
    // These identify the exact functions that contain the assertions.
    private static final long[][] FUNC_NAME_STRINGS = {
        // Ghidra VA, category
        // AnalogMeasurer::Initialize lambda
        {0x503119L},
        // DigitalMeasurer::Initialize lambda
        {0x4906dfL},
        // MeasurementNode::MeasureRange
        {0x49dfefL},
        // MeasurementNode::ClearApi
        {0x4be0d9L},
    };

    // Additional key strings to find functions through
    private static final String[] EXTRA_STRINGS = {
        "measure_range",
        "measure_point",
        "Missing functions `measure_range` and `measure_point`, must implement at least one",
        "Missing _get_measures",
        "Missing _get_annotations",
        "init saleae.measurements.runner",
        "saleae.measurements.runner.run_measurement",
        "saleae.measurements._runner",
        "saleae.measurements.runner",
        "/supportsPointMeasurement",
        "/supportsRangeMeasurement",
        "/measureNames",
        "/measuredValues",
        "/measurementResults",
        "/measurerResults",
        "Measure value has been overwritten",
        "Annotation value has been overwritten",
        "Not set by Measurer",
        "'{}' must be a subclass of AnalogMeasurer",
        "Unsupported API type while running measurements",
        "/dependentMeasures",
        "/frequency",
        "/period",
        "/finalFrequency",
        "/initialFrequency",
        "Estimated frequency: {}",
        "mWaveformStoreApi != nullptr",
        "Running a RangeAllRequest against a range that includes multiple waveforms",
    };

    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        FunctionManager funcMgr = currentProgram.getFunctionManager();
        ReferenceManager refMgr = currentProgram.getReferenceManager();
        Memory memory = currentProgram.getMemory();

        Set<Address> decompiled = new LinkedHashSet<>();
        Map<Address, String> reasons = new LinkedHashMap<>();

        println("=== MEASUREMENT ENGINE DECOMPILATION (Phase 2) ===");
        println("");

        // Phase 1: Find xrefs to function name assertion strings
        println("--- Finding functions via assertion function-name strings ---");
        for (long[] entry : FUNC_NAME_STRINGS) {
            Address strAddr = toAddr(entry[0]);
            // Read the string at this address
            String str = readStringAt(strAddr, memory);
            println("String at " + strAddr + ": " + truncate(str, 80));

            findAndAdd(strAddr, refMgr, funcMgr, memory, decompiled, reasons, str);
        }

        // Phase 2: Find via extra strings
        println("");
        println("--- Finding functions via extra measurement strings ---");
        for (String searchStr : EXTRA_STRINGS) {
            Address strAddr = findStringInMemory(searchStr, memory);
            if (strAddr != null) {
                println("Found '" + truncate(searchStr, 50) + "' at " + strAddr);
                findAndAdd(strAddr, refMgr, funcMgr, memory, decompiled, reasons, searchStr);
            }
        }

        println("");
        println("=== Found " + decompiled.size() + " unique functions ===");
        println("");

        // Phase 3: Decompile
        println("--- Decompiling ---");
        int count = 0;
        for (Address funcAddr : decompiled) {
            Function func = funcMgr.getFunctionAt(funcAddr);
            if (func == null) {
                func = funcMgr.getFunctionContaining(funcAddr);
            }
            if (func == null) continue;

            Address entry = func.getEntryPoint();
            long elfVA = entry.getOffset() - 0x100000L;

            println("");
            println("// ============================================================");
            println("// Function: " + func.getName());
            println("// Ghidra VA: " + entry);
            println("// ELF VA: 0x" + Long.toHexString(elfVA));
            println("// Size: " + func.getBody().getNumAddresses() + " bytes");
            println("// Reason: " + reasons.getOrDefault(funcAddr, "unknown"));
            println("// ============================================================");

            try {
                DecompileResults results = decomp.decompileFunction(func, 120, monitor);
                if (results.decompileCompleted()) {
                    println(results.getDecompiledFunction().getC());
                } else {
                    println("// DECOMPILATION FAILED: " + results.getErrorMessage());
                }
            } catch (Exception e) {
                println("// ERROR: " + e.getMessage());
            }
            count++;
        }

        println("");
        println("=== Decompiled " + count + " functions (Phase 2) ===");
        decomp.dispose();
    }

    private void findAndAdd(Address strAddr, ReferenceManager refMgr, FunctionManager funcMgr,
                            Memory memory, Set<Address> decompiled, Map<Address, String> reasons,
                            String reason) {
        // Direct xrefs
        ReferenceIterator refs = refMgr.getReferencesTo(strAddr);
        while (refs.hasNext()) {
            Reference ref = refs.next();
            Function func = funcMgr.getFunctionContaining(ref.getFromAddress());
            if (func != null) {
                Address entry = func.getEntryPoint();
                if (decompiled.add(entry)) {
                    reasons.put(entry, truncate(reason, 60));
                    println("  -> " + func.getName() + " at " + entry);
                }
            }
        }

        // Pointer xrefs (search for 8-byte LE pointer in rodata)
        long addrVal = strAddr.getOffset();
        byte[] ptrBytes = new byte[8];
        for (int i = 0; i < 8; i++) {
            ptrBytes[i] = (byte) ((addrVal >> (i * 8)) & 0xFF);
        }
        try {
            Address searchStart = toAddr(0x44a580L);
            Address searchEnd = toAddr(0x182ea20L);
            Address found = memory.findBytes(searchStart, searchEnd, ptrBytes, null, true, monitor);
            int ptrCount = 0;
            while (found != null && ptrCount < 10) {
                ReferenceIterator ptrRefs = refMgr.getReferencesTo(found);
                while (ptrRefs.hasNext()) {
                    Reference ref = ptrRefs.next();
                    Function func = funcMgr.getFunctionContaining(ref.getFromAddress());
                    if (func != null) {
                        Address entry = func.getEntryPoint();
                        if (decompiled.add(entry)) {
                            reasons.put(entry, "ptr(" + truncate(reason, 50) + ")");
                            println("  -> (indirect) " + func.getName() + " at " + entry);
                        }
                    }
                }
                ptrCount++;
                found = memory.findBytes(toAddr(found.getOffset() + 8), searchEnd, ptrBytes, null, true, monitor);
            }
        } catch (Exception e) {
            // ignore
        }
    }

    private String readStringAt(Address addr, Memory memory) {
        try {
            byte[] buf = new byte[256];
            int read = memory.getBytes(addr, buf);
            int end = 0;
            while (end < read && buf[end] != 0) end++;
            return new String(buf, 0, end, "ASCII");
        } catch (Exception e) {
            return "<error>";
        }
    }

    private Address findStringInMemory(String searchStr, Memory memory) {
        try {
            byte[] searchBytes = searchStr.getBytes("ASCII");
            Address rodataStart = toAddr(0x44a580L);
            Address rodataEnd = toAddr(0x182ea20L);
            return memory.findBytes(rodataStart, rodataEnd, searchBytes, null, true, monitor);
        } catch (Exception e) {
            return null;
        }
    }

    private String truncate(String s, int maxLen) {
        if (s == null) return "";
        if (s.length() <= maxLen) return s;
        return s.substring(0, maxLen) + "...";
    }
}
