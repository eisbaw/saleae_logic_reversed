// Decompile LogicGraduateDevice virtual methods and key referenced functions
// Addresses are Ghidra VAs (ELF VA + 0x100000)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileGraduate extends GhidraScript {
    @Override
    public void run() throws Exception {
        // LogicGraduateDevice vtable entries (ELF VA -> Ghidra VA = +0x100000)
        // slot[0]  = 0x1c3d790 -> destructor D1
        // slot[1]  = 0x1c3d7d0 -> destructor D0
        // slot[2]  = 0x1c3d810 -> unknown (unique to Graduate)
        // slot[3]  = 0x1c4f910 -> shared helper (calls vtable[12])
        // slot[4]  = 0x1c4e7c0 -> GetForcedSpeedFlag (shared)
        // slot[5]  = 0x1c4fa90 -> GetSomeConfig (shared)
        // slot[6]  = 0x1c3d970 -> GetUsbSpeed (unique)
        // slot[7]  = 0x1c3d990 -> unknown (unique)
        // slot[8]  = 0x1c3d9a0 -> unknown (unique)
        // slot[9]  = 0x1c3e0a0 -> PerformanceOptions / GetDeviceSettings
        // slot[10] = 0x1c3e5b0 -> StartSampling
        // slot[11] = 0x1c50230 -> ComputeTransferSize (shared)
        // slot[12] = 0x1c3f700 -> unknown (called by slot[3])
        // slot[13] = 0x1c3f920 -> GetDeviceSettings (unique)
        // slot[14] = 0x1c24310 -> shared
        // slot[15] = 0x1c3fa20 -> unknown (unique)
        // slot[16] = 0x1c3fcf0 -> unknown (unique)
        //
        // Also decompile the constructor and key referenced functions

        String[][] entries = new String[][] {
            // Vtable entries (all unique to Graduate, skip shared ones already documented)
            {"0x1d3d790", "vtable_slot00_destructor_D1"},
            {"0x1d3d7d0", "vtable_slot01_destructor_D0"},
            {"0x1d3d810", "vtable_slot02_unknown"},
            {"0x1d3d970", "vtable_slot06_GetUsbSpeed"},
            {"0x1d3d990", "vtable_slot07_unknown"},
            {"0x1d3d9a0", "vtable_slot08_unknown"},
            {"0x1d3e0a0", "vtable_slot09_PerformanceOptions_or_GetDeviceSettings"},
            {"0x1d3e5b0", "vtable_slot10_StartSampling"},
            {"0x1d3f700", "vtable_slot12_called_by_slot3"},
            {"0x1d3f920", "vtable_slot13_GetDeviceSettings"},
            {"0x1d3fa20", "vtable_slot15_unknown"},
            {"0x1d3fcf0", "vtable_slot16_unknown"},
            // Constructor (from multi-device-differences.md: 0x1c3d1f6)
            {"0x1d3d1f6", "LogicGraduateDevice_constructor"},
            // Shared vtable entries for reference
            {"0x1d4f910", "vtable_slot03_shared_helper"},
            {"0x1d4e7c0", "vtable_slot04_GetForcedSpeedFlag"},
            {"0x1d4fa90", "vtable_slot05_GetSomeConfig"},
            {"0x1d24310", "vtable_slot14_shared"},
            // Also decompile the FakeReadThread binding target seen in strings
            // Logic2FpgaDevice buffer callback bound to Graduate
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
                println("--- NO FUNCTION at " + addrStr + " (" + label + ") ---");
                println("  Trying to create function...");
                func = createFunction(addr, label);
                if (func == null) {
                    println("  Failed to create function at " + addrStr);
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
