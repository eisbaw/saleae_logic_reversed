// Decompile the INNER SerializeConverter functions that do actual JSON serialization
// These are the functions called by the thin wrappers found in the first pass
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

import java.util.*;

public class DecompileSerializeConverter2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Inner functions called by SerializeConverter wrappers (Ghidra VAs)
        // Format: {Ghidra VA, type name, purpose}
        String[][] funcs = new String[][] {
            // CalibrationData
            {"0x2db7460", "CalibrationData", "type name factory"},
            {"0x2db71c0", "CalibrationData", "wrapper (78 bytes)"},
            // SamplingConfigurationData
            {"0x2db06c0", "SamplingConfigurationData", "wrapper (82 bytes)"},
            // LogicDevice types - inner serialization functions
            {"0x1cf66d0", "LogicDevice::SetSampleRate", "serialize"},
            {"0x1cf6960", "LogicDevice::EnableChannels", "serialize"},
            {"0x1cf7810", "LogicDevice::StartCapture", "serialize"},
            {"0x1cf65d0", "LogicDevice::SettingResponse", "serialize"},
            {"0x1cf7a10", "LogicDevice::StopCapture", "serialize"},
            {"0x1cf6f10", "LogicDevice::SetDigitalVoltageThreshold", "serialize"},
            {"0x1cf74d0", "LogicDevice::StartCaptureResponse", "serialize"},
            {"0x1cf7910", "LogicDevice::StopCaptureResponse", "serialize"},
            {"0x1cf7b10", "LogicDevice::SetAdcFullScaleGain", "serialize"},
            {"0x1cf7240", "LogicDevice::SetLed", "serialize"},
            {"0x1cf7ef0", "LogicDeviceState / AcquireLogicDeviceResult", "serialize"},
            {"0x1cf6300", "LogicDevice::LogicDeviceState", "serialize"},
        };

        DecompInterface decomp = new DecompInterface();
        DecompileOptions opts = new DecompileOptions();
        decomp.setOptions(opts);
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        println("// ================================================================");
        println("// Inner SerializeConverter functions (actual serialization logic)");
        println("// These contain the JSON field names and data layout");
        println("// ================================================================");

        for (String[] entry : funcs) {
            String addrStr = entry[0];
            String typeName = entry[1];
            String purpose = entry[2];

            Address addr = af.getAddress(addrStr);
            if (addr == null) {
                println("// ERROR: bad address " + addrStr);
                continue;
            }

            Function func = fm.getFunctionAt(addr);
            if (func == null) {
                // Try to find containing function
                func = fm.getFunctionContaining(addr);
                if (func == null) {
                    println("// No function at " + addrStr + " (" + typeName + ")");
                    continue;
                }
            }

            long ghidraVA = func.getEntryPoint().getOffset();
            long elfVA = ghidraVA - 0x100000;

            println("");
            println("// ================================================================");
            println("// " + typeName + " -- " + purpose);
            println("// Ghidra VA: 0x" + Long.toHexString(ghidraVA));
            println("// ELF VA: 0x" + Long.toHexString(elfVA));
            println("// Size: " + func.getBody().getNumAddresses() + " bytes");
            println("// ================================================================");

            DecompileResults res = decomp.decompileFunction(func, 60, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("// DECOMPILE FAILED: " + res.getErrorMessage());
            }
        }

        decomp.dispose();
        println("// === END ===");
    }
}
