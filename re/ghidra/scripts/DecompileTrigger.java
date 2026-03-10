// Decompile trigger engine functions at rebased addresses (+0x100000)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileTrigger extends GhidraScript {
    @Override
    public void run() throws Exception {
        // All addresses are Ghidra VAs (ELF VA + 0x100000)
        // These are code locations referencing trigger assertion/log strings
        String[][] targets = new String[][] {
            // DigitalTriggerNode::HandleDigitalData - assertion string xref
            {"01be107f", "HandleDigitalData_xref"},
            // DigitalTriggerNode::HandleSamplingConfiguration - assertion string xref
            {"01be0ebc", "HandleSamplingConfiguration_xref"},
            // SetSoftwareTrigger function - log string xref
            {"02e66d94", "SetSoftwareTrigger_xref"},
            // Received software trigger - handler
            {"02e45483", "ReceivedSoftwareTrigger_xref"},
            // Received empty software trigger
            {"02e4537d", "ReceivedEmptySoftwareTrigger_xref"},
            // Trigger buffer config validation
            {"02e64d4d", "MinTriggerBufferSamples_xref"},
            {"02e64d9b", "PreTriggerLength_xref"},
            {"02e64de9", "PostTriggerLength_xref"},
            {"02e64e87", "MaxTriggerBufXilinx_xref"},
            {"02e64fd4", "MaxTriggerBufAltera_xref"},
            // Holdoff/timeout validation
            {"02e66001", "HoldoffSamples_xref"},
            // Hysteresis validation
            {"02e6609d", "HysteresisFalling_xref"},
            {"02e660e8", "HysteresisRising_xref"},
            // Glitch filter / pulse width
            {"02e66514", "PulseWidthGlitchFilter_xref"},
            // MsoFpga analog trigger channel source
            {"02e47f4d", "AnalogTriggerChannelSrc_xref"},
            // isfinite(triggerThreshold)
            {"01cc1215", "isfinite_TriggerThreshold_xref"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Decompile the containing function for each address
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        java.util.Set<String> decompiled = new java.util.HashSet<>();

        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                // Try creating a function at this address
                CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                int tx = currentProgram.startTransaction("Create function");
                try {
                    cmd.applyTo(currentProgram);
                } finally {
                    currentProgram.endTransaction(tx, true);
                }
                func = fm.getFunctionContaining(addr);
            }
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
