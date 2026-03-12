// Decompile GraphTime sub-serializers for RocksDB keys
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

public class DecompileRocksDBKeys2 extends GhidraScript {

    private static final long[] FUNC_ADDRS = {
        // GraphTime sub-serializers (called from FUN_01c9a920)
        0x1c9aad0L, // FUN_01c9aad0 - serialize first component (param_1 = i128 ptr)
        0x1d9a530L, // FUN_01d9a530 - serialize second component (param_1+0x10)

        // The Write/Put calls from the store functions reference these
        // DAT_005202e4 is referenced as default column family
        // Let's look at the store initialization and the callers

        // AnalogStore::Initialize callers
        // DigitalStore constructor lambda

        // Also look at the data that references "saleae.data.timing.MakeGraphTime"
        // at ELF 0x3d9d22 -> Ghidra 0x4d9d22
    };

    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        FunctionManager funcMgr = currentProgram.getFunctionManager();

        String outPath = System.getProperty("user.dir") + "/re/ghidra/decompiled-rocksdb-keys2.txt";
        File outFile = new File(outPath);
        PrintWriter pw = new PrintWriter(outFile);

        pw.println("// ================================================================");
        pw.println("// GraphTime i128 key serialization sub-functions");
        pw.println("// ================================================================");
        pw.println();

        for (long addr : FUNC_ADDRS) {
            Address funcAddr = toAddr(addr);
            Function func = funcMgr.getFunctionAt(funcAddr);
            if (func == null) {
                pw.println("// WARNING: No function at Ghidra VA 0x" + Long.toHexString(addr));
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

            DecompileResults results = decomp.decompileFunction(func, 120, monitor);
            if (results.decompileCompleted()) {
                pw.println(results.getDecompiledFunction().getC());
            } else {
                pw.println("// DECOMPILE FAILED: " + results.getErrorMessage());
            }
            pw.println();
        }

        pw.close();
        decomp.dispose();
        println("Done: " + outPath);
    }
}
