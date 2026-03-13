// Extract the AfeConfig_RawHardware AllConfigs table from memory
// Table is at DAT_0089a448 (Ghidra VA), 66 entries, 0x18 bytes each
//@category RE
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.*;

public class ExtractAfeConfigTable extends GhidraScript {
    @Override
    public void run() throws Exception {
        // AllConfigs array at Ghidra VA 0x0089a448
        // 66 entries, stride 0x18 (24 bytes)
        // From the format string and struct analysis:
        // offset 0x00: attn1 (byte/bool)
        // offset 0x01: attn2 (byte/bool)
        // offset 0x02: preamp (short)
        // offset 0x04: ladder (short? from format layout)
        // offset 0x06: filter (short)
        // offset 0x08: ladder_char (char, from GetConfigNumber: param_1[8])
        // offset 0x10: dac (double, 8 bytes, from format: dac={:.6f})
        //
        // But let's just dump raw bytes and interpret

        AddressFactory af = currentProgram.getAddressFactory();
        Memory mem = currentProgram.getMemory();

        Address tableAddr = af.getAddress("0x0089a448");

        int numEntries = 66;
        int entrySize = 0x18; // 24 bytes

        println("=== AfeConfig_RawHardware AllConfigs Table ===");
        println("Table address: 0x0089a448 (Ghidra VA)");
        println("ELF VA: 0x0079a448");
        println("Entries: " + numEntries);
        println("Entry size: " + entrySize + " bytes (0x18)");
        println("");
        println("Format (from struct analysis):");
        println("  byte[0]:   attn1");
        println("  byte[1]:   attn2");
        println("  byte[2-3]: preamp (short)");
        println("  byte[4-5]: ??? (short)");
        println("  byte[6-7]: filter (short)");
        println("  byte[8]:   ladder (char, signed)");
        println("  byte[9-15]: padding/unknown");
        println("  byte[16-23]: dac (double)");
        println("");

        // From the format string output function (FUN_02e4f0c0):
        // local_88[0] = (ulong)*param_1;       // param_1[0] = attn1
        // local_78 = (ulong)param_1[1];         // param_1[1] = attn2
        // local_68 = *(undefined8 *)(param_1 + 0x10);  // offset 0x10 = dac (double)
        // local_48 = (ulong)(uint)(int)(char)param_1[8];  // offset 0x08 = ladder (signed char)
        // local_58 = param_1 + 2;  // offset 0x02 = preamp (via formatter FUN_02e4f1f0)
        // local_38 = param_1 + 6;  // offset 0x06 = filter (via formatter FUN_02e4f420)
        // local_28 = param_1 + 4;  // offset 0x04 = power (via formatter FUN_02e4f650) [note: "pwoer" typo]
        //
        // Format string order: attn1, attn2, dac, preamp, ladder, filter, pwoer
        // But formatters are passed differently. Let's analyze:
        // The format args in local_88 array: attn1, attn2, dac, preamp, ladder, filter, power
        // local_88[0] = attn1 (byte at offset 0)
        // local_78   = attn2 (byte at offset 1)
        // local_68   = dac (double at offset 0x10)
        // local_48   = ladder (signed char at offset 8)
        // local_58   = &param_1[2] = preamp (formatter callback)
        // local_38   = &param_1[6] = filter (formatter callback)
        // local_28   = &param_1[4] = power (formatter callback)

        println("Refined layout based on format function:");
        println("  [0]:    attn1 (bool/byte)");
        println("  [1]:    attn2 (bool/byte)");
        println("  [2-3]:  preamp (enum, 2 bytes)");
        println("  [4-5]:  power (enum, 2 bytes) -- 'pwoer' in original");
        println("  [6-7]:  filter (enum, 2 bytes)");
        println("  [8]:    ladder (int8_t, -20 to 0 in steps of 2)");
        println("  [9-15]: padding");
        println("  [16-23]: dac (double, voltage)");
        println("");

        println("Idx | attn1 | attn2 | preamp | power | filter | ladder | dac");
        println("----+-------+-------+--------+-------+--------+--------+--------");

        for (int i = 0; i < numEntries; i++) {
            Address entryAddr = tableAddr.add(i * entrySize);
            byte[] data = new byte[entrySize];
            mem.getBytes(entryAddr, data);

            int attn1 = data[0] & 0xFF;
            int attn2 = data[1] & 0xFF;
            int preamp = (data[2] & 0xFF) | ((data[3] & 0xFF) << 8);
            int power = (data[4] & 0xFF) | ((data[5] & 0xFF) << 8);
            int filter = (data[6] & 0xFF) | ((data[7] & 0xFF) << 8);
            int ladder = data[8]; // signed

            // Extract double at offset 0x10
            long bits = 0;
            for (int b = 0; b < 8; b++) {
                bits |= ((long)(data[16 + b] & 0xFF)) << (b * 8);
            }
            double dac = Double.longBitsToDouble(bits);

            println(String.format("%3d |   %3d |   %3d |    %3d |   %3d |    %3d |   %4d | %.6f",
                i, attn1, attn2, preamp, power, filter, ladder, dac));
        }

        println("");
        println("=== END TABLE ===");
    }
}
