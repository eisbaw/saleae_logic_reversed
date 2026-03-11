// Decompile functions related to USB bandwidth, transfer sizing, and compression
// Searches for string references and decompiles containing functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class DecompileCompression extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Search terms to find in string data
        String[] searchTerms = {
            "hasLimitedBandwidth",
            "GetInterfaceMaximumBitRate",
            "ComputeTransferSize",
            "AdjustTransferSize",
            "ideal transfer size",
            "Transfer size is",
            "capping transfer size",
            "LowSpeedDivider",
            "USB Full Speed",
            "USB High Speed",
            "USB Super Speed",
            "Invalid USB speed",
            "DownsampleMode",
            "downsampleMode",
            "downsampleRatio",
            "AnalogDownsampleRate"
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setOptions(new DecompileOptions());

        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();
        LinkedHashSet<Function> funcsToDecompile = new LinkedHashSet<>();

        // Search through all defined data for matching strings
        for (Data data : currentProgram.getListing().getDefinedData(true)) {
            if (monitor.isCancelled()) break;

            String val = data.getDefaultValueRepresentation();
            if (val == null) continue;

            for (String term : searchTerms) {
                if (val.contains(term)) {
                    println("STRING: " + data.getAddress() + " = " + val.substring(0, Math.min(val.length(), 120)));

                    // Find all code references TO this string
                    Reference[] refs = getReferencesTo(data.getAddress());
                    for (Reference ref : refs) {
                        Address fromAddr = ref.getFromAddress();
                        Function f = fm.getFunctionContaining(fromAddr);
                        if (f != null) {
                            println("  -> referenced by " + f.getName() + " @ " + f.getEntryPoint() + " (from " + fromAddr + ")");
                            funcsToDecompile.add(f);
                        } else {
                            println("  -> referenced from " + fromAddr + " (no function)");
                        }
                    }
                    break; // Don't match multiple terms for same string
                }
            }
        }

        println("\n===== DECOMPILING " + funcsToDecompile.size() + " FUNCTIONS =====\n");

        for (Function f : funcsToDecompile) {
            if (monitor.isCancelled()) break;
            println("\n=== " + f.getName() + " @ " + f.getEntryPoint() + " (size=" + f.getBody().getNumAddresses() + ") ===");
            try {
                DecompileResults res = decomp.decompileFunction(f, 180, monitor);
                if (res.decompileCompleted()) {
                    String c = res.getDecompiledFunction().getC();
                    // Limit output to first 500 lines per function to avoid overwhelming output
                    String[] lines = c.split("\n");
                    if (lines.length > 500) {
                        StringBuilder sb = new StringBuilder();
                        for (int i = 0; i < 500; i++) {
                            sb.append(lines[i]).append("\n");
                        }
                        sb.append("... TRUNCATED (").append(lines.length).append(" total lines)");
                        println(sb.toString());
                    } else {
                        println(c);
                    }
                } else {
                    println("DECOMPILE FAILED: " + res.getErrorMessage());
                }
            } catch (Exception e) {
                println("EXCEPTION: " + e.getMessage());
            }
        }

        decomp.dispose();
        println("\nDone. Decompiled " + funcsToDecompile.size() + " functions.");
    }
}
