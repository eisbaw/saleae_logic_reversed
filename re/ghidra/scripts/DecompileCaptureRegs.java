// Decompile functions relevant to SetCaptureParameters register computation
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileCaptureRegs extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] addrs = new String[] {
            // FUN_01d4bbb0 - computes total channel data rate (used for reg 0x14)
            "0x01d4bbb0",
            // FUN_01d47a80 - HasDigital check on SampleRateOption
            "0x01d47a80",
            // FUN_01d47aa0 - DigitalSampleRateHz getter
            "0x01d47aa0",
            // FUN_01d47a90 - HasAnalog check
            "0x01d47a90",
            // FUN_01d47b30 - AnalogPhysicalSampleRateHz getter
            "0x01d47b30",
            // FUN_01d47cc0 - GetDeviceType
            "0x01d47cc0",
            // FUN_01d4bb60 - GetCurrentSampleRateOption
            "0x01d4bb60",
            // FUN_01d47d20 - GetTotalChannelCount
            "0x01d47d20",
            // FUN_01d47d40 - GetChannelByIndex
            "0x01d47d40",
            // FUN_01d47500 - IsDigitalEnabled on channel
            "0x01d47500",
            // FUN_01d47510 - IsAnalogEnabled on channel
            "0x01d47510",
            // FUN_01d4bea0 - GetAdcTypeConfigurations
            "0x01d4bea0",
            // FUN_01d4bee0 - GetAdcTypeForCurrentConfig
            "0x01d4bee0",
            // FUN_01d4c510 - CompareAdcType
            "0x01d4c510",
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        for (String addrStr : addrs) {
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("--- NO FUNCTION at " + addrStr + " ---");
                continue;
            }
            println("=== " + func.getName() + " @ " + func.getEntryPoint()
                    + " (ELF: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000) + ") ===");
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
