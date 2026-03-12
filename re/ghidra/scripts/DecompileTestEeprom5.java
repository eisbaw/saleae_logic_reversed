// Decompile FX3 WriteEeprom (FUN_01d50d80) and ReadEeprom (FUN_01d52460)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileTestEeprom5 extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[][] targets = new String[][] {
            {"01d50d80", "Fx3DeviceFeatures_WriteEeprom"},
            {"01d52460", "Fx3DeviceFeatures_ReadEeprom"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Create functions where missing
        int tx = currentProgram.startTransaction("Create FX3 eeprom functions");
        try {
            for (String[] t : targets) {
                Address addr = af.getAddress(t[0]);
                Function func = fm.getFunctionAt(addr);
                if (func == null) {
                    Function containing = fm.getFunctionContaining(addr);
                    if (containing != null) {
                        println("NOTE: " + t[0] + " inside " + containing.getName() + " @ " + containing.getEntryPoint());
                    } else {
                        CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                        cmd.applyTo(currentProgram);
                        func = fm.getFunctionAt(addr);
                        if (func != null) {
                            func.setName(t[1], SourceType.USER_DEFINED);
                            println("CREATED: " + t[1] + " @ " + t[0]);
                        } else {
                            println("FAILED to create: " + t[0]);
                        }
                    }
                } else {
                    println("EXISTS: " + func.getName() + " @ " + t[0]);
                }
            }
        } finally {
            currentProgram.endTransaction(tx, true);
        }

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            Function func = fm.getFunctionAt(addr);
            if (func == null) func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("=== NO FUNCTION at " + t[0] + " ===");
                continue;
            }
            println("=== " + func.getName() + " @ " + func.getEntryPoint() + " ===");
            DecompileResults res = decomp.decompileFunction(func, 300, monitor);
            if (res.decompileCompleted()) {
                String code = res.getDecompiledFunction().getC();
                if (code == null || code.isEmpty()) {
                    println("DECOMPILE RETURNED EMPTY CODE");
                } else {
                    println(code);
                }
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("--- END ---");
        }
        decomp.dispose();
    }
}
