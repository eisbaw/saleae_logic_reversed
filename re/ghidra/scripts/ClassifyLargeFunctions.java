// Decompile top 5 largest unnamed functions and extract string refs for classification
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.data.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.scalar.*;

public class ClassifyLargeFunctions extends GhidraScript {
    @Override
    public void run() throws Exception {
        // ELF VAs + 0x100000 = Ghidra VAs
        String[] ghidraAddrs = new String[] {
            "0x02b3a820",  // ELF 0x02a3a820 (44,952 bytes)
            "0x0206d820",  // ELF 0x01f6d820 (41,864 bytes)
            "0x020379c0",  // ELF 0x01f379c0 (34,211 bytes)
            "0x027daa90",  // ELF 0x026daa90 (34,012 bytes)
            "0x023c7120",  // ELF 0x022c7120 (29,844 bytes)
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();
        Listing listing = currentProgram.getListing();

        for (String addrStr : ghidraAddrs) {
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("--- NO FUNCTION at " + addrStr + " ---");
                continue;
            }

            long size = func.getBody().getNumAddresses();
            String elfVa = "0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000);
            println("============================================================");
            println("=== " + func.getName() + " @ " + func.getEntryPoint()
                    + " (ELF: " + elfVa + ", " + size + " bytes) ===");
            println("============================================================");

            // Extract string references from the function body
            println("--- STRING REFERENCES ---");
            AddressSetView body = func.getBody();
            int stringCount = 0;
            java.util.Set<String> strings = new java.util.LinkedHashSet<>();
            for (AddressRange range : body) {
                Address start = range.getMinAddress();
                Address end = range.getMaxAddress();
                AddressIterator addrIter = rm.getReferenceSourceIterator(new AddressSet(start, end), true);
                while (addrIter.hasNext()) {
                    Address refSource = addrIter.next();
                    Reference[] refs = rm.getReferencesFrom(refSource);
                    for (Reference ref : refs) {
                        Address target = ref.getToAddress();
                        Data data = listing.getDefinedDataAt(target);
                        if (data != null && data.getDataType() instanceof StringDataType
                            || (data != null && data.getDataType().getName().contains("string"))) {
                            String val = data.getValue() != null ? data.getValue().toString() : "(null)";
                            if (val.length() > 200) val = val.substring(0, 200) + "...";
                            strings.add(val);
                        } else if (data != null && data.getDataType() instanceof PointerDataType) {
                            // Follow pointer to string
                            Object ptrVal = data.getValue();
                            if (ptrVal instanceof Address) {
                                Data strData = listing.getDefinedDataAt((Address)ptrVal);
                                if (strData != null && strData.getValue() != null) {
                                    String sv = strData.getValue().toString();
                                    if (sv.length() > 200) sv = sv.substring(0, 200) + "...";
                                    strings.add("(ptr->) " + sv);
                                }
                            }
                        }
                    }
                }
            }
            for (String s : strings) {
                println("  STR: " + s);
                stringCount++;
            }
            println("  (total " + stringCount + " unique strings)");

            // Decompile with 300s timeout for large functions
            println("--- DECOMPILED (first 200 lines) ---");
            DecompileResults res = decomp.decompileFunction(func, 300, monitor);
            if (res.decompileCompleted()) {
                String code = res.getDecompiledFunction().getC();
                String[] lines = code.split("\n");
                int limit = Math.min(lines.length, 200);
                for (int i = 0; i < limit; i++) {
                    println(lines[i]);
                }
                if (lines.length > 200) {
                    println("  ... (" + (lines.length - 200) + " more lines) ...");
                }
                println("  (total " + lines.length + " lines decompiled)");
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }

            // Called functions (to see what subsystem it belongs to)
            println("--- CALLED FUNCTIONS (named only) ---");
            java.util.Set<Function> called = func.getCalledFunctions(monitor);
            int namedCount = 0;
            for (Function cf : called) {
                if (!cf.getName().startsWith("FUN_")) {
                    println("  CALLS: " + cf.getName() + " @ " + cf.getEntryPoint());
                    namedCount++;
                }
            }
            println("  (" + namedCount + " named callees, " + called.size() + " total)");
            println("");
        }
        decomp.dispose();
    }
}
