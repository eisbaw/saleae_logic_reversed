// Read vtables of SerializeConverter specializations and decompile their virtual methods
// The vtable contains the actual Serialize/Deserialize function pointers
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.*;

import java.util.*;

public class DecompileSerializeConverter3 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Vtable addresses found in constructors (*param_1 = &DAT_XXXXXXXX)
        // These are Ghidra VAs. Read 8 pointers from each vtable to find virtual methods.
        String[][] vtables = new String[][] {
            // From FUN_01cf3640: SetSampleRate constructor sets *param_1 = &DAT_02f45a08
            {"0x2f45a08", "LogicDevice::SetSampleRate"},
        };

        DecompInterface decomp = new DecompInterface();
        DecompileOptions opts = new DecompileOptions();
        decomp.setOptions(opts);
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        Memory mem = currentProgram.getMemory();

        Set<Address> decompiled = new HashSet<>();

        // First, let me find ALL the vtable addresses by looking at constructors
        // The pattern is: *param_1 = &DAT_XXXX  (vtable assignment)
        // Let me scan the constructor cluster at 0x1cf3530-0x1cf3c00

        // Read each constructor to find vtable addresses
        // These are the larger (non-12-byte) functions from the first pass
        String[][] constructors = new String[][] {
            {"0x1cf3530", "LogicDevice::SettingResponse"},
            {"0x1cf3640", "LogicDevice::SetSampleRate"},
            {"0x1cf36d0", "LogicDevice::EnableChannels"},
            {"0x1cf3780", "LogicDevice::SetDigitalVoltageThreshold"},
            {"0x1cf37f0", "LogicDevice::SetLed"},
            {"0x1cf3860", "LogicDevice::StartCaptureResponse"},
            {"0x1cf3920", "LogicDevice::StartCapture"},
            {"0x1cf3980", "LogicDevice::StopCaptureResponse"},
            {"0x1cf3a90", "LogicDevice::StopCapture"},
            {"0x1cf3ae0", "LogicDevice::SetAdcFullScaleGain"},
            {"0x1cf3b40", "LogicDevice::LogicDeviceState"},
            {"0x1cf33d0", "LogicDevice::LogicDeviceState (alt)"},
            {"0x2db71c0", "CalibrationData"},
            {"0x2db06c0", "SamplingConfigurationData"},
        };

        println("// ================================================================");
        println("// Phase 1: Read vtable addresses from constructors");
        println("// ================================================================");

        // For each constructor, look at the instructions for LEA to find vtable
        // Use a simpler approach: decompile and look for the DAT_ reference
        Map<String, Long> vtableMap = new HashMap<>();

        for (String[] entry : constructors) {
            Address cAddr = af.getAddress(entry[0]);
            Function func = fm.getFunctionAt(cAddr);
            if (func == null) {
                func = fm.getFunctionContaining(cAddr);
            }
            if (func == null) continue;

            DecompileResults res = decomp.decompileFunction(func, 30, monitor);
            if (!res.decompileCompleted()) continue;

            String code = res.getDecompiledFunction().getC();
            // Look for DAT_XXXXXXXX pattern (vtable assignment)
            int idx = code.indexOf("DAT_0");
            while (idx >= 0) {
                // Extract the address
                int end = idx + 4; // past DAT_
                while (end < code.length() && ((code.charAt(end) >= '0' && code.charAt(end) <= '9') ||
                       (code.charAt(end) >= 'a' && code.charAt(end) <= 'f'))) {
                    end++;
                }
                String datAddr = code.substring(idx + 4, end);
                if (datAddr.length() >= 7) {
                    // This is likely a vtable address
                    vtableMap.put(entry[1], Long.parseLong(datAddr, 16));
                    println("// " + entry[1] + " vtable at 0x" + datAddr);
                    break;
                }
                idx = code.indexOf("DAT_0", idx + 1);
            }
        }

        println("");
        println("// ================================================================");
        println("// Phase 2: Read vtable entries and decompile virtual methods");
        println("// ================================================================");

        for (Map.Entry<String, Long> ve : vtableMap.entrySet()) {
            String typeName = ve.getKey();
            long vtableAddr = ve.getValue();

            println("");
            println("// ================================================================");
            println("// VTABLE for " + typeName + " at 0x" + Long.toHexString(vtableAddr));
            println("// ================================================================");

            // Read up to 12 vtable entries (function pointers, 8 bytes each)
            // Typically: destructor, destructor2, GetTypeName, Serialize, Deserialize, ...
            for (int i = 0; i < 12; i++) {
                Address vAddr = af.getAddress("0x" + Long.toHexString(vtableAddr + i * 8));
                try {
                    long fptr = mem.getLong(vAddr);
                    if (fptr == 0) {
                        println("// vtable[" + i + "] = NULL (end of vtable)");
                        break;
                    }

                    Address funcAddr = af.getAddress("0x" + Long.toHexString(fptr));
                    Function func = fm.getFunctionAt(funcAddr);
                    if (func == null) {
                        func = fm.getFunctionContaining(funcAddr);
                    }

                    String funcName = (func != null) ? func.getName() : "???";
                    long elfVA = fptr - 0x100000;
                    println("// vtable[" + i + "] = 0x" + Long.toHexString(fptr) +
                            " (ELF 0x" + Long.toHexString(elfVA) + ") " + funcName);

                    // Decompile non-trivial functions we haven't seen
                    if (func != null && !decompiled.contains(func.getEntryPoint())) {
                        long size = func.getBody().getNumAddresses();
                        // Skip tiny functions (destructors, type-name getters)
                        if (size > 50) {
                            decompiled.add(func.getEntryPoint());
                            DecompileResults res = decomp.decompileFunction(func, 60, monitor);
                            if (res.decompileCompleted()) {
                                println(res.getDecompiledFunction().getC());
                            }
                        }
                    }
                } catch (Exception e) {
                    println("// vtable[" + i + "] read error: " + e.getMessage());
                    break;
                }
            }
        }

        decomp.dispose();
        println("// === END ===");
    }
}
