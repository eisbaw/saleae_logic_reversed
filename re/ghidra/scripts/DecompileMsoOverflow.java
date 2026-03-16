// Decompile functions related to overflow handling and per-block stats consumption
// TASK-0210: Find consumer of 124-byte per-block stats
// TASK-0211: Trace overflow metadata (16 bytes) interpretation
//
// Strategy:
//   1. Find callers of PrintErrors (0x1d1fcb0) to find overflow flag unpacking
//   2. Decompile functions that reference the "cur_overflow" / "prev_overflow" strings
//   3. Find callers/callees of HandleWaveformData to trace stats consumption
//   4. Look for references to "offset_into_stats_bytes" assertion
//   5. Find the function at ELF 0x2d03a82 (assertion about mAnalogStats)
//
//@category RE

import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;

import java.io.PrintWriter;
import java.util.LinkedHashSet;
import java.util.Set;

public class DecompileMsoOverflow extends GhidraScript {
    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();

        Set<Function> functions = new LinkedHashSet<>();

        // === Direct addresses to decompile ===
        // Ghidra VA = ELF VA + 0x100000

        // 1. PrintErrors caller: find who calls 0x1d1fcb0
        //    This will show the overflow flag unpacking logic
        Address printErrorsAddr = af.getAddress("0x1d1fcb0");
        Function printErrors = fm.getFunctionContaining(printErrorsAddr);
        if (printErrors != null) {
            functions.add(printErrors);
            println("PrintErrors: " + printErrors.getEntryPoint());

            // Find callers of PrintErrors
            ReferenceIterator refs = rm.getReferencesTo(printErrors.getEntryPoint());
            int callerCount = 0;
            while (refs.hasNext() && callerCount < 10) {
                Reference ref = refs.next();
                if (ref.getReferenceType().isCall()) {
                    Function caller = fm.getFunctionContaining(ref.getFromAddress());
                    if (caller != null) {
                        functions.add(caller);
                        println("PrintErrors caller: " + caller.getName() + " @ " + caller.getEntryPoint());
                        callerCount++;
                    }
                }
            }
        }

        // 2. Function containing assertion about mAnalogStats
        //    ELF 0x2d03a82 -> Ghidra 0x2e03a82
        Address mAnalogStatsRef = af.getAddress("0x2e03a82");
        Function mAnalogStatsFunc = fm.getFunctionContaining(mAnalogStatsRef);
        if (mAnalogStatsFunc != null) {
            functions.add(mAnalogStatsFunc);
            println("mAnalogStats function: " + mAnalogStatsFunc.getName() + " @ " + mAnalogStatsFunc.getEntryPoint());
        }

        // 3. Search for string references related to overflow/stats consumption
        long[] stringAddrs = {
            // "offset_into_stats_bytes < data.mAnalogStats.size_bytes()"
            0x4df6f5L,
            // "data.mAnalogStats.size() > 0"
            0x4e665bL,
            // "WaveformIterator::GetStats PRETTY_FUNCTION"
            0x4df72eL,
            // "ChunkIterator::GetStats PRETTY_FUNCTION"
            0x496ae8L,
            // "PackedWaveformGetStats PRETTY_FUNCTION"
            0x488b96L,
            // "cur_overflow={}, prev_overflow={}" is part of the big format string
            // at HandleWaveformData; already decompiled. But look for other refs.
        };

        for (long addr : stringAddrs) {
            Address strAddr = toAddr(addr);
            ReferenceIterator refs = rm.getReferencesTo(strAddr);
            while (refs.hasNext()) {
                Reference ref = refs.next();
                Function func = fm.getFunctionContaining(ref.getFromAddress());
                if (func != null) {
                    functions.add(func);
                    println("Stats string ref func: " + func.getName() + " @ " + func.getEntryPoint() +
                            " (from string at 0x" + Long.toHexString(addr) + ")");
                }
            }
        }

        // 4. Find the function that processes EP2 stats in the Logic device path
        //    (DisaggregatorNode or similar). Look for string "Disaggregat"
        long[] disaggStrAddrs = {
            // Search for disaggregator-related strings in the binary
            // Let's look for the assertion about stats from the withstats doc
        };

        // 5. Also decompile HandleChunkStats / HandleOverflow if they exist
        //    Search for functions that reference 0x7c (124) as a multiplier
        //    or functions near HandleWaveformData

        // 6. Decompile the function that reads the 16-byte overflow metadata
        //    ParseMsoWaveformData saves overflow_entry_size=8 at struct[0x20]
        //    and stream position at struct[0x24]. Find who reads struct[0x20].

        // Write output
        String outPath = "/home/mpedersen/nvidia/saleae_reverse/re/ghidra/decompiled-mso-overflow.txt";
        PrintWriter pw = new PrintWriter(outPath);

        pw.println("// ================================================================");
        pw.println("// Decompiled overflow handling and per-block stats functions");
        pw.println("// TASK-0210: Per-block stats (124 bytes) consumer");
        pw.println("// TASK-0211: Overflow metadata (16 bytes) interpretation");
        pw.println("// Saleae Logic 2 v2.4.40 - libgraph_server_shared.so");
        pw.println("// ================================================================");
        pw.println();

        int count = 0;
        for (Function func : functions) {
            long elfVA = func.getEntryPoint().getOffset() - 0x100000L;
            pw.println("// ----------------------------------------------------------------");
            pw.println("// Function at Ghidra VA 0x" + Long.toHexString(func.getEntryPoint().getOffset()));
            pw.println("// ELF VA: 0x" + Long.toHexString(elfVA));
            pw.println("// Name: " + func.getName());
            pw.println("// Size: " + func.getBody().getNumAddresses() + " bytes");
            pw.println("// ----------------------------------------------------------------");

            DecompileResults results = decomp.decompileFunction(func, 180, monitor);
            if (results.decompileCompleted()) {
                String code = results.getDecompiledFunction().getC();
                pw.println(code);
                // Print summary to console
                String[] lines = code.split("\n");
                for (int i = 0; i < Math.min(30, lines.length); i++) {
                    println(lines[i]);
                }
                if (lines.length > 30) {
                    println("... (" + (lines.length - 30) + " more lines)");
                }
            } else {
                pw.println("// DECOMPILE FAILED: " + results.getErrorMessage());
                println("DECOMPILE FAILED for " + func.getName() + ": " + results.getErrorMessage());
            }
            pw.println();
            count++;
        }

        pw.close();
        decomp.dispose();
        println("\nDecompiled " + count + " functions to: " + outPath);
    }
}
