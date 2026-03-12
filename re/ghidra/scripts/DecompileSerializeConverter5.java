// Find and decompile functions that reference key JSON field names
// These are the actual serialization functions that read/write JSON fields
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;

import java.util.*;

public class DecompileSerializeConverter5 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // JSON Pointer field name strings (ELF VA + 0x100000 = Ghidra VA)
        String[][] fieldStrings = new String[][] {
            {"0x470634", "/sampleRate"},
            {"0x489808", "/digitalThreshold"},
            {"0x490a60", "/led"},
            {"0x4c4b40", "/digitalChannels"},
            {"0x4e0373", "/analogChannels"},
            {"0x4e743c", "/fullScaleGain"},
            {"0x4ee85d", "/enabledChannels"},
            {"0x4fbbaa", "/analogSamplingConfiguration"},
            {"0x50a76f", "/calibration"},
            {"0x526bf4", "/analogSampleRate"},
            {"0x462889", "/digitalSettings"},
            {"0x4fbb9a", "/analogSettings"},
        };

        DecompInterface decomp = new DecompInterface();
        DecompileOptions opts = new DecompileOptions();
        decomp.setOptions(opts);
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();

        Set<Address> decompiled = new HashSet<>();

        println("// ================================================================");
        println("// Functions that use JSON Pointer field names");
        println("// These contain the actual serialization/deserialization logic");
        println("// ================================================================");

        for (String[] entry : fieldStrings) {
            String addrStr = entry[0];
            String fieldName = entry[1];

            Address strAddr = af.getAddress(addrStr);
            if (strAddr == null) continue;

            println("");
            println("// ================================================================");
            println("// Field: " + fieldName + " at " + addrStr);
            println("// ================================================================");

            // Find references within +/- 4 bytes
            List<Reference> refs = new ArrayList<>();
            for (int offset = -4; offset <= 4; offset++) {
                Address nearby = strAddr.add(offset);
                for (Reference r : rm.getReferencesTo(nearby)) {
                    refs.add(r);
                }
            }

            if (refs.isEmpty()) {
                println("// No xrefs found");
                continue;
            }

            println("// Found " + refs.size() + " xrefs");

            // Decompile each referencing function, but cap at 3 per field
            int count = 0;
            for (Reference ref : refs) {
                if (count >= 3) break;
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func == null) continue;
                if (decompiled.contains(func.getEntryPoint())) {
                    println("// (already decompiled: " + func.getName() + ")");
                    continue;
                }

                long size = func.getBody().getNumAddresses();
                // Only decompile functions > 100 bytes (skip trivial wrappers)
                if (size < 100) {
                    println("// Skipping small function " + func.getName() + " (" + size + " bytes)");
                    continue;
                }
                // Skip extremely large functions (>10K) to avoid timeout
                if (size > 10000) {
                    println("// Skipping very large function " + func.getName() + " (" + size + " bytes)");
                    continue;
                }

                decompiled.add(func.getEntryPoint());
                count++;

                long ghidraVA = func.getEntryPoint().getOffset();
                long elfVA = ghidraVA - 0x100000;

                println("");
                println("// Function: " + func.getName());
                println("// Ghidra VA: 0x" + Long.toHexString(ghidraVA));
                println("// ELF VA: 0x" + Long.toHexString(elfVA));
                println("// Size: " + size + " bytes");
                println("// Referenced field: " + fieldName);

                DecompileResults res = decomp.decompileFunction(func, 60, monitor);
                if (res.decompileCompleted()) {
                    println(res.getDecompiledFunction().getC());
                } else {
                    println("// DECOMPILE FAILED: " + res.getErrorMessage());
                }
            }
        }

        decomp.dispose();
        println("// === END ===");
    }
}
