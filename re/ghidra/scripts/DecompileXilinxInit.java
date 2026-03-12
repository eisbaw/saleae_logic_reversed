// Decompile AD9637/Xilinx Spartan-6 init functions by xref addresses
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import java.util.*;

public class DecompileXilinxInit extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Ghidra VAs (ELF VA + 0x100000) - addresses within functions that reference key strings
        // We use getFunctionContaining to find the enclosing function
        String[][] refs = new String[][] {
            // {ghidra_va_of_reference, description}
            {"0x1d20a4f", "WriteAd9637Register (xref to assertion string)"},
            {"0x1d236d2", "SetAd9637Enabled (xref to assertion string)"},
            {"0x1d23530", "SetAd9637Enabled caller (xref to short string)"},
            {"0x1d2273f", "CommonDeviceInit (xref to name string)"},
            {"0x1d22330", "IsFpgaResponsive (xref 1)"},
            {"0x1d22a99", "IsFpgaResponsive (xref 2 - possibly different func)"},
            {"0x1d2275e", "FPGA readback failed (xref)"},
            {"0x1d254be", "SetCaptureParameters (xref 1 to assertion)"},
            {"0x1d2638e", "SetCaptureParameters (xref 2 - may be different func)"},
            {"0x1d264a6", "SetCaptureParameters (xref 3)"},
            {"0x1d265c5", "SetCaptureParameters (xref 4)"},
            {"0x1d26620", "SetCaptureParameters (xref 5)"},
            {"0x1d54643", "FpgaDeviceFeatures::WriteConfigurationData (xref)"},
            {"0x2e3833a", "FPGA Programmed? (xref)"},
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Track already-decompiled functions to avoid duplicates
        Set<String> decompiled = new HashSet<>();

        for (String[] ref : refs) {
            String addrStr = ref[0];
            String desc = ref[1];
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("--- NO FUNCTION containing " + addrStr + " (" + desc + ") ---");
                continue;
            }
            String entryStr = func.getEntryPoint().toString();
            if (decompiled.contains(entryStr)) {
                println("--- ALREADY DECOMPILED: " + func.getName() + " @ " + entryStr + " (from " + desc + ") ---");
                println("");
                continue;
            }
            decompiled.add(entryStr);

            println("=== " + func.getName() + " @ " + entryStr + " (found via: " + desc + ") ===");
            println("ELF VA: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000));
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
