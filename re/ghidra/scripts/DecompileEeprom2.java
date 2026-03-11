// Decompile additional EEPROM-related functions: ReadDeviceId, EepromSizeMax, EepromPageSize
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileEeprom2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // FUN_01d51a30 -- called in device constructors to read device ID from EEPROM
        // This is the function that calls ReadEeprom and returns the 8-byte device ID
        // Also decompile the ReadDeviceId wrapper and EepromSizeMax/EepromPageSize vtable methods
        String[][] groups = new String[][] {
            // ReadDeviceId -- called by device constructors
            {"ReadDeviceId (FUN_01d51a30)", "0x1d51a30"},
            // SetIdToDevice (already have, but want the caller too)
            {"SetIdToDevice (FUN_01d51a50)", "0x1d51a50"},
            // The vtable dispatch for ReadEeprom: param_1[0x10] is at offset 0x80
            // FUN_01d509a0 -- FpgaDeviceFeatures constructor
            {"FpgaDeviceFeatures ctor (FUN_01d509a0)", "0x1d509a0"},
            // FUN_01d51e10 -- ProFpgaDeviceFeatures constructor
            {"ProFpgaDeviceFeatures ctor (FUN_01d51e10)", "0x1d51e10"},
            // FUN_01d53440 -- another DeviceFeatures constructor?
            {"DeviceFeatures ctor 2 (FUN_01d53440)", "0x1d53440"},
            // FUN_01d54890 -- another DeviceFeatures constructor?
            {"DeviceFeatures ctor 3 (FUN_01d54890)", "0x1d54890"},
            // FUN_01d43660 -- called by TestEeprom to read back data
            {"TestEeprom ReadBack (FUN_01d43660)", "0x1d43660"},
            // FUN_01d201f0 -- called early in constructors (timing/init?)
            {"Device init helper (FUN_01d201f0)", "0x1d201f0"},
            // ReadEeprom callers -- the actual EEPROM read via I2C
            // FUN_01d296c0 -- referenced in CommonDeviceInit
            {"CommonDeviceInit helper (FUN_01d296c0)", "0x1d296c0"},
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        for (String[] group : groups) {
            String label = group[0];
            String addrStr = group[1];

            Address addr = af.getAddress(addrStr);
            if (addr == null) {
                println("--- INVALID ADDRESS: " + addrStr + " (" + label + ") ---");
                continue;
            }
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("--- NO FUNCTION at " + addrStr + " (" + label + ") ---");
                for (int delta = -0x100; delta <= 0; delta += 0x10) {
                    Address tryAddr = addr.add(delta);
                    func = fm.getFunctionContaining(tryAddr);
                    if (func != null) {
                        println("  Found at offset " + delta + ": " + func.getEntryPoint());
                        break;
                    }
                }
                if (func == null) {
                    println("  No function found nearby.\n");
                    continue;
                }
            }

            long elfVa = func.getEntryPoint().getOffset() - 0x100000;
            long funcSize = func.getBody().getNumAddresses();
            println("=== " + label + " ===");
            println("  Function: " + func.getName());
            println("  Ghidra VA: " + func.getEntryPoint());
            println("  ELF VA: 0x" + Long.toHexString(elfVa));
            println("  Size: " + funcSize + " bytes");
            println("");

            DecompileResults res = decomp.decompileFunction(func, 180, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("---END---\n");
        }
        decomp.dispose();
    }
}
