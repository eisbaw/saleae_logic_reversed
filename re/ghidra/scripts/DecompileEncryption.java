// Decompile encryption functions at rebased addresses (+0x100000)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileEncryption extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Original objdump VAs + 0x100000 = Ghidra addresses
        String[][] targets = new String[][] {
            {"02e97ec0", "SimpleEncryption_write_wrapper"},   // was 0x2d97ec0
            {"02e987b0", "SetControlByte"},                   // was 0x2d987b0
            {"02e989e0", "XOR_encrypt_SIMD"},                 // was 0x2d989e0
            {"02e98710", "XOR_scalar_loop"},                  // was 0x2d98710
            {"02e99290", "LFSR_advance"},                     // was 0x2d99290
            {"02e99310", "SimpleEncryption_read_dispatch"},   // was 0x2d99310
            {"02e91950", "ReseedDevice"},                     // was 0x2d91950
            {"02e99c10", "prepare_buffer"},                   // was 0x2d99c10
            {"01d4e860", "TestFirmwareResponsivenessAndReseed"}, // was 0x1c4e860
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Create functions where missing
        int tx = currentProgram.startTransaction("Create encryption functions");
        try {
            for (String[] t : targets) {
                Address addr = af.getAddress(t[0]);
                Function func = fm.getFunctionAt(addr);
                if (func == null) {
                    Function containing = fm.getFunctionContaining(addr);
                    if (containing != null) {
                        println("NOTE: " + t[0] + " (" + t[1] + ") inside " + containing.getName() + " @ " + containing.getEntryPoint());
                    } else {
                        CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                        cmd.applyTo(currentProgram);
                        func = fm.getFunctionAt(addr);
                        if (func != null) {
                            func.setName(t[1], SourceType.USER_DEFINED);
                            println("CREATED: " + t[1] + " @ " + t[0]);
                        } else {
                            println("FAILED: " + t[0] + " (" + t[1] + ")");
                        }
                    }
                } else {
                    println("EXISTS: " + func.getName() + " @ " + t[0]);
                }
            }
        } finally {
            currentProgram.endTransaction(tx, true);
        }

        // Decompile
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            Function func = fm.getFunctionAt(addr);
            if (func == null) func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("=== NO FUNCTION: " + t[0] + " (" + t[1] + ") ===\n");
                continue;
            }
            println("=== " + func.getName() + " @ " + func.getEntryPoint() + " (" + t[1] + ") ===");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("");
        }
        decomp.dispose();
    }
}
