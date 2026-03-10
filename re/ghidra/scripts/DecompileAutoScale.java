// Decompile auto-scale related functions for TASK-0072
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileAutoScale extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Ghidra VAs (ELF VA + 0x100000)
        // Function entry points identified by scanning for prologues
        String[] addrs = new String[] {
            // Auto-scale core functions
            "0x2e5bb20",  // AutoScaleRunner::OnWaveform (from autoscale_runner.cpp)
            "0x2e5c1e0",  // FullScaleKernel::ctor (from autoscale_horizontal_kernel.cpp)
            "0x2e5c600",  // FullScaleKernel::HandleWaveform (from autoscale_horizontal_kernel.cpp)
            "0x2e5d8b0",  // VerticalScaleKernel::HandleWaveform (from autoscale_vertical_kernel.cpp)

            // Helper functions (nearby)
            "0x2e5c0e0",  // Unknown function between OnWaveform and ctor
            "0x2e5d270",  // Unknown function between HandleWaveform variants
            "0x2e5d5a0",  // Unknown function before VerticalScaleKernel::HandleWaveform
            "0x2e5e6c0",  // Unknown function after VerticalScaleKernel::HandleWaveform

            // ForConfigNumber / GetConfigNumber
            "0x2e5b3a0",  // ForConfigNumber (from assertion)
            "0x2e5b6e0",  // Function between ForConfigNumber and OnWaveform

            // AnalogChannelConfigToHardwareConfig
            "0x2e42b00",  // AnalogChannelConfigToHardwareConfig

            // ClampAndQuantizeDacVoltage
            "0x2e32760",  // ClampAndQuantizeDacVoltage

            // HandleStartAutoScale (the entry point from IPC)
            "0x1cbe3d0",  // HandleStartAutoScale

            // Serialization of results
            "0x2e8c2e0",  // FullScaleChannelResult serialize
            "0x2e8d3d0",  // VerticalScaleChannelResult serialize

            // Additional nearby functions (0x2e5b010)
            "0x2e5b010",  // Unknown function at start of autoscale region
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
                // Try to create function at this address
                println("    (Ghidra may not have identified this as a function)");
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
