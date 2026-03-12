// Decompile the vtable[1] (Deserialize) functions from SerializeConverter specializations
// These are the actual functions that read JSON fields
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

import java.util.*;

public class DecompileSerializeConverter4 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // vtable[1] functions from Phase 2 output (Ghidra VAs)
        // These are the Deserialize virtual methods
        String[][] funcs = new String[][] {
            {"0x1cf84b0", "LogicDevice::StartCapture Deserialize"},
            {"0x1cf8340", "LogicDevice::SetSampleRate Deserialize"},
            {"0x1cf8520", "LogicDevice::StopCapture Deserialize"},
            {"0x1cf8540", "LogicDevice::SetAdcFullScaleGain Deserialize"},
            {"0x1cf8430", "LogicDevice::SetLed Deserialize"},
        };

        // Also, let me find the missing vtables by reading more constructor bodies.
        // First, let me find the SettingResponse, EnableChannels, SetDigitalVoltageThreshold,
        // StartCaptureResponse, StopCaptureResponse, LogicDeviceState vtable addresses
        // by reading the raw vtable area near 0x2f45a08 (which is where SetSampleRate vtable was)

        // Additional vtable[1] functions to find:
        // SettingResponse constructor at 0x1cf3530 (203 bytes - bigger, has complex serialize)
        // EnableChannels constructor at 0x1cf36d0
        // etc.

        DecompInterface decomp = new DecompInterface();
        DecompileOptions opts = new DecompileOptions();
        decomp.setOptions(opts);
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        Set<Address> decompiled = new HashSet<>();

        println("// ================================================================");
        println("// SerializeConverter Deserialize methods");
        println("// These read JSON fields using RapidJSON Pointer paths");
        println("// ================================================================");

        for (String[] entry : funcs) {
            Address addr = af.getAddress(entry[0]);
            Function func = fm.getFunctionAt(addr);
            if (func == null) func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("// No function at " + entry[0] + " (" + entry[1] + ")");
                continue;
            }

            long ghidraVA = func.getEntryPoint().getOffset();
            long elfVA = ghidraVA - 0x100000;

            println("");
            println("// ================================================================");
            println("// " + entry[1]);
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

        // Now also decompile the bigger constructors that may contain inline serialize code
        // The SettingResponse (203 bytes), StopCaptureResponse (203 bytes), and
        // LogicDeviceState (335 bytes) constructors are bigger than average
        String[][] bigConstructors = new String[][] {
            {"0x1cf3530", "LogicDevice::SettingResponse constructor (203 bytes)"},
            {"0x1cf3980", "LogicDevice::StopCaptureResponse constructor (203 bytes)"},
            {"0x1cf3b40", "LogicDeviceState / AcquireLogicDeviceResult constructor (335 bytes)"},
            {"0x1cf33d0", "LogicDevice::LogicDeviceState constructor (100 bytes)"},
        };

        println("");
        println("// ================================================================");
        println("// Larger constructors (may contain inline serialize setup)");
        println("// ================================================================");

        for (String[] entry : bigConstructors) {
            Address addr = af.getAddress(entry[0]);
            Function func = fm.getFunctionAt(addr);
            if (func == null) func = fm.getFunctionContaining(addr);
            if (func == null) continue;

            long ghidraVA = func.getEntryPoint().getOffset();
            long elfVA = ghidraVA - 0x100000;

            println("");
            println("// ================================================================");
            println("// " + entry[1]);
            println("// Ghidra VA: 0x" + Long.toHexString(ghidraVA));
            println("// ELF VA: 0x" + Long.toHexString(elfVA));
            println("// Size: " + func.getBody().getNumAddresses() + " bytes");
            println("// ================================================================");

            DecompileResults res = decomp.decompileFunction(func, 60, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            }
        }

        // Also decompile the handlers that CONSUME these types
        // HandleSetSampleRate, HandleSamplingConfigurationData, etc
        // These will reveal the actual fields accessed

        // Search for HandleSetSampleRate
        println("");
        println("// ================================================================");
        println("// Handler functions that consume these data types");
        println("// ================================================================");

        // HandleSetSampleRate string at ELF 0x42d341 -> Ghidra 0x52d341
        // Let me find xrefs to the string
        String[] handlerStrings = new String[] {
            "HandleSetSampleRate",
            "HandleSamplingConfigurationData",
        };

        for (String str : handlerStrings) {
            println("// Searching for: " + str);
        }

        decomp.dispose();
        println("// === END ===");
    }
}
