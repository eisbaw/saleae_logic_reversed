// Create functions at known addresses and decompile them
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class CreateAndDecompile extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[][] targets = new String[][] {
            {"02d97ec0", "SimpleEncryption_write_wrapper"},
            {"02d987b0", "SetControlByte"},
            {"02d989e0", "XOR_encrypt_SIMD"},
            {"02d98710", "XOR_scalar_loop"},
            {"02d99290", "LFSR_advance"},
            {"02d99310", "SimpleEncryption_read_dispatch"},
            {"02d91950", "ReseedDevice"},
            {"02d99c10", "prepare_buffer"},
            {"01c4e860", "TestFirmwareResponsivenessAndReseed"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // First pass: create functions where missing
        int tx = currentProgram.startTransaction("Create encryption functions");
        try {
            for (String[] t : targets) {
                Address addr = af.getAddress(t[0]);
                Function func = fm.getFunctionAt(addr);
                if (func == null) {
                    func = fm.getFunctionContaining(addr);
                    if (func != null) {
                        println("NOTE: " + t[0] + " is inside " + func.getName() + " @ " + func.getEntryPoint());
                    } else {
                        println("Creating function at " + t[0]);
                        CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                        cmd.applyTo(currentProgram);
                        func = fm.getFunctionAt(addr);
                        if (func != null) {
                            func.setName(t[1], ghidra.program.model.symbol.SourceType.USER_DEFINED);
                            println("  Created: " + func.getName());
                        } else {
                            println("  FAILED to create function at " + t[0]);
                        }
                    }
                } else {
                    println("EXISTS: " + func.getName() + " @ " + func.getEntryPoint());
                }
            }
        } finally {
            currentProgram.endTransaction(tx, true);
        }

        // Second pass: decompile
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            Function func = fm.getFunctionAt(addr);
            if (func == null) {
                func = fm.getFunctionContaining(addr);
            }
            if (func == null) {
                println("=== STILL NO FUNCTION at " + t[0] + " (" + t[1] + ") ===");
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
