// Decompile device-specific slot 6 (USB speed) and the ReadStream functions
// Addresses are Ghidra-rebased (ELF VA + 0x100000)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileTransferSize2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Slot 6 per device (USB speed / some device method):
        //   LogicGraduateDevice:      ELF 0x1c3d970 -> Ghidra 0x1d3d970
        //   LogicStudentDevice:       ELF 0x1c56080 -> Ghidra 0x1d56080
        //   LogicProfessionalDevice:  ELF 0x1c415e0 -> Ghidra 0x1d415e0
        //
        // FUN_01d4bbb0 - extracts bandwidth from DeviceSettings
        // FUN_01d4e850 - called in USB speed area, possibly USB speed getter
        //
        // ReadStream start function (the one that creates TransferQueue)
        // We found "Starting bulk read thread" at VA 0x02d939d9 (ELF)
        // Let me find the function containing it
        //
        // The TransferQueue constructor caller:
        //   ELF 0x02d93800 area -> Ghidra 0x2e93800 area
        //
        // FUN_01d50310 - called right before TransferQueue setup in StartSampling
        // FUN_01d1e790 - called with the packed transfer params
        // FUN_01d50530 - called for non-digital-only path

        String[][] targets = new String[][] {
            // Device-specific slot 6
            {"01d3d970", "Graduate_slot6"},
            {"01d56080", "Student_slot6"},
            {"01d415e0", "Professional_slot6"},

            // Bandwidth extractor
            {"01d4bbb0", "ExtractBandwidth_from_settings"},

            // USB speed getter
            {"01d4e850", "GetUsbSpeed_or_similar"},

            // TransferQueue-related in LinuxUsbDevice
            {"02e93900", "ReadStream_area_near_TransferQueue"},

            // Pre-TransferQueue setup
            {"01d50310", "PreTransferQueueSetup"},
            {"01d1e790", "PackTransferParams"},
            {"01d50530", "ConnectionTest_for_capture"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        int tx = currentProgram.startTransaction("Create transfer size2 functions");
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
