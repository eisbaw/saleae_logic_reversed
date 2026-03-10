// Decompile sample rate configuration functions
// Addresses are Ghidra-rebased (ELF VA + 0x100000)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileSampleRate extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Addresses are Ghidra-rebased (ELF VA + 0x100000)
        //
        // Key functions identified via assertion string xrefs:
        //
        // SampleRateOption accessors (from "attempt to read un-set SampleRateOption" xrefs)
        //   ELF 0x1c47940 -> Ghidra 0x1d47940: SampleRateOption validate/accessor
        //
        // SampleRateOption builder (from device-settings.md)
        //   ELF 0x1c47b30 -> Ghidra 0x1d47b30: SampleRateOption builder function
        //
        // GetDeviceSettings implementations
        //   ELF 0x1c3e0a0 -> Ghidra 0x1d3e0a0: LogicGraduateDevice::GetDeviceSettings
        //   ELF 0x1c41c50 -> Ghidra 0x1d41c50: LogicProfessionalDevice::GetDeviceSettings
        //
        // DeviceSettings performance option methods (from the 0x1c49xxx cluster)
        //   ELF 0x1c49710 -> Ghidra 0x1d49710: big function with GetAnalogBasic + GetPerformanceInterfaceType
        //   ELF 0x1c49ec0 -> Ghidra 0x1d49ec0: function with push rbp prologue
        //   ELF 0x1c4a140 -> Ghidra 0x1d4a140: GetAnalogBasicPerformanceOptions xref 2
        //   ELF 0x1c4a250 -> Ghidra 0x1d4a250: prologue before builder area
        //   ELF 0x1c4ac00 -> Ghidra 0x1d4ac00: another function start
        //   ELF 0x1c4ad60 -> Ghidra 0x1d4ad60: SetAnalogBasicPerformanceOption
        //   ELF 0x1c4af40 -> Ghidra 0x1d4af40: function with push rbp
        //   ELF 0x1c4b130 -> Ghidra 0x1d4b130: function start
        //   ELF 0x1c4b3f0 -> Ghidra 0x1d4b3f0: function with push rbp
        //
        // HandleSetSampleRate
        //   ELF 0x1be4e10 -> Ghidra 0x1ce4e10
        //
        // LogicStudentDevice performance options
        //   ELF 0x1c56310 -> Ghidra 0x1d56310: LogicStudentDevice performance options
        //
        // SampleRate should never be 0
        //   ELF 0x2db1beb -> Ghidra 0x2eb1beb: xref in far-away function

        String[][] targets = new String[][] {
            // SampleRateOption struct methods
            {"01d47940", "SampleRateOption_accessor"},
            {"01d47b30", "SampleRateOption_builder"},

            // LogicGraduateDevice (Logic 8) GetDeviceSettings
            {"01d3e0a0", "LogicGraduateDevice_GetDeviceSettings"},

            // LogicProfessionalDevice GetDeviceSettings
            {"01d41c50", "LogicProfessionalDevice_GetDeviceSettings"},

            // Large function cluster: DeviceSettings performance options
            {"01d49710", "DeviceSettings_perf_func_1"},
            {"01d49e40", "DeviceSettings_GetPerformanceInterfaceType_helper"},
            {"01d49ec0", "DeviceSettings_perf_func_2"},
            {"01d4a140", "DeviceSettings_GetAnalogBasic_wrapper"},
            {"01d4a250", "DeviceSettings_build_options"},
            {"01d4ac00", "DeviceSettings_perf_func_3"},
            {"01d4ad60", "DeviceSettings_SetAnalogBasic"},
            {"01d4af40", "DeviceSettings_SetDigitalOnly_area"},
            {"01d4b130", "DeviceSettings_SetAnalogAdvanced_area"},
            {"01d4b3f0", "DeviceSettings_perf_func_4"},

            // HandleSetSampleRate
            {"01ce4e10", "HandleSetSampleRate"},

            // LogicStudentDevice (Logic 4) performance options
            {"01d56310", "LogicStudentDevice_perf_options"},

            // LogicProfessional8Device GetDeviceSettings (from device-settings.md: 0x1c449d0)
            {"01d449d0", "LogicProfessional8Device_GetDeviceSettings"},
            // LogicProfessional16Device GetDeviceSettings (from device-settings.md: 0x1c44750)
            {"01d44750", "LogicProfessional16Device_GetDeviceSettings"},
            // LogicStudentDevice GetDeviceSettings (from device-settings.md: 0x1c57980)
            {"01d57980", "LogicStudentDevice_GetDeviceSettings"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Create functions where missing
        int tx = currentProgram.startTransaction("Create sample rate functions");
        try {
            for (String[] t : targets) {
                Address addr = af.getAddress(t[0]);
                Function func = fm.getFunctionAt(addr);
                if (func == null) {
                    Function containing = fm.getFunctionContaining(addr);
                    if (containing != null) {
                        println("NOTE: " + t[0] + " (" + t[1] + ") inside " + containing.getName()
                            + " @ " + containing.getEntryPoint());
                    } else {
                        CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                        cmd.applyTo(currentProgram);
                        func = fm.getFunctionAt(addr);
                        if (func != null) {
                            func.setName(t[1], SourceType.USER_DEFINED);
                            println("CREATED: " + t[1] + " @ " + t[0]);
                        } else {
                            println("FAILED to create function: " + t[0] + " (" + t[1] + ")");
                        }
                    }
                } else {
                    println("EXISTS: " + func.getName() + " @ " + t[0]);
                }
            }
        } finally {
            currentProgram.endTransaction(tx, true);
        }

        // Decompile all targets
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            Function func = fm.getFunctionAt(addr);
            if (func == null) func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("=== NO FUNCTION: " + t[0] + " (" + t[1] + ") ===\n");
                continue;
            }
            println("=== " + func.getName() + " @ " + func.getEntryPoint()
                + " (" + t[1] + ") ===");
            DecompileResults res = decomp.decompileFunction(func, 180, monitor);
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
