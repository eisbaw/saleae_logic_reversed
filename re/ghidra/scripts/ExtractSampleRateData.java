// Extract global data values used by sample rate functions
//@category RE
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.*;

public class ExtractSampleRateData extends GhidraScript {
    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        AddressFactory af = currentProgram.getAddressFactory();

        // Helper: read 4 bytes as int
        // Note: Ghidra addresses are rebased +0x100000

        println("=== GetPerformanceInterfaceType lookup table ===");
        // DAT_005748cc in ELF = 0x005748cc + 0x100000 = 0x006748cc in Ghidra
        for (int i = 0; i < 9; i++) {
            Address addr = af.getAddress(String.format("%08x", 0x006748cc + i * 4));
            int val = mem.getInt(addr);
            println("  [" + i + "] = " + val);
        }

        println("");
        println("=== USB max bit rate per speed ===");
        // DAT_005748f0 -> Ghidra 0x006748f0
        for (int i = 0; i < 3; i++) {
            Address addr = af.getAddress(String.format("%08x", 0x006748f0 + i * 8));
            long val = mem.getLong(addr);
            println("  [" + i + "] = " + val + " (0x" + Long.toHexString(val) + ")");
        }

        println("");
        println("=== LogicGraduateDevice divisor constants ===");
        // DAT_0044e3b8 -> Ghidra 0x0054e3b8
        Address addr = af.getAddress("0054e3b8");
        long bits = mem.getLong(addr);
        double dval = Double.longBitsToDouble(bits);
        println("  DAT_0044e3b8 = " + dval + " (" + (long)dval + ")");

        addr = af.getAddress("0054e278");
        bits = mem.getLong(addr);
        dval = Double.longBitsToDouble(bits);
        println("  DAT_0044e278 = " + dval + " (" + (long)dval + ")");

        println("");
        println("=== LogicProfessionalDevice divisor constants ===");
        addr = af.getAddress("0054d9d0");
        bits = mem.getLong(addr);
        dval = Double.longBitsToDouble(bits);
        println("  DAT_0044d9d0 = " + dval + " (" + (long)dval + ")");

        addr = af.getAddress("0054e2e8");
        bits = mem.getLong(addr);
        dval = Double.longBitsToDouble(bits);
        println("  DAT_0044e2e8 = " + dval + " (" + (long)dval + ")");

        println("");
        println("=== Channel config constants ===");
        addr = af.getAddress("0054d4cc");
        println("  DAT_0044d4cc = " + mem.getInt(addr) + " (0x" + Integer.toHexString(mem.getInt(addr)) + ")");
        addr = af.getAddress("0054d4dc");
        println("  DAT_0044d4dc = " + mem.getInt(addr) + " (0x" + Integer.toHexString(mem.getInt(addr)) + ")");
        addr = af.getAddress("0054d4d4");
        println("  DAT_0044d4d4 = " + mem.getInt(addr) + " (0x" + Integer.toHexString(mem.getInt(addr)) + ")");

        println("");
        println("=== Sample rate arrays in .rodata ===");

        // From search: rate array at VA 0x471a28 (Ghidra: 0x571a28)
        println("Array at 0x471a28 (digital rates for Pro devices):");
        for (int i = 0; i < 20; i++) {
            addr = af.getAddress(String.format("%08x", 0x571a28 + i * 4));
            int val = mem.getInt(addr);
            if (val > 0 && val <= 1000000000) {
                println("  [" + i + "] = " + val + " (" + (val / 1000000.0) + " MHz)");
            } else {
                println("  [" + i + "] = " + val);
            }
        }

        // Array at 0x471a64 (could be analog rates)
        println("");
        println("Array at 0x471a64 (analog rates for Pro devices?):");
        for (int i = 0; i < 20; i++) {
            addr = af.getAddress(String.format("%08x", 0x571a64 + i * 4));
            int val = mem.getInt(addr);
            if (val > 0 && val <= 1000000000) {
                println("  [" + i + "] = " + val + " (" + (val / 1000000.0) + " MHz)");
            } else {
                println("  [" + i + "] = " + val);
            }
        }

