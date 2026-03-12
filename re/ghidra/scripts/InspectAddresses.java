// Check what Ghidra sees at specific addresses
//@category RE
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.*;

public class InspectAddresses extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] addrs = {
            "02d97ec0", "02d987b0", "02d989e0", "02d98710",
            "02d99290", "02d99310", "02d91950", "02d99c10",
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        Memory mem = currentProgram.getMemory();
        Listing listing = currentProgram.getListing();

        for (String addrStr : addrs) {
            Address addr = af.getAddress(addrStr);
            Function funcAt = fm.getFunctionAt(addr);
            Function funcContaining = fm.getFunctionContaining(addr);
            CodeUnit cu = listing.getCodeUnitAt(addr);

            println("--- " + addrStr + " ---");
            println("  funcAt: " + (funcAt != null ? funcAt.getName() + " @ " + funcAt.getEntryPoint() : "null"));
            println("  funcContaining: " + (funcContaining != null ? funcContaining.getName() + " @ " + funcContaining.getEntryPoint() : "null"));
            println("  codeUnit: " + (cu != null ? cu.getClass().getSimpleName() + " = " + cu.toString() : "null"));

            // Dump 16 bytes
            byte[] bytes = new byte[16];
            try {
                mem.getBytes(addr, bytes);
                StringBuilder sb = new StringBuilder("  bytes: ");
                for (byte b : bytes) sb.append(String.format("%02x ", b & 0xff));
                println(sb.toString());
            } catch (Exception e) {
                println("  bytes: read failed: " + e);
            }

            // Check if it's in defined data
            Data data = listing.getDefinedDataContaining(addr);
            if (data != null) {
                println("  data: " + data.getDataType().getName() + " @ " + data.getAddress() + " len=" + data.getLength());
            }

            // Show instruction at this address
            Instruction inst = listing.getInstructionAt(addr);
            if (inst != null) {
                println("  inst: " + inst.toString());
            } else {
                // Try nearby
                inst = listing.getInstructionContaining(addr);
                if (inst != null) {
                    println("  instContaining: " + inst.toString() + " @ " + inst.getAddress());
                }
            }
        }
    }
}
