// Decompile DeviceHelpers.cpp functions and surrounding region
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileDeviceHelpers extends GhidraScript {
    @Override
    public void run() throws Exception {
        // DeviceHelpers.cpp functions (Ghidra VA = ELF VA + 0x100000)
        // Known:
        //   DeviceCalculateMixRate at ELF 0x1c19fa0 = Ghidra 0x1c29fa0
        //   DeviceCalculateSamplesCollected at ELF 0x1c1a130 = Ghidra 0x1c2a130
        //   Small thunk at ELF 0x1c19f90 = Ghidra 0x1c29f90
        //   Single-ret function at ELF 0x1c1a280 = Ghidra 0x1c2a280
        //
        // Also scan neighborhood for additional DeviceHelpers functions
        String[] addrs = new String[] {
            // Functions in the DeviceHelpers region
            "0x1c29f90",  // Thunk: loads "/id", jumps   ELF: 0x1c19f90
            "0x1c29fa0",  // DeviceCalculateMixRate       ELF: 0x1c19fa0
            "0x1c2a130",  // DeviceCalculateSamplesCollected ELF: 0x1c1a130
            "0x1c2a280",  // Single-ret function          ELF: 0x1c1a280

            // Known callers of DeviceCalculateMixRate to check context
            "0x1bf7ce7",  // Caller 1                     ELF: 0x1be7ce7
            "0x1c34fb9",  // Caller 2                     ELF: 0x1c24fb9
            "0x1c36c80",  // Caller 3                     ELF: 0x1c26c80
            "0x1c5a6f9",  // Caller 4                     ELF: 0x1c4a6f9

            // Callers of thunk "/id" function
            "0x1bdfd79",  // Caller of thunk 1            ELF: 0x1bcfd79
            "0x1bdfe3e",  // Caller of thunk 2            ELF: 0x1bcfe3e
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        for (String addrStr : addrs) {
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("--- NO FUNCTION at " + addrStr + " ---");
                continue;
            }
            Address entry = func.getEntryPoint();
            long elfVA = entry.getOffset() - 0x100000;
            println("=== " + func.getName() + " @ " + entry +
                    " (ELF: 0x" + Long.toHexString(elfVA) +
                    ", queried: " + addrStr + ") ===");
            println("Size: " + func.getBody().getNumAddresses() + " bytes");
            DecompileResults res = decomp.decompileFunction(func, 60, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("");
        }

        // Also scan the wider neighborhood for function boundaries
        println("=== FUNCTION SCAN: 0x1c29800 to 0x1c2a400 ===");
        Address scanStart = af.getAddress("0x1c29800");
        Address scanEnd = af.getAddress("0x1c2a400");
        FunctionIterator fiter = fm.getFunctions(scanStart, true);
        while (fiter.hasNext()) {
            Function f = fiter.next();
            if (f.getEntryPoint().compareTo(scanEnd) > 0) break;
            long elfAddr = f.getEntryPoint().getOffset() - 0x100000;
            println("  FUNC: " + f.getName() +
                    " @ " + f.getEntryPoint() +
                    " (ELF: 0x" + Long.toHexString(elfAddr) +
                    ") size=" + f.getBody().getNumAddresses());
        }

        decomp.dispose();
    }
}
