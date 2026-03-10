// Decompile ComputeTransferSize, AdjustTransferSize, and related functions
// per device class to extract transfer sizing formulas
// Addresses are Ghidra-rebased (ELF VA + 0x100000)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileTransferSize extends GhidraScript {
    @Override
    public void run() throws Exception {
        // ELF VA + 0x100000 = Ghidra address
        //
        // Vtable slot 10 per device (candidate for ComputeTransferSize or StartSampling):
        //   LogicGraduateDevice:      ELF 0x1c3e5b0 -> Ghidra 0x1d3e5b0
        //   LogicStudentDevice:       ELF 0x1c56810 -> Ghidra 0x1d56810
        //   LogicProfessionalDevice:  ELF 0x1c41eb0 -> Ghidra 0x1d41eb0
        //   (Pro8 and Pro16 inherit from LogicProfessionalDevice slot 10)
        //
        // Vtable slot 9 per device:
        //   LogicGraduateDevice:      ELF 0x1c3e0a0 -> Ghidra 0x1d3e0a0
        //   LogicStudentDevice:       ELF 0x1c56310 -> Ghidra 0x1d56310
        //   LogicProfessionalDevice:  ELF 0x1c418c0 -> Ghidra 0x1d418c0
        //
        // AdjustTransferSize function: ELF 0x1c4fc50 -> Ghidra 0x1d4fc50
        //   Contains: "AdjustTransferSize", "ideal transfer size", "capping transfer size"
        //   Called from ComputeTransferSize
        //
        // Function containing ComputeTransferSize assertion:
        //   ELF 0x1c3bef0 -> Ghidra 0x1d3bef0
        //   Contains assertion "virtual U32 LogicDevice::ComputeTransferSize(U64)"
        //   This is likely the base ComputeTransferSize or a caller
        //
        // StartSampling-related functions:
        //   Slot 6 (LogicGraduateDevice):    ELF 0x1c3d970 -> Ghidra 0x1d3d970
        //   Slot 7 (LogicGraduateDevice):    ELF 0x1c3d990 -> Ghidra 0x1d3d990
        //   Slot 8 (LogicGraduateDevice):    ELF 0x1c3d9a0 -> Ghidra 0x1d3d9a0
        //
        // ReadStream_QueueBuffersThread call site (in LinuxUsbDevice):
        //   ELF 0x02d939d9 xref "Starting bulk read thread" -> function around 0x2d93800
        //   Ghidra: 0x2e93800 area
        //
        // SetOverrideUsb2PowerLimit:
        //   String at 0x355882
        //
        // USB speed detection function:
        //   ELF 0x2d9b820 area -> Ghidra 0x2e9b820 area

        String[][] targets = new String[][] {
            // === ComputeTransferSize per device (slot 10) ===
            {"01d3e5b0", "Graduate_slot10_ComputeTransferSize_or_StartSampling"},
            {"01d56810", "Student_slot10_ComputeTransferSize_or_StartSampling"},
            {"01d41eb0", "Professional_slot10_contains_TransferSizeLog"},

            // === Vtable slot 9 per device ===
            {"01d3e0a0", "Graduate_slot9"},
            {"01d56310", "Student_slot9"},
            {"01d418c0", "Professional_slot9"},

            // === AdjustTransferSize ===
            {"01d4fc50", "AdjustTransferSize"},

            // === Base ComputeTransferSize (assertion holder) ===
            {"01d3bef0", "LogicDevice_ComputeTransferSize_base_or_caller"},

            // === Shared virtual methods ===
            {"01d4f910", "shared_slot3_all_devices"},
            {"01d4e7c0", "shared_slot4_all_devices"},
            {"01d4fa90", "shared_slot5_all_devices"},
            {"01d50230", "shared_slot11_all_devices"},

            // === Additional per-device overrides ===
            // Slot 1 (Pro8 vs Pro16 differ)
            {"01d45410", "Pro8_slot1"},
            {"01d453d0", "Pro16_slot1"},

            // === TransferQueue construction context ===
            {"02e938c0", "ReadStream_QueueBuffersThread_area"},

            // === USB speed detection ===
            {"02e9b800", "USB_speed_area"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Create functions where missing
        int tx = currentProgram.startTransaction("Create transfer size functions");
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
            DecompileResults res = decomp.decompileFunction(func, 300, monitor);
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
