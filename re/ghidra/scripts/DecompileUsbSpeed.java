// Decompile USB speed-related functions for TASK-0071
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileUsbSpeed extends GhidraScript {
    @Override
    public void run() throws Exception {
        // All addresses are Ghidra VAs (ELF VA + 0x100000)
        String[] addrs = new String[] {
            // GetUsbSpeed implementations (vtable slot 6)
            "0x1d3d970",  // LogicGraduateDevice::GetUsbSpeed (Logic 8)
            "0x1d56080",  // LogicStudentDevice::GetUsbSpeed (Logic 4)
            "0x1d415e0",  // LogicProfessionalDevice::GetUsbSpeed (Pro 8/16)

            // ComputeTransferSize and related
            "0x1d50230",  // ComputeTransferSize (shared, vtable slot 11)
            "0x1d4fc50",  // AdjustTransferSize
            "0x1d4a0c0",  // GetInterfaceMaximumBitRate

            // USB speed string references
            "0x1ed9a21",  // references "USB Full Speed" string

            // SetOverrideUsb2PowerLimit handler
            "0x020fd338",  // references /overrideUsb2PowerLimit
            "0x020fd40e",  // references /overrideUsb2PowerLimit
            "0x020fdd78",  // references /overrideUsb2PowerLimit

            // DownloadBitstream (chunk size depends on USB speed)
            "0x1d53f20",  // FpgaDeviceFeatures::DownloadBitstream

            // GetAnalogAdvancedPerformanceOptions (bandwidth tiers)
            "0x1d4a250",  // GetAnalogAdvancedPerformanceOptions

            // Bandwidth calculation
            "0x1d4bbb0",  // CalculateBandwidth

            // LogicProfessionalDevice GetDeviceSettings/performance
            "0x1d418c0",  // LogicProfessionalDevice performance options

            // Logic 8 GetDeviceSettings
            "0x1d3e0a0",  // LogicGraduateDevice performance options
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
            println("=== " + func.getName() + " @ " + func.getEntryPoint() + " (queried: " + addrStr + ") ===");
            println("Size: " + func.getBody().getNumAddresses() + " bytes");
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
