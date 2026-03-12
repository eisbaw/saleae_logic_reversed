// Decompile ParseFirmwareImage and GetFirmwareForDeviceType
// Ghidra addresses = objdump VA + 0x100000
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileFirmwareParsing extends GhidraScript {
    @Override
    public void run() throws Exception {
        // objdump VA + 0x100000 = Ghidra address
        String[][] targets = new String[][] {
            {"02dc9130", "ParseFirmwareImage"},
            {"02dc96b0", "ParseIHexFirmwareImage"},
            {"02daad60", "GetFirmwareForDeviceType"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Create functions, handling cases where address is inside another function
        int tx = currentProgram.startTransaction("Create firmware functions");
        try {
            for (String[] t : targets) {
                Address addr = af.getAddress(t[0]);
                Function func = fm.getFunctionAt(addr);
                if (func == null) {
                    Function containing = fm.getFunctionContaining(addr);
                    if (containing != null) {
                        println("INSIDE: " + t[0] + " (" + t[1] + ") is inside " + containing.getName() + " @ " + containing.getEntryPoint());
                        // Disassemble at our address and try to create function
                        disassemble(addr);
                        CreateFunctionCmd cmd = new CreateFunctionCmd(addr, true);
                        boolean ok = cmd.applyTo(currentProgram);
                        if (ok) {
                            func = fm.getFunctionAt(addr);
                            if (func != null) {
                                func.setName(t[1], SourceType.USER_DEFINED);
                                println("CREATED (split): " + t[1] + " @ " + t[0]);
                            }
                        } else {
                            println("FAILED split at: " + t[0] + " (" + t[1] + ")");
                            // Decompile the containing function instead
                        }
                    } else {
                        // No containing function, try to create
                        disassemble(addr);
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
                    if (!func.getName().equals(t[1])) {
                        func.setName(t[1], SourceType.USER_DEFINED);
                        println("RENAMED: " + t[1] + " @ " + t[0]);
                    } else {
                        println("EXISTS: " + func.getName() + " @ " + t[0]);
                    }
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
            if (func == null) {
                // Try containing function
                func = fm.getFunctionContaining(addr);
                if (func != null) {
                    println("=== CONTAINING: " + func.getName() + " @ " + func.getEntryPoint() + " (contains " + t[1] + ") ===");
                } else {
                    println("=== NO FUNCTION: " + t[0] + " (" + t[1] + ") ===\n");
                    continue;
                }
            } else {
                println("=== " + func.getName() + " @ " + func.getEntryPoint() + " (" + t[1] + ") ===");
            }
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
