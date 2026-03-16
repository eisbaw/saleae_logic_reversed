// Decompile small trigger helper functions for enum analysis
// TASK-0208/0209/0214: Focus on the assertion function and trigger init
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileMsoTriggerEnums2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // All addresses are Ghidra VAs (ELF VA + 0x100000)
        String[][] targets = new String[][] {
            // FUN_01be5210 - assertion/abort when trigger type is invalid
            {"01be5210", "InvalidTriggerType_Assert"},

            // FUN_01bc4350 - initializes trigger settings from config
            {"01bc4350", "InitTriggerSettings"},

            // FUN_01be1890 - computes trigger range
            {"01be1890", "ComputeTriggerRange"},

            // FUN_01be12f0 - called in HandleSamplingConfiguration path
            {"01be12f0", "HandleSamplingConfig_helper"},

            // FUN_01be0d60 - callback registered in DigitalTriggerNode
            {"01be0d60", "TriggerNodeCallback1"},

            // Callers of SetAnalogTriggerConfiguration
            // ELF 0x1799d0c -> Ghidra 0x1899d0c (within UNKNOWN)
            {"01899d0c", "Caller1_SetAnalogTriggerConfig"},
            // ELF 0x195e148 -> Ghidra 0x1a5e148 (within UNKNOWN)
            {"01a5e148", "Caller2_SetAnalogTriggerConfig"},
            // ELF 0x2d4b31a -> Ghidra 0x2e4b31a (within FUN_02e4a280)
            {"02e4a280", "Caller3_SetAnalogTriggerConfig_MsoDevice"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        java.util.Set<String> decompiled = new java.util.HashSet<>();

        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("=== NO FUNCTION containing " + t[0] + " (" + t[1] + ") ===");
                continue;
            }

            String funcKey = func.getEntryPoint().toString();
            if (decompiled.contains(funcKey)) {
                println("=== ALREADY DECOMPILED: " + func.getName() + " @ " + funcKey + " (for " + t[1] + ") ===");
                continue;
            }
            decompiled.add(funcKey);

            long elfVa = func.getEntryPoint().getOffset() - 0x100000;
            println("=== " + func.getName() + " @ " + func.getEntryPoint()
                    + " (ELF: 0x" + Long.toHexString(elfVa) + ", via " + t[1] + ") ===");
            println("Size: " + func.getBody().getNumAddresses() + " bytes");

            // Skip very large functions (>10KB) to avoid output overflow
            if (func.getBody().getNumAddresses() > 10000) {
                println("SKIPPED: function too large (" + func.getBody().getNumAddresses() + " bytes)");
                println("");
                continue;
            }

            DecompileResults res = decomp.decompileFunction(func, 180, monitor);
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
