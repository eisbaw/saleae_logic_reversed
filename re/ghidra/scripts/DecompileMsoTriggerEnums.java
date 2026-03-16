// Decompile functions related to MSO trigger type/mode/glitch filter enums
// TASK-0208, TASK-0209, TASK-0214: Identify all possible enum values
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileMsoTriggerEnums extends GhidraScript {
    @Override
    public void run() throws Exception {
        // All addresses are Ghidra VAs (ELF VA + 0x100000)
        String[][] targets = new String[][] {
            // FUN_01be5210 - assertion/abort when trigger type is invalid (not 0 or 1)
            // Called from DigitalTrigger_CheckTrigger when iVar1 != 0 && iVar1 != 1
            {"01be5210", "InvalidTriggerType_Assert"},

            // FUN_01bc4350 - initializes trigger settings from config
            // Called early in DigitalTriggerNode constructor
            {"01bc4350", "InitTriggerSettings"},

            // FUN_01be1890 - called from CheckTrigger, computes trigger range
            {"01be1890", "ComputeTriggerRange"},

            // FUN_01be12f0 - called in HandleSamplingConfiguration path
            {"01be12f0", "HandleSamplingConfig_helper"},

            // FUN_01be0d60 - callback registered in DigitalTriggerNode
            {"01be0d60", "TriggerNodeCallback1"},

            // DisableAnalogTrigger region - look for TriggerMode enum usage
            // ELF 0x02d65280 -> Ghidra 0x02e65280
            {"02e65280", "DisableAnalogTrigger"},

            // SetCaptureParameters - where r0x1B is written
            // ELF 0x1c24ca0 -> Ghidra 0x1d24ca0
            {"01d24ca0", "SetCaptureParameters"},

            // FUN_01c6f110 - registered for /currentTriggerType path
            // This might show the enum serialization
            {"01c6f110", "CurrentTriggerType_Handler"},

            // Look at area around /triggerType string reference
            // Search for functions near the trigger configuration area
            // that might set or read trigger type/mode enum values

            // FUN_02db06b0 - called in DigitalTriggerNode to get EdgeTrigger string
            {"02db06b0", "GetEdgeTriggerName"},

            // FUN_02db3cc0 - called in DigitalTriggerNode to get second trigger name
            {"02db3cc0", "GetSecondTriggerName"},

            // FUN_02db4050 - called in DigitalTriggerNode for third name
            {"02db4050", "GetThirdTriggerName"},

            // FUN_02dfe470 - called to get first string in trigger node
            {"02dfe470", "GetFirstTriggerString"},
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
            DecompileResults res = decomp.decompileFunction(func, 180, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("");
        }

        // Search for xrefs to SetAnalogTriggerConfiguration to find all callers
        println("\n=== XREFS to SetAnalogTriggerConfiguration (Ghidra 0x02e654a0) ===");
        Address trigCfgAddr = af.getAddress("02e654a0");
        Function trigCfgFunc = fm.getFunctionContaining(trigCfgAddr);
        if (trigCfgFunc != null) {
            ghidra.program.model.symbol.ReferenceManager refMgr = currentProgram.getReferenceManager();
            java.util.Iterator<ghidra.program.model.symbol.Reference> refs =
                refMgr.getReferencesTo(trigCfgFunc.getEntryPoint()).iterator();
            while (refs.hasNext()) {
                ghidra.program.model.symbol.Reference ref = refs.next();
                Address fromAddr = ref.getFromAddress();
                Function caller = fm.getFunctionContaining(fromAddr);
                String callerName = caller != null ? caller.getName() : "UNKNOWN";
                long callerElf = fromAddr.getOffset() - 0x100000;
                println("  Called from " + fromAddr + " (ELF 0x" + Long.toHexString(callerElf) + ") in " + callerName);
            }
        }

        decomp.dispose();
    }
}
