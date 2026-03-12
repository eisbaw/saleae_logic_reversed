// Decompile FpgaDeviceFeatures register operations by finding xrefs to known strings.
// @category Saleae
// @author Claude

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

import java.io.File;
import java.io.PrintWriter;
import java.util.LinkedHashSet;
import java.util.Set;

public class DecompileRegisterOps extends GhidraScript {

    // Strings found in the binary (ELF VA + Ghidra rebase of 0x100000)
    // ELF VA 0x369d38 -> Ghidra 0x469d38 (WriteRegisters sig)
    // ELF VA 0x3b7791 -> Ghidra 0x4b7791 (ReadRegisters sig)
    // ELF VA 0x42e3d4 -> Ghidra 0x52e3d4 (WriteRegisterBit sig)
    // ELF VA 0x3d1e4a -> Ghidra 0x4d1e4a (num_registers < 32)

    // Also search for these shorter strings
    private static final String[] SEARCH_STRINGS = {
        "WriteRegisters",
        "ReadRegisters",
        "WriteRegisterBit",
        "num_registers < 32",
        "WriteConfigurationData response",
        "WriteConfigurationData does not support lengths larger than 62/509",
    };

    // Known string addresses (ELF VA + 0x100000 rebase)
    private static final long[] STRING_ADDRS = {
        0x469d38L, // WriteRegisters signature
        0x4b7791L, // ReadRegisters signature
        0x52e3d4L, // WriteRegisterBit signature
        0x4d1e4aL, // num_registers < 32
    };

    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        FunctionManager funcManager = currentProgram.getFunctionManager();
        Set<Function> targetFunctions = new LinkedHashSet<>();

        // Method 1: Find xrefs from known string addresses
        for (long addrVal : STRING_ADDRS) {
            Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(addrVal);
            ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(addr);
            while (refs.hasNext()) {
                Reference ref = refs.next();
                Address fromAddr = ref.getFromAddress();
                Function func = funcManager.getFunctionContaining(fromAddr);
                if (func != null) {
                    targetFunctions.add(func);
                    printf("Found function %s at %s (xref from string at %s)\n",
                        func.getName(), func.getEntryPoint(), addr);
                }
            }
        }

        // Method 2: Search for string references by content
        // (in case rebase is different than expected)
        for (String searchStr : SEARCH_STRINGS) {
            Address addr = currentProgram.getMinAddress();
            while (addr != null) {
                addr = find(addr, searchStr.getBytes());
                if (addr == null) break;

                // Find xrefs to this string
                ReferenceIterator refs = currentProgram.getReferenceManager().getReferencesTo(addr);
                while (refs.hasNext()) {
                    Reference ref = refs.next();
                    Address fromAddr = ref.getFromAddress();
                    Function func = funcManager.getFunctionContaining(fromAddr);
                    if (func != null) {
                        targetFunctions.add(func);
                        printf("Found function %s at %s (xref to '%s' at %s)\n",
                            func.getName(), func.getEntryPoint(), searchStr, addr);
                    }
                }
                addr = addr.add(1);
            }
        }

        // Also find WriteConfigurationData function
        // ELF VA from task: 0x1c544b0 -> Ghidra 0x1d544b0
        Address wcAddr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(0x1d544b0L);
        Function wcFunc = funcManager.getFunctionContaining(wcAddr);
        if (wcFunc != null) {
            targetFunctions.add(wcFunc);
            printf("Found WriteConfigurationData at %s\n", wcFunc.getEntryPoint());
        }

        printf("\nTotal unique functions to decompile: %d\n\n", targetFunctions.size());

        // Decompile all found functions
        File outFile = new File(getScriptArgs()[0]);
        PrintWriter out = new PrintWriter(outFile);

        for (Function func : targetFunctions) {
            out.printf("// ============================================================\n");
            out.printf("// Function: %s\n", func.getName());
            out.printf("// Address: %s (Ghidra) / 0x%x (ELF VA)\n",
                func.getEntryPoint(),
                func.getEntryPoint().getOffset() - 0x100000);
            out.printf("// Size: %d bytes\n", func.getBody().getNumAddresses());
            out.printf("// ============================================================\n\n");

            DecompileResults results = decomp.decompileFunction(func, 60, monitor);
            if (results.decompileCompleted()) {
                out.println(results.getDecompiledFunction().getC());
            } else {
                out.printf("// DECOMPILATION FAILED: %s\n", results.getErrorMessage());
            }
            out.println();
        }

        out.flush();
        out.close();
        decomp.dispose();

        printf("Output written to: %s\n", outFile.getAbsolutePath());
    }
}
