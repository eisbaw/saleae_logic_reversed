// Decompile inner trigger detection functions (called from HandleDigitalData)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileTrigger2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Inner functions called by DigitalTriggerNode::HandleDigitalData
        // and related trigger processing functions
        // All addresses are Ghidra VAs (ELF VA + 0x100000)
        String[][] targets = new String[][] {
            // Called from HandleDigitalData when trigger enabled & not triggered
            {"01be1720", "DigitalTrigger_CheckTrigger"},
            // Called from HandleDigitalData at end (trigger output/emit)
            {"01be10f0", "DigitalTrigger_EmitOutput"},
            // Called from HandleSamplingConfiguration
            {"01be12f0", "DigitalTrigger_ConfigReset"},
            // FUN_01be50b0 - appears to be a channel/state lookup
            {"01be50b0", "DigitalTrigger_GetChannelState"},
            // FUN_02e00f10 - called with lVar4 + 0x68
            {"02e00f10", "Trigger_UpdateState"},
            // FUN_02e00ba0 - called when trigger active
            {"02e00ba0", "Trigger_ProcessData"},
            // DigitalTriggerSettingsData serialization
            // Find the DigitalTriggerNode constructor area
            {"01be0600", "DigitalTriggerNode_area1"},
            {"01be0700", "DigitalTriggerNode_area2"},
            {"01be0800", "DigitalTriggerNode_area3"},
            {"01be0900", "DigitalTriggerNode_area4"},
            {"01be0a00", "DigitalTriggerNode_area5"},
            {"01be0b00", "DigitalTriggerNode_area6"},
            {"01be0c00", "DigitalTriggerNode_area7"},
            {"01be0d00", "DigitalTriggerNode_area8"},
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
                println("=== NO FUNCTION containing " + t[0] + " (" + t[1] + ") ===\n");
                continue;
            }

            String funcKey = func.getEntryPoint().toString();
            if (decompiled.contains(funcKey)) {
                println("=== ALREADY DECOMPILED: " + func.getName() + " @ " + funcKey + " (for " + t[1] + ") ===\n");
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
