// Decompile slab/deque helper functions not captured by string xrefs
// @category Analysis
// @author claude

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;

import java.io.File;
import java.io.PrintWriter;

public class DecompileSlabHelpers extends GhidraScript {

    // Ghidra VAs (ELF + 0x100000) for helper functions referenced in the decompiled code
    private static final long[] FUNC_ADDRS = {
        0x2e08130L, // mcommit helper (called from EnsureCommitted)
        0x2e08250L, // FixedByteDeque::PopFront (called with uVar10 * 0x10)
        0x2e08520L, // FixedByteDeque::PopBack (called with uVar11 * 8)
        0x2e06dc0L, // Slab::Manager::Allocate (called when free list empty)
        0x2e07b60L, // DequeArena::Size (returns arena byte count)
        0x2e06070L, // some callback/notify function
    };

    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        FunctionManager funcMgr = currentProgram.getFunctionManager();

        String outPath = System.getProperty("user.dir") + "/re/ghidra/decompiled-slab-helpers.txt";
        PrintWriter pw = new PrintWriter(new File(outPath));

        pw.println("// Slab/Deque helper functions");
        pw.println();

        for (long addr : FUNC_ADDRS) {
            Address funcAddr = toAddr(addr);
            Function func = funcMgr.getFunctionAt(funcAddr);
            if (func == null) {
                func = funcMgr.getFunctionContaining(funcAddr);
            }
            if (func == null) {
                pw.println("// NO FUNCTION at Ghidra VA 0x" + Long.toHexString(addr));
                pw.println();
                continue;
            }

            long elfVA = func.getEntryPoint().getOffset() - 0x100000L;
            pw.println("// ----------------------------------------------------------------");
            pw.println("// Function at Ghidra VA 0x" + Long.toHexString(func.getEntryPoint().getOffset()));
            pw.println("// ELF VA: 0x" + Long.toHexString(elfVA));
            pw.println("// Name: " + func.getName());
            pw.println("// Size: " + func.getBody().getNumAddresses() + " bytes");
            pw.println("// ----------------------------------------------------------------");

            DecompileResults results = decomp.decompileFunction(func, 60, monitor);
            if (results.decompileCompleted()) {
                pw.println(results.getDecompiledFunction().getC());
            } else {
                pw.println("// DECOMPILE FAILED: " + results.getErrorMessage());
            }
            pw.println();
        }

        pw.close();
        decomp.dispose();
        println("Decompiled helpers to: " + outPath);
    }
}
