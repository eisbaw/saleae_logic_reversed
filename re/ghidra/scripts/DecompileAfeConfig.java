// Decompile AFE config enumeration and VGA/ADC register config functions
// Targets: ForConfigNumber, GetConfigNumber, EncodeVgaConfig, GetAdcRegisterConfiguration
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class DecompileAfeConfig extends GhidraScript {
    @Override
    public void run() throws Exception {
        // String VAs (ELF file offset + 0x100000 Ghidra rebase):
        String[][] stringTargets = new String[][] {
            {"0x48ff4c", "ForConfigNumber assertion string"},
            {"0x4e6ad4", "EncodeVgaConfig assertion string"},
            {"0x5116ea", "GetAdcRegisterConfiguration assertion string"},
            {"0x52d81c", "GetConfigNumber assertion string"},
            {"0x52d6f8", "AfeConfig_RawHardware format string"},
        };

        DecompInterface decomp = new DecompInterface();
        DecompileOptions options = new DecompileOptions();
        decomp.setOptions(options);
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        Set<String> decompiled = new HashSet<>();

        for (String[] target : stringTargets) {
            String strAddrStr = target[0];
            String desc = target[1];

            Address strAddr = af.getAddress(strAddrStr);
            if (strAddr == null) {
                println("--- INVALID STRING ADDRESS: " + strAddrStr + " (" + desc + ") ---");
                continue;
            }

            Data data = currentProgram.getListing().getDataAt(strAddr);
            String strValue = "(no data)";
            if (data != null && data.hasStringValue()) {
                strValue = data.getValue().toString();
                if (strValue.length() > 120) strValue = strValue.substring(0, 120) + "...";
            }
            println("--- String target: " + desc + " ---");
            println("    String addr: " + strAddrStr + " = \"" + strValue + "\"");

            // Use GhidraScript's getReferencesTo which returns Reference[]
            List<Address> refAddrs = new ArrayList<>();
            ReferenceIterator refIter = currentProgram.getReferenceManager().getReferencesTo(strAddr);
            while (refIter.hasNext()) {
                refAddrs.add(refIter.next().getFromAddress());
            }

            if (refAddrs.isEmpty()) {
                // Try nearby addresses
                println("    No direct refs, trying nearby addresses...");
                for (int delta = -4; delta <= 4; delta++) {
                    if (delta == 0) continue;
                    Address nearby = strAddr.add(delta);
                    refIter = currentProgram.getReferenceManager().getReferencesTo(nearby);
                    while (refIter.hasNext()) {
                        refAddrs.add(refIter.next().getFromAddress());
                    }
                    if (!refAddrs.isEmpty()) {
                        println("    Found refs at offset " + delta + " from target");
                        break;
                    }
                }
            }

            if (refAddrs.isEmpty()) {
                println("    NO REFERENCES FOUND to " + strAddrStr);
                println("");
                continue;
            }

            println("    References found: " + refAddrs.size());

            for (Address fromAddr : refAddrs) {
                Function func = fm.getFunctionContaining(fromAddr);
                if (func == null) {
                    println("    Ref from " + fromAddr + " -- no containing function");
                    continue;
                }

                String entryStr = func.getEntryPoint().toString();
                if (decompiled.contains(entryStr)) {
                    println("    --- ALREADY DECOMPILED: " + func.getName() + " @ " + entryStr + " ---");
                    continue;
                }
                decompiled.add(entryStr);

                long elfVa = func.getEntryPoint().getOffset() - 0x100000;
                println("");
                println("=== " + func.getName() + " @ " + entryStr + " ===");
                println("    ELF VA: 0x" + Long.toHexString(elfVa));
                println("    Size: " + func.getBody().getNumAddresses() + " bytes");
                println("    Found via: " + desc);
                println("    String ref from: " + fromAddr);

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
}
