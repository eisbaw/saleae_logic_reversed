// Decompile edge/pulse trigger detection and settings serialization
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileTrigger3 extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[][] targets = new String[][] {
            // Edge trigger detection (called when trigger type == 0)
            {"01be1960", "EdgeTrigger_Detect"},
            // Pulse trigger detection (called when trigger type == 1)
            {"01be23c0", "PulseTrigger_Detect"},
            // Get sample range for trigger check
            {"01be1890", "GetTriggerSampleRange"},
            // DigitalTriggerNode constructor (large fn at 01be0530)
            {"01be0530", "DigitalTriggerNode_constructor"},
            // FUN_02dfe590 - called to get digital data length
            {"02dfe590", "DigitalData_GetLength"},
            // FUN_02e00d10 - update trigger output data
            {"02e00d10", "TriggerOutput_Update"},
            // FUN_02e00f80 - read trigger state
            {"02e00f80", "TriggerState_Read"},
            // FUN_02e000f0 - trigger state processing
            {"02e000f0", "TriggerState_Process"},
            // FUN_01bde140 - part of trigger output chain
            {"01bde140", "TriggerOutput_Chain"},
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

            println("=== " + func.getName() + " @ " + func.getEntryPoint() + " (via " + t[1] + ") ===");
            println("Size: " + func.getBody().getNumAddresses() + " bytes");
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
