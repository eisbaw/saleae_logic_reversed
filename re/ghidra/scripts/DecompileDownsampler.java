// Decompile analog downsampler functions by finding string xrefs
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.mem.*;
import java.util.*;

public class DecompileDownsampler extends GhidraScript {
    @Override
    public void run() throws Exception {
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager refMgr = currentProgram.getReferenceManager();
        Listing listing = currentProgram.getListing();

        // Search terms for finding relevant strings
        String[] searchTerms = {
            "downsampler", "downsample", "decimate",
            "DownsampleMode", "DownsampleRatio",
            "analog_downsampler_node",
            "waveform_summary_node",
            "WaveformSummary"
        };

        Set<Function> funcsToDecompile = new LinkedHashSet<>();

        println("=== PHASE 1: Finding relevant strings ===\n");

        DataIterator dataIter = listing.getDefinedData(true);
        while (dataIter.hasNext() && !monitor.isCancelled()) {
            Data data = dataIter.next();
            if (data.getValueClass() != null && String.class.isAssignableFrom(data.getValueClass())) {
                Object val = data.getValue();
                if (val == null) continue;
                String strVal = val.toString();

                boolean matches = false;
                for (String term : searchTerms) {
                    if (strVal.toLowerCase().contains(term.toLowerCase())) {
                        matches = true;
                        break;
                    }
                }
                if (!matches) continue;

                // Truncate for display
                String display = strVal.length() > 120 ? strVal.substring(0, 120) + "..." : strVal;
                println("STRING @ " + data.getAddress() + ": " + display);

                // Follow xrefs
                Reference[] refs = getReferencesTo(data.getAddress());
                for (Reference ref : refs) {
                    Address fromAddr = ref.getFromAddress();
                    Function f = fm.getFunctionContaining(fromAddr);
                    if (f != null) {
                        funcsToDecompile.add(f);
                        println("  -> XREF from " + f.getName() + " @ " + f.getEntryPoint());
                    } else {
                        // Might be in a vtable or data pointer -- follow one more level
                        Reference[] refs2 = getReferencesTo(fromAddr);
                        for (Reference ref2 : refs2) {
                            Function f2 = fm.getFunctionContaining(ref2.getFromAddress());
                            if (f2 != null) {
                                funcsToDecompile.add(f2);
                                println("  -> XREF (indirect) from " + f2.getName() + " @ " + f2.getEntryPoint());
                            }
                        }
                    }
                }
            }
        }

        println("\n=== PHASE 2: Decompiling " + funcsToDecompile.size() + " functions ===\n");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        // Set max payload size for large functions
        decomp.setOptions(new DecompileOptions());

        int count = 0;
        for (Function f : funcsToDecompile) {
            if (monitor.isCancelled()) break;
            count++;
            println("\n========================================");
            println("FUNCTION " + count + "/" + funcsToDecompile.size() +
                    ": " + f.getName() + " @ " + f.getEntryPoint());
            println("Size: " + f.getBody().getNumAddresses() + " bytes");
            println("========================================\n");

            DecompileResults res = decomp.decompileFunction(f, 180, monitor);
            if (res.decompileCompleted()) {
                String code = res.getDecompiledFunction().getC();
                println(code);
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
        }

        decomp.dispose();
        println("\n=== DONE: Decompiled " + count + " functions ===");
    }
}
