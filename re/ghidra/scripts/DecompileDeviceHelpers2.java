// Decompile ALL functions in the DeviceHelpers neighborhood and callers
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class DecompileDeviceHelpers2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager refMgr = currentProgram.getReferenceManager();

        long rebase = 0x100000;  // Confirmed: Ghidra = ELF VA + 0x100000

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        // Decompile functions in the DeviceHelpers neighborhood
        // From function scan: ELF 0x1c19800 to 0x1c1a280 contains candidates
        long[] elfAddrs = {
            // Functions from scan that might belong to DeviceHelpers.cpp
            0x1c19800,  // size=218
            0x1c198e0,  // size=383
            0x1c19ab0,  // size=15
            0x1c19ac0,  // size=28
            0x1c19ae0,  // size=10
            0x1c19b00,  // size=112
            0x1c19ba0,  // size=1
            0x1c19bb0,  // size=239
            0x1c19cc0,  // size=300
            0x1c19f90,  // size=15 (thunk with "/id")
            0x1c19fa0,  // DeviceCalculateMixRate size=317
            0x1c1a0dd,  // Assertion handler for MixRate size=80
            0x1c1a130,  // DeviceCalculateSamplesCollected size=329
            0x1c1a280,  // Single ret, size=1

            // Callers of DeviceCalculateMixRate
            0x1be7ce7,  // Caller 1
            0x1c24fb9,  // Caller 2
            0x1c26c80,  // Caller 3
            0x1c4a6f9,  // Caller 4 (internal call from SamplesCollected already shown)
        };

        for (long elfAddr : elfAddrs) {
            long ghidraAddr = elfAddr + rebase;
            Address addr = af.getAddress("0x" + Long.toHexString(ghidraAddr));
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("--- NO FUNCTION at ELF 0x" + Long.toHexString(elfAddr) + " ---");
                continue;
            }
            long funcElf = func.getEntryPoint().getOffset() - rebase;
            if (funcElf != elfAddr) {
                println("--- ELF 0x" + Long.toHexString(elfAddr) +
                        " is inside " + func.getName() + " @ ELF 0x" + Long.toHexString(funcElf) + " ---");
                continue;
            }
            println("=== " + func.getName() + " @ ELF 0x" + Long.toHexString(funcElf) +
                    " size=" + func.getBody().getNumAddresses() + " ===");

            // Get callers
            Address entryAddr = func.getEntryPoint();
            ReferenceIterator refIter = refMgr.getReferencesTo(entryAddr);
            StringBuilder callerList = new StringBuilder();
            int callerCount = 0;
            while (refIter.hasNext()) {
                Reference ref = refIter.next();
                Address fromAddr = ref.getFromAddress();
                Function callerFunc = fm.getFunctionContaining(fromAddr);
                if (callerFunc != null) {
                    long callerElf = callerFunc.getEntryPoint().getOffset() - rebase;
                    callerList.append("  Caller: " + callerFunc.getName() +
                            " @ ELF 0x" + Long.toHexString(callerElf) + "\n");
                    callerCount++;
                }
            }
            println("Callers: " + callerCount);
            if (callerCount > 0) println(callerList.toString());

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
