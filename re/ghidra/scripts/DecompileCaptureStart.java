// Decompile FpgaDeviceFeatures capture start sequence
// Functions related to StartCapture, SetCaptureParameters, GPIF, registers
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileCaptureStart extends GhidraScript {
    @Override
    public void run() throws Exception {
        // ELF VA + 0x100000 = Ghidra address
        // Grouped by functional area
        String[][] targets = new String[][] {
            // HandleStartCapture - top-level entry point
            {"01ce63c0", "HandleStartCapture"},

            // LogicDevice::StartCapture
            {"01cf38d0", "LogicDevice_StartCapture"},

            // IsFpgaResponsive - checks FPGA ready before capture
            {"01d21fb0", "IsFpgaResponsive_variant1"},
            {"01d22610", "IsFpgaResponsive_variant2"},

            // Logic2FpgaDevice::SetCaptureParameters
            {"01d24f50", "Logic2FpgaDevice_SetCaptureParameters"},
            {"01d25900", "SetCaptureParameters_caller"},

            // SendStartCommand - sends COMMAND_START_READ (0x01)
            {"01d42130", "SendStartCommand_1"},
            {"01d42540", "SendStartCommand_2"},

            // NoValidAnalogGpifWidth - analog/GPIF width validation
            {"01d19fc0", "AnalogGpifWidthValidation"},

            // EnableGpif - GPIF state machine enable
            {"02e62480", "EnableGpif"},

            // FPGA WriteRegister functions (16-bit register writes)
            {"02e62140", "FpgaWriteRegister16"},

            // DigitalDerived configuration
            {"02e61bb0", "DigitalDerivedConfig"},
            {"02e66a10", "SetDigitalDerivedVoltageThresholds"},

            // FpgaDeviceFeatures::WriteRegisters / ReadRegisters implementations
            {"01d538f0", "FpgaDeviceFeatures_WriteRegisters"},
            {"01d53ce0", "FpgaDeviceFeatures_ReadRegisters"},

            // GPIF timeout packet handling
            {"02e138c0", "GpifTimeoutPacketHandler"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Create functions where missing
        int tx = currentProgram.startTransaction("Create capture start functions");
        try {
            for (String[] t : targets) {
                Address addr = af.getAddress(t[0]);
                Function func = fm.getFunctionAt(addr);
                if (func == null) {
                    Function containing = fm.getFunctionContaining(addr);
                    if (containing != null) {
                        println("NOTE: " + t[0] + " (" + t[1] + ") inside " + containing.getName() + " @ " + containing.getEntryPoint());
                    } else {
                        CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                        cmd.applyTo(currentProgram);
                        func = fm.getFunctionAt(addr);
                        if (func != null) {
                            func.setName(t[1], SourceType.USER_DEFINED);
                            println("CREATED: " + t[1] + " @ " + t[0]);
                        } else {
                            println("FAILED to create function at " + t[0] + " (" + t[1] + ")");
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
            println("=== " + func.getName() + " @ " + func.getEntryPoint() + " (label: " + t[1] + ") ===");
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
