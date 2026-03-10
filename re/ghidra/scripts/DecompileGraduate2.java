// Decompile additional functions referenced by LogicGraduateDevice methods
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileGraduate2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[][] entries = new String[][] {
            // FUN_01d23840 - called from slot 2 (Initialize) to read HW revision
            {"0x1d23840", "Initialize_ReadHwRevision"},
            // FUN_01d21fb0 - called from slot 2 after revision read
            {"0x1d21fb0", "Initialize_PostRevision"},
            // FUN_01d50530 - IsConnectedTest (called from slot 8 SetCaptureParameters)
            {"0x1d50530", "IsConnectedTest"},
            // FUN_01d21c90 - called from StartSampling after connection test
            {"0x1d21c90", "PreCapture_CommonDeviceInit"},
            // FUN_01d24ca0 - called from StartSampling with param_2 (DeviceSettings)
            {"0x1d24ca0", "SetCaptureParameters"},
            // FUN_01d50310 - called from StartSampling (captures enabled)
            {"0x1d50310", "PrepareCapture"},
            // FUN_01d3ffb0 - creates the read thread
            {"0x1d3ffb0", "CreateReadThread"},
            // FUN_01d3f150 - callback from StartSampling (buffer callback?)
            {"0x1d3f150", "BufferCallback"},
            // FUN_01d3f350 - another callback from StartSampling
            {"0x1d3f350", "ErrorCallback"},
            // FUN_01d4c000 - channel config setup (called 8 times for 8 channels)
            {"0x1d4c000", "AddChannelConfig"},
            // FUN_01d4c1e0 - set USB speed on DeviceSettings
            {"0x1d4c1e0", "SetUsbSpeed"},
            // FUN_01d4c2a0 - build sample rate options from rate arrays
            {"0x1d4c2a0", "BuildSampleRateOptions"},
            // FUN_01d509a0 - FpgaDeviceFeatures constructor (passed USB endpoints)
            {"0x1d509a0", "FpgaDeviceFeatures_ctor"},
            // FUN_01d53440 - another features constructor
            {"0x1d53440", "SecondFeatures_ctor"},
            // FUN_01d51a30 - ReadDeviceId
            {"0x1d51a30", "ReadDeviceId"},
        };

        DecompInterface decomp = new DecompInterface();
        DecompileOptions options = new DecompileOptions();
        decomp.setOptions(options);
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        for (String[] entry : entries) {
            String addrStr = entry[0];
            String label = entry[1];
            Address addr = af.getAddress(addrStr);
            if (addr == null) {
                println("--- INVALID ADDRESS: " + addrStr + " (" + label + ") ---");
                continue;
            }
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                func = createFunction(addr, label);
                if (func == null) {
                    println("--- NO FUNCTION at " + addrStr + " (" + label + ") ---");
                    continue;
                }
            }
            long elfVa = func.getEntryPoint().getOffset() - 0x100000;
            println("=== " + label + " @ Ghidra:" + func.getEntryPoint() +
                    " ELF:0x" + Long.toHexString(elfVa) +
                    " (size=" + func.getBody().getNumAddresses() + " bytes) ===");
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
