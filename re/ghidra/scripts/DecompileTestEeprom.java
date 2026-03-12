// Find and decompile LogicProfessionalDevice::TestEeprom()
// by searching for xrefs to EEPROM-related strings
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class DecompileTestEeprom extends GhidraScript {
    @Override
    public void run() throws Exception {
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager refMgr = currentProgram.getReferenceManager();

        // Known string addresses (Ghidra rebased = ELF VA + 0x100000)
        String[][] targets = {
            {"0x489977", "void LogicProfessionalDevice::TestEeprom()"},
            {"0x4ee91a", "EEprom error detected!"},
            {"0x4a5b4b", "WipeEeprom"},
            {"0x4c4f2a", "ReadEeprom failed after 3 tries"},
            {"0x527803", "eeprom is only 256 bytes"},
        };

        Set<String> funcAddrs = new LinkedHashSet<>();

        for (String[] t : targets) {
            Address strAddr = af.getAddress(t[0]);
            println("=== Xrefs to \"" + t[1] + "\" @ " + strAddr + " ===");

            // Try exact address and nearby
            for (int delta = -2; delta <= 2; delta++) {
                Address addr = strAddr.add(delta);
                ReferenceIterator iter = refMgr.getReferencesTo(addr);
                while (iter.hasNext()) {
                    Reference ref = iter.next();
                    Address fromAddr = ref.getFromAddress();
                    Function func = fm.getFunctionContaining(fromAddr);
                    if (func != null) {
                        println("  Ref from " + fromAddr + " in " + func.getName() +
                                " @ " + func.getEntryPoint() +
                                (delta != 0 ? " (delta=" + delta + ")" : ""));
                        funcAddrs.add(func.getEntryPoint().toString());
                    } else {
                        println("  Ref from " + fromAddr + " (no function, delta=" + delta + ")");
                    }
                }
            }
        }

        // Also search for ReadEeprom/WriteEeprom function names in the symbol table
        println("\n=== Searching symbol table for EEPROM functions ===");
        SymbolTable st = currentProgram.getSymbolTable();
        SymbolIterator symIter = st.getAllSymbols(true);
        while (symIter.hasNext()) {
            Symbol sym = symIter.next();
            String name = sym.getName();
            if (name.contains("Eeprom") || name.contains("eeprom") || name.contains("EEPROM")) {
                println("  Symbol: " + name + " @ " + sym.getAddress());
                Function func = fm.getFunctionAt(sym.getAddress());
                if (func != null) {
                    funcAddrs.add(func.getEntryPoint().toString());
                }
            }
        }

        // Decompile all found functions
        println("\n\n========== DECOMPILING EEPROM FUNCTIONS ==========\n");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        for (String addrStr : funcAddrs) {
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
}
