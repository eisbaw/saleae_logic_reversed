// Decompile FPGA glitch filter and hardware trigger register functions
// TASK-0091: Extract TriggerMode/TriggerType enums, glitch filter params,
// hardware trigger register layout
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileGlitchTrigger extends GhidraScript {
    @Override
    public void run() throws Exception {
        // All addresses are Ghidra VAs (ELF VA + 0x100000)
        String[][] targets = new String[][] {
            // SetAnalogTriggerConfiguration - main trigger config function
            // ELF 0x02d654a0 -> Ghidra 0x02e654a0
            {"02e654a0", "MsoFpga_SetAnalogTriggerConfiguration"},

            // InternalSetDigitalConfiguration - glitch filter
            // ELF 0x02d663f0 -> Ghidra 0x02e663f0
            {"02e663f0", "MsoFpga_InternalSetDigitalConfiguration"},

            // SetAnalogChannelConfiguration - writes trigger buffer regs
            // ELF 0x02d64640 -> Ghidra 0x02e64640
            {"02e64640", "MsoFpga_SetAnalogChannelConfiguration"},

            // SetSoftwareTrigger
            // ELF 0x02d66d60 -> Ghidra 0x02e66d60
            {"02e66d60", "MsoFpga_SetSoftwareTrigger"},

            // PreprocessConfiguration (analog trigger channel source)
            // ELF 0x02d47b00 -> Ghidra 0x02e47b00
            {"02e47b00", "PreprocessConfiguration"},

            // SetField<TriggerType> assertion is at a specific template instantiation
            // Search around the SetAnalogTriggerConfiguration function
            // SetField assertion strings indicate specific instantiation addresses
            // We'll decompile a range around the main function

            // CalculateDigitalConfigurationAddress_DigitalDerived
            // Look for function near the assertion string
            // "address < DigitalConfiguration_DigitalDerived_END"
            // This is part of FpgaRegisters namespace

            // SetDigitalDerivedVoltageThresholds - referenced in strings
            // Need to find via xref from Ghidra

            // SetEnabledSmartCableDigitalChannels
            // ELF VA in re/notes has it documented but let's include related functions

            // Try addresses from nearby functions in the MsoFpga:: region
            // The MsoFpga functions cluster around 0x02e64xxx-0x02e67xxx

            // DisableAnalogTrigger - search around the known functions
            // Based on "Disabling analog trigger" string
            {"02e66c30", "MsoFpga_DisableAnalogTrigger_guess1"},
            {"02e66b00", "MsoFpga_DisableAnalogTrigger_guess2"},

            // Functions near InternalSetDigitalConfiguration
            {"02e66580", "MsoFpga_near_glitch_1"},
            {"02e66680", "MsoFpga_near_glitch_2"},
            {"02e66780", "MsoFpga_near_glitch_3"},

            // Functions near SetAnalogTriggerConfiguration
            {"02e65f00", "MsoFpga_near_trigger_1"},
            {"02e66000", "MsoFpga_near_trigger_2"},
            {"02e66100", "MsoFpga_near_trigger_3"},
            {"02e66200", "MsoFpga_near_trigger_4"},
            {"02e66300", "MsoFpga_near_trigger_5"},
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

        // Also enumerate all functions in the MsoFpga region (0x02e63xxx - 0x02e67xxx)
        println("\n=== FUNCTION LIST in MsoFpga region 0x02e63000-0x02e68000 ===");
        Address start = af.getAddress("02e63000");
        Address end = af.getAddress("02e68000");
        FunctionIterator iter = fm.getFunctions(start, true);
        while (iter.hasNext()) {
            Function f = iter.next();
            if (f.getEntryPoint().compareTo(end) > 0) break;
            println("  " + f.getEntryPoint() + "  " + f.getName() +
                    "  size=" + f.getBody().getNumAddresses());
        }

        decomp.dispose();
    }
}
