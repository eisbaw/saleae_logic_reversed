// Decompile libusb speed functions and GPIF state machine init for TASK-0071
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileUsbSpeed2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // All addresses are Ghidra VAs (ELF VA + 0x100000)
        String[] addrs = new String[] {
            // libusb speed wrapper - called by all GetUsbSpeed implementations
            "0x2f9ac00",  // FUN_02e9ac00 - query USB speed from libusb

            // Device type query function
            "0x2f9ae80",  // FUN_02e9ae80 - get device type from USB device

            // WriteConfigurationData (chunk size logic)
            "0x1d544b0",  // FpgaDeviceFeatures::WriteConfigurationData

            // Save function for DeviceSettings (serializes hasLimitedBandwidth)
            "0x1d4bbb0",  // CalculateBandwidth / Save

            // GetDeviceSettings for Pro - see how it sets up USB speed
            "0x1d449d0",  // LogicProfessional8Device GetDeviceSettings

            // LogicProfessionalDevice constructor (sets up USB speed detection)
            "0x1d41540",  // LogicProfessionalDevice constructor
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