        // Array at 0x472220 (second copy of digital rates)
        println("");
        println("Array at 0x472220 (digital rates, second copy):");
        for (int i = 0; i < 20; i++) {
            addr = af.getAddress(String.format("%08x", 0x572220 + i * 4));
            int val = mem.getInt(addr);
            if (val > 0 && val <= 1000000000) {
                println("  [" + i + "] = " + val + " (" + (val / 1000000.0) + " MHz)");
            } else {
                println("  [" + i + "] = " + val);
            }
        }

        // Array at 0x472260 (second copy of analog rates?)
        println("");
        println("Array at 0x472260 (analog rates, second copy?):");
        for (int i = 0; i < 20; i++) {
            addr = af.getAddress(String.format("%08x", 0x572260 + i * 4));
            int val = mem.getInt(addr);
            if (val > 0 && val <= 1000000000) {
                println("  [" + i + "] = " + val + " (" + (val / 1000000.0) + " MHz)");
            } else {
                println("  [" + i + "] = " + val);
            }
        }

        // Check what the LogicStudentDevice builder refs actually point to
        println("");
        println("=== LogicStudentDevice builder - DAT refs (Ghidra-rebased) ===");
        addr = af.getAddress("00c71b00");  // DAT_00b71b00 + 0x100000
        println("  DAT_00b71b00 raw bytes:");
        byte[] bytes = new byte[16];
        mem.getBytes(addr, bytes);
        StringBuilder sb = new StringBuilder();
        for (byte b : bytes) sb.append(String.format("%02x ", b & 0xff));
        println("    " + sb.toString());
        println("    as u32: " + mem.getInt(addr));

        addr = af.getAddress("006b8d80");  // DAT_005b8d80 + 0x100000
        println("  DAT_005b8d80 raw bytes:");
        mem.getBytes(addr, bytes);
        sb = new StringBuilder();
        for (byte b : bytes) sb.append(String.format("%02x ", b & 0xff));
        println("    " + sb.toString());
        println("    as u32: " + mem.getInt(addr));

        // Look at the rate arrays that are pointed to by runtime pointers
        // DAT_0307aa30 -> Ghidra 0x0407aa30 (in .bss, populated at runtime)
        // These won't be readable from the binary. Instead, the actual arrays
        // are constructed in the GetDeviceSettings functions.

        // Look at the analog rate pair arrays
        println("");
        println("=== Analog rate pair arrays (8-byte entries: physical_rate, output_rate) ===");

        // Search near the digital rate arrays for analog pairs
        // After 0x471a28+15*4 = 0x471a64, check for analog rates
        println("Array at 0x471a64:");
        for (int i = 0; i < 12; i++) {
            addr = af.getAddress(String.format("%08x", 0x571a64 + i * 4));
            int val = mem.getInt(addr);
            println("  [" + i + "] = " + val + " (" + (val / 1000000.0) + " MHz)");
        }

        // Rate arrays at 0x4742b0
        println("");
        println("Array at 0x4742b0 (third location):");
        for (int i = 0; i < 15; i++) {
            addr = af.getAddress(String.format("%08x", 0x5742b0 + i * 4));
            int val = mem.getInt(addr);
            if (val > 0 && val <= 1000000000) {
                println("  [" + i + "] = " + val + " (" + (val / 1000000.0) + " MHz)");
            } else {
                println("  [" + i + "] = " + val);
            }
        }

        // Check rate array at 0x473ce0
        println("");
        println("Array at 0x473ce0:");
        for (int i = 0; i < 15; i++) {
            addr = af.getAddress(String.format("%08x", 0x573ce0 + i * 4));
            int val = mem.getInt(addr);
            if (val > 0 && val <= 1000000000) {
                println("  [" + i + "] = " + val + " (" + (val / 1000000.0) + " MHz)");
            } else {
                println("  [" + i + "] = " + val);
            }
        }
    }
}
