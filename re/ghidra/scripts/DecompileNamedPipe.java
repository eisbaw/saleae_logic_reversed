// Decompile functions related to NamedPipe / NamedPipeUnix classes
// Finds xrefs to key strings and decompiles referencing functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;

import java.util.*;

public class DecompileNamedPipe extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Key string addresses (Ghidra VA = ELF file offset + 0x100000 for .rodata)
        // ELF file offsets from `strings -t x`:
        //   0x35d881 "unknown error occured while trying to open named pipe for writing: "
        //   0x369902 "Failed to create named pipe. Terminal mode will be disabled..."
        //   0x36ab45 "failed to create named pipe"
        //   0x36ab61 "unable to close named pipe handle, error: "
        //   0x371a82 "/workspace/monorepo/graph-io/core/named_pipe/src/named_pipe_unix.cpp"
        //   0x3a6893 "~NamedPipeUnix"
        //   0x3abf61 "/pipe"
        //   0x3b7827 "named pipe connected"
        //   0x3d9a18 "named pipe not connected"
        //   0x3e03bf "/namedPipeIdentifier"
        //   0x42875b "unable to delete named pipe, error: "
        //   0x43a936 "A pipe can only have one reader"
        //   0x43caa4 "A pipe in mConnectedOutputs should always exist in mExternalPipeDock"

        String[][] stringAddrs = new String[][] {
            {"0x45d881", "unknown error opening named pipe for writing"},
            {"0x469902", "Failed to create named pipe - terminal mode disabled"},
            {"0x46ab45", "failed to create named pipe"},
            {"0x46ab61", "unable to close named pipe handle"},
            {"0x471a82", "named_pipe_unix.cpp source path"},
            {"0x4a6893", "~NamedPipeUnix destructor string"},
            {"0x4abf61", "/pipe path component"},
            {"0x4b7827", "named pipe connected"},
            {"0x4d9a18", "named pipe not connected"},
            {"0x4e03bf", "/namedPipeIdentifier"},
            {"0x52875b", "unable to delete named pipe"},
            {"0x53a936", "A pipe can only have one reader"},
            {"0x53caa4", "A pipe in mConnectedOutputs should always exist in mExternalPipeDock"},
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();

        Set<Address> decompiled = new HashSet<>();

        for (String[] entry : stringAddrs) {
            String addrStr = entry[0];
            String desc = entry[1];
            Address strAddr = af.getAddress(addrStr);

            println("=== Finding xrefs to string: " + desc + " @ " + addrStr + " ===");

            // Get all references TO this string address
            List<Reference> refs = new ArrayList<>();
            for (Reference r : rm.getReferencesTo(strAddr)) {
                refs.add(r);
            }
            if (refs.isEmpty()) {
                // Try scanning nearby (sometimes strings get split or misaligned)
                for (int offset = -8; offset <= 8; offset++) {
                    if (offset == 0) continue;
                    Address nearby = strAddr.add(offset);
                    for (Reference r : rm.getReferencesTo(nearby)) {
                        refs.add(r);
                    }
                    if (!refs.isEmpty()) {
                        println("  Found " + refs.size() + " refs at offset " + offset + " (" + nearby + ")");
                        break;
                    }
                }
            }

            println("  Found " + refs.size() + " xrefs");

            for (Reference ref : refs) {
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func == null) {
                    println("  Xref from " + fromAddr + " -- no containing function");
                    continue;
                }
                if (decompiled.contains(func.getEntryPoint())) {
                    println("  Xref from " + fromAddr + " in " + func.getName() + " -- already decompiled");
                    continue;
                }
                decompiled.add(func.getEntryPoint());

                long elfVA = func.getEntryPoint().getOffset() - 0x100000;
                println("\n--- Decompiling: " + func.getName() + " @ " + func.getEntryPoint()
                        + " (ELF VA: 0x" + Long.toHexString(elfVA) + ") ---");
                println("  Referenced string: " + desc);

                DecompileResults res = decomp.decompileFunction(func, 120, monitor);
                if (res.decompileCompleted()) {
                    println(res.getDecompiledFunction().getC());
                } else {
                    println("DECOMPILE FAILED: " + res.getErrorMessage());
                }
                println("");
            }
        }

        decomp.dispose();
        println("=== Total unique functions decompiled: " + decompiled.size() + " ===");
    }
}
