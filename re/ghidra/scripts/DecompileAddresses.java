// Decompile functions at specific addresses
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileAddresses extends GhidraScript {
    @Override
    public void run() throws Exception {
        String[] addrs = new String[] {
            // ComputationUtilities.cpp functions (Ghidra VA = ELF VA + 0x100000)
            "0x1d46ab0",  // SelectClosestIndex(vector<U64>&, U64)  ELF: 0x1c46ab0
            "0x1d46d40",  // SelectClosestIndex(vector<U32>&, U32)  ELF: 0x1c46d40
            "0x1d47080",  // RandomSelect helper                    ELF: 0x1c47080
            "0x1d47140",  // Gcd(U32, U32)                          ELF: 0x1c47140
            "0x1d47170",  // Lcm(U32, U32)                          ELF: 0x1c47170
            "0x1d47240",  // LcmWithQuotients(U32, U32, U32*, U32*) ELF: 0x1c47240
            "0x1d47340",  // Next function after LcmWithQuotients    ELF: 0x1c47340
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
            println("=== " + func.getName() + " @ " + func.getEntryPoint() + " (queried: " + addrStr + ") ===");
            DecompileResults res = decomp.decompileFunction(func, 60, monitor);
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
