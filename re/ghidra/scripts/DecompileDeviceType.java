// Decompile DeviceType enum functions at rebased addresses (+0x100000)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileDeviceType extends GhidraScript {
    @Override
    public void run() throws Exception {
        // ELF VAs + 0x100000 = Ghidra addresses
        // GetVidPidForDevice: ELF VA 0x1c13df0 -> Ghidra 0x1d13df0
        // DeviceTypeToString function: near ELF VA 0x2ddb0f0 -> Ghidra 0x2edb0f0
        // The assertion xref function at ELF 0x1ae9815 -> Ghidra 0x1be9815
        // Second assertion xref function at ELF 0x1aeadf0 -> Ghidra 0x1beadf0
        // DeviceTypeToString table lookup function at ~0x2ddb141 -> Ghidra 0x2edb141
        // Small helper at ~0x2ddb0f1 -> Ghidra 0x2edb0f1
        // AddNewDevice factory near 0x1c13eb0 -> Ghidra 0x1d13eb0
        String[][] targets = new String[][] {
            {"01d13df0", "GetVidPidForDevice"},
            {"01d13eb0", "AddNewDevice_factory"},
            {"02edb0f0", "DeviceTypeToString_helper"},
            {"02edb141", "DeviceTypeToString_main"},
            {"01be9815", "assertion_func_1"},
            {"01beadf0", "assertion_func_2"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Create functions where missing
        int tx = currentProgram.startTransaction("Create device type functions");
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
                            println("FAILED to create function: " + t[0] + " (" + t[1] + ")");
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
