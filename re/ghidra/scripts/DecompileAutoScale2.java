// Decompile VerticalScaleKernel and additional auto-scale functions (TASK-0072)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileAutoScale2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Ghidra VAs (ELF VA + 0x100000)
        String[] addrs = new String[] {
            // VerticalScaleKernel::HandleWaveform (the REAL one, called from FullScaleKernel::HandleWaveform)
            "0x2e5df90",  // ELF 0x2d5df90 - VerticalScaleKernel::HandleWaveform

            // Additional functions in the autoscale region
            "0x2e5cfb0",  // ELF 0x2d5cfb0 - function after FullScaleKernel::HandleWaveform
            "0x2e5d190",  // ELF 0x2d5d190
            "0x2e5d2c0",  // ELF 0x2d5d2c0 - logging/format helper
            "0x2e5ddc0",  // ELF 0x2d5ddc0
            "0x2e5de70",  // ELF 0x2d5de70
            "0x2e5e680",  // ELF 0x2d5e680 - called from FullScaleKernel (line 382)
            "0x2e5e6e0",  // ELF 0x2d5e6e0
            "0x2e5ea00",  // ELF 0x2d5ea00
            "0x2e5ecf0",  // ELF 0x2d5ecf0

            // FUN_02e5b780 called from FullScaleKernel::HandleWaveform (compute result)
            "0x2e5b780",  // ELF 0x2d5b780

            // FUN_02e477d0 called from OnWaveform
            "0x2e477d0",  // ELF 0x2d477d0

            // FUN_02e49330 called from OnWaveform
            "0x2e49330",  // ELF 0x2d49330

            // FUN_02e5d190 called from FullScaleKernel (the ADC sample iterator)
            "0x2e5d190",  // ELF 0x2d5d190
        };

        DecompInterface decomp = new DecompInterface();
        DecompileOptions options = new DecompileOptions();
        decomp.setOptions(options);
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        for (String addrStr : addrs) {
            Address addr = af.getAddress(addrStr);
            if (addr == null) {
                println("--- INVALID ADDRESS: " + addrStr + " ---");
                continue;
            }
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("--- NO FUNCTION at " + addrStr + " ---");
                continue;
            }
            println("=== " + func.getName() + " @ " + func.getEntryPoint() + " (queried: " + addrStr + ") ===");
            println("    Size: " + func.getBody().getNumAddresses() + " bytes");
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
