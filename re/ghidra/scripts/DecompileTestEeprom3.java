// Find callers of TestEeprom (FUN_01d44010) and find where it's called
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class DecompileTestEeprom3 extends GhidraScript {
    @Override
    public void run() throws Exception {
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager refMgr = currentProgram.getReferenceManager();

        // TestEeprom is at Ghidra address 01d44010
        Address testEepromAddr = af.getAddress("01d44010");
        println("=== Finding callers of TestEeprom @ " + testEepromAddr + " ===");

        ReferenceIterator iter = refMgr.getReferencesTo(testEepromAddr);
        Set<String> callerAddrs = new LinkedHashSet<>();
        while (iter.hasNext()) {
            Reference ref = iter.next();
            Address fromAddr = ref.getFromAddress();
            Function func = fm.getFunctionContaining(fromAddr);
            if (func != null) {
                println("  Called from " + fromAddr + " in " + func.getName() + " @ " + func.getEntryPoint());
                callerAddrs.add(func.getEntryPoint().toString());
            } else {
                println("  Ref from " + fromAddr + " (no function / vtable?)");
            }
        }

        // Decompile callers
        if (!callerAddrs.isEmpty()) {
            println("\n=== Decompiling callers ===\n");
            DecompInterface decomp = new DecompInterface();
            decomp.openProgram(currentProgram);

            for (String addrStr : callerAddrs) {
                Address addr = af.getAddress(addrStr);
                Function func = fm.getFunctionAt(addr);
                if (func != null) {
                    println("=== " + func.getName() + " @ " + func.getEntryPoint() + " ===");
                    DecompileResults res = decomp.decompileFunction(func, 120, monitor);
                    if (res.decompileCompleted()) {
                        println(res.getDecompiledFunction().getC());
                    } else {
                        println("DECOMPILE FAILED: " + res.getErrorMessage());
                    }
                    println("");
                }
            }
            decomp.dispose();
        }

        // Also check the FX3 WriteEeprom vtable call in TestEeprom
        // vtable offset 0x10 on *(param_1 + 0x80) = WriteEeprom
        // vtable offset 0x18 on *(param_1 + 0x80) = ReadEeprom
        // Let's find what class this vtable belongs to
        println("\n=== Checking vtable for FX3 device features ===");
        // The assertion string mentions LogicProfessionalDevice.cpp line 0x54 = line 84
        // This is in the LogicProfessionalDevice class
    }
}
