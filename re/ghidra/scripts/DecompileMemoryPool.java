// Decompile Saleae::MemoryPool functions and their callers.
//
// Key findings so far:
// - FUN_02e9d770 (ELF 0x2d9d770) = MemoryPool::MemoryPool(Configuration) constructor
// - FUN_02e07eb0 (ELF 0x2d07eb0) = FixedByteDeque destructor (uses munmap, not MemoryPool)
// - FUN_02e07fd0 (ELF 0x2d07fd0) = FixedByteDeque::EnsureCommitted (not MemoryPool)
// - The MemoryPool ctor is called from ELF 0x2d96a3f (streaming thread setup)
// - "Waiting for memory" string at ELF 0x2d94e47 (streaming thread)
//
// Usage: ghidra-analyzeHeadless <project_dir> <project_name> -process -noanalysis \
//   -scriptPath re/ghidra/scripts -postScript DecompileMemoryPool.java
//
// @category USB

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.listing.FunctionIterator;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;

import java.io.File;
import java.io.FileWriter;
import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;

public class DecompileMemoryPool extends GhidraScript {

    // Ghidra addresses (ELF VA + 0x100000)
    private static final long[] FUNCTION_ADDRESSES = {
        // MemoryPool constructor
        0x2e9d770L,  // MemoryPool::MemoryPool(Configuration) - confirmed

        // Caller containing MemoryPool ctor call + "Waiting for memory" string
        // ELF 0x2d96a3f calls ctor, ELF 0x2d94e47 references "Waiting for memory"
        // Need to find the containing function
    };

    // Code instruction addresses within functions we want
    private static final long[] CODE_REFS = {
        0x2e9d8beL,  // inside MemoryPool ctor (references ctor name string)
        0x2d96a3fL + 0x100000,  // caller of MemoryPool ctor (ELF 0x2d96a3f -> Ghidra 0x2e96a3f)
        0x2d94e47L + 0x100000,  // references "Waiting for memory" (Ghidra 0x2e94e47)
    };

    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        FunctionManager fm = currentProgram.getFunctionManager();
        Set<Long> seen = new LinkedHashSet<>();
        List<Function> targets = new ArrayList<>();

        // 1. Find functions at known addresses
        for (long addr : FUNCTION_ADDRESSES) {
            Address ghidraAddr = currentProgram.getAddressFactory()
                .getDefaultAddressSpace().getAddress(addr);
            Function f = fm.getFunctionAt(ghidraAddr);
            if (f == null) {
                f = fm.getFunctionContaining(ghidraAddr);
            }
            if (f != null && seen.add(f.getEntryPoint().getOffset())) {
                targets.add(f);
                println("Found function at 0x" + Long.toHexString(addr) +
                    ": " + f.getName() + " @ " + f.getEntryPoint() +
                    " (ELF VA: 0x" + Long.toHexString(f.getEntryPoint().getOffset() - 0x100000) + ")");
            }
        }

        // 2. Find functions containing code references
        for (long addr : CODE_REFS) {
            Address ghidraAddr = currentProgram.getAddressFactory()
                .getDefaultAddressSpace().getAddress(addr);
            Function f = fm.getFunctionContaining(ghidraAddr);
            if (f != null && seen.add(f.getEntryPoint().getOffset())) {
                targets.add(f);
                println("Found containing function for 0x" + Long.toHexString(addr) +
                    ": " + f.getName() + " @ " + f.getEntryPoint() +
                    " (ELF VA: 0x" + Long.toHexString(f.getEntryPoint().getOffset() - 0x100000) + ")");
            }
        }

        // 3. Find callers of the MemoryPool constructor
        Address ctorAddr = currentProgram.getAddressFactory()
            .getDefaultAddressSpace().getAddress(0x2e9d770L);
        Function ctorFunc = fm.getFunctionAt(ctorAddr);
        if (ctorFunc != null) {
            // Get references TO the constructor
            ReferenceIterator refIter = currentProgram.getReferenceManager()
                .getReferencesTo(ctorFunc.getEntryPoint());
            while (refIter.hasNext()) {
                Reference ref = refIter.next();
                Function caller = fm.getFunctionContaining(ref.getFromAddress());
                if (caller != null && seen.add(caller.getEntryPoint().getOffset())) {
                    targets.add(caller);
                    println("Found caller of MemoryPool ctor: " + caller.getName() +
                        " @ " + caller.getEntryPoint());
                }
            }
        }

        // 4. Find functions called BY the MemoryPool constructor (helper methods)
        if (ctorFunc != null) {
            // Look at calls made from within the constructor
            Address start = ctorFunc.getEntryPoint();
            Address end = ctorFunc.getBody().getMaxAddress();
            ReferenceIterator refIter = currentProgram.getReferenceManager()
                .getReferenceIterator(start);
            while (refIter.hasNext()) {
                Reference ref = refIter.next();
                if (ref.getFromAddress().compareTo(end) > 0) break;
                if (ref.getReferenceType().isCall()) {
                    Function callee = fm.getFunctionAt(ref.getToAddress());
                    if (callee != null && seen.add(callee.getEntryPoint().getOffset())) {
                        // Only add non-trivial callees (skip standard library)
                        long calleeAddr = callee.getEntryPoint().getOffset();
                        if (calleeAddr > 0x2000000L && calleeAddr < 0x2f00000L) {
                            targets.add(callee);
                            println("Found callee of MemoryPool ctor: " + callee.getName() +
                                " @ " + callee.getEntryPoint());
                        }
                    }
                }
            }
        }

        // 5. Also look for functions that might be MemoryPool::Allocate
        // Search for functions adjacent to the constructor
        for (long offset = 0x2e9d8f0L; offset < 0x2e9e100L; offset += 0x10) {
            Address addr = currentProgram.getAddressFactory()
                .getDefaultAddressSpace().getAddress(offset);
            Function f = fm.getFunctionAt(addr);
            if (f != null && seen.add(f.getEntryPoint().getOffset())) {
                targets.add(f);
                println("Found adjacent function: " + f.getName() + " @ " + f.getEntryPoint() +
                    " size=" + f.getBody().getNumAddresses());
            }
        }

        println("Total functions to decompile: " + targets.size());

        // 6. Decompile and write output
        String outPath = System.getProperty("user.home") +
            "/nvidia/saleae_reverse/re/ghidra/decompiled-memory-pool.txt";
        PrintWriter out = new PrintWriter(new FileWriter(outPath));

        out.println("// Decompiled MemoryPool functions from libgraph_server_shared.so");
        out.println("// Generated by DecompileMemoryPool.java");
        out.println("// Ghidra addresses = ELF VA + 0x100000");
        out.println("// " + new java.util.Date());
        out.println();

        for (Function f : targets) {
            out.println("// ========================================================================");
            out.println("// Function: " + f.getName());
            out.println("// Address:  " + f.getEntryPoint() + " (ELF VA: 0x" +
                Long.toHexString(f.getEntryPoint().getOffset() - 0x100000) + ")");
            out.println("// Size:     " + f.getBody().getNumAddresses() + " bytes");
            out.println("// ========================================================================");
            out.println();

            DecompileResults result = decomp.decompileFunction(f, 120, monitor);
            if (result.decompileCompleted()) {
                out.println(result.getDecompiledFunction().getC());
            } else {
                out.println("// DECOMPILATION FAILED: " + result.getErrorMessage());
            }
            out.println();
        }

        out.close();
        decomp.dispose();

        println("Output written to: " + outPath);
    }
}
