// Decompile EEPROM-related functions: device ID read, hardware revision,
// ParseBoardRevision, CommonDeviceInit, TestEeprom, WipeEeprom, SetIdToDevice,
// ReadEeprom, WriteEeprom, GetFirmwareVersion
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileEeprom extends GhidraScript {
    @Override
    public void run() throws Exception {
        // All addresses are Ghidra VAs (ELF VA + 0x100000)
        // Grouped by functional area
        String[][] groups = new String[][] {
            // Group 1: CommonDeviceInit and ParseBoardRevision
            {"CommonDeviceInit region", "0x1d2273f"},
            {"ParseBoardRevision region", "0x1d23aad"},
            {"Unsupported board revision for Ad9637", "0x1d2369c"},

            // Group 2: Device ID reading per device type
            {"Logic16 Device ID read", "0x1d1a6bf"},
            {"Logic Device ID read (FX2)", "0x1d39bfb"},
            {"Logic 8 Device ID read", "0x1d3d4ef"},
            {"Logic 8 Device ID read failure", "0x1d3d471"},
            {"Logic 8 Device ID complete failure", "0x1d3d551"},
            {"Logic Pro Device ID read", "0x1d410e4"},
            {"Logic Student Device ID read", "0x1d55ba1"},
            {"Hardware revision (Pro)", "0x1d3d8c9"},
            {"Hardware revision (other)", "0x1d413a9"},

            // Group 3: Firmware build date / version
            {"Firmware build date", "0x1d41056"},

            // Group 4: EEPROM operations
            {"TestEeprom", "0x1d44211"},
            {"EEprom error detected", "0x1d441e4"},
            {"WipeEeprom", "0x1d50852"},
            {"SetIdToDevice failed", "0x1d51b0a"},
            {"eeprom is only 256 bytes", "0x1d50be6"},

            // Group 5: FX3 ReadEeprom/WriteEeprom (from i2c-eeprom-protocol.md)
            {"Fx3 WriteEeprom (opcode 0x55)", "0x1d51fee"},
            {"Fx3 ReadEeprom", "0x1d52500"},
            {"Fx2 WriteEeprom", "0x1d50b4d"},
            {"Fx2 ReadEeprom region", "0x1d50e00"},

            // Group 6: Getting hardware revision (MSO/FX3 path)
            {"Getting hardware revision", "0x2e6281a"},
            {"GetFirmwareVersion", "0x2e348ab"},

            // Group 7: Board revision detail
            {"Device hardware revision not supported", "0x2e9a8db"},

            // Group 8: /deviceId and /hardwareRevision JSON path handlers
            {"/deviceId handler 1", "0x1cddbfe"},
            {"/deviceId handler 2", "0x1cddc87"},
            {"/deviceId handler 3", "0x1cddea5"},
            {"/hardwareRevision handler", "0x1ce04de"},
            {"/boardRevision handler 1", "0x2e80153"},
            {"/boardRevision handler 2", "0x2e801d0"},
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        // Increase timeout for large functions
        decomp.setSimplificationStyle("decompile");

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
                println("  Trying to create function...");
                // Try nearby addresses
                for (int delta = -0x100; delta <= 0; delta += 0x10) {
                    Address tryAddr = addr.add(delta);
                    func = fm.getFunctionContaining(tryAddr);
                    if (func != null) {
                        println("  Found function at offset " + delta + ": " + func.getEntryPoint());
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
                String code = res.getDecompiledFunction().getC();
                println(code);
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("---END---\n");
        }
        decomp.dispose();
    }
}
