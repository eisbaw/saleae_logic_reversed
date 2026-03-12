// Find the FX3 WriteEeprom and ReadEeprom implementations
// by looking at what vtable entries are at offsets 0x10 and 0x18
// of the Fx3DeviceFeatures vtable
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class DecompileTestEeprom4 extends GhidraScript {
    @Override
    public void run() throws Exception {
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager refMgr = currentProgram.getReferenceManager();

        // Search for Fx3DeviceFeatures::WriteEeprom and ReadEeprom strings
        // From strings output:
        // "virtual void Fx3DeviceFeatures::WriteEeprom(U16, std::vector<U8> &)" at some offset
        // "virtual void Fx3DeviceFeatures::ReadEeprom(U16, U8 *, U16)" at some offset
        // Let's find these strings

        // Search for the assertion string references
        // "Fx3DeviceFeatures::WriteEeprom"
        // "Fx3DeviceFeatures::ReadEeprom"

        // First, let's search the binary for these function name strings
        println("=== Searching for Fx3DeviceFeatures EEPROM strings ===");

        // Let's try to find function at known addresses
        // From the Fx2 WriteEeprom (FUN_01d50ad0), we know the packet format
        // [0x42, 0x06, 0x55, addr_lo, size, data...]
        // But FX3 uses a different path (ReadVersion command = opcode 0x07)

        // The ReadEeprom in the memory doc says:
        // ReadVersion (0x07) with I2C params [0x33, 0x81, addr_lo, addr_hi, cnt_lo, cnt_hi]
        // So WriteEeprom for FX3 should be WriteVersion or similar

        // Let's search for "Fx3" strings
        println("Searching for Fx3 device features strings...");

        // Look at all data references near the known string addresses
        // "virtual void Fx3DeviceFeatures::WriteEeprom(U16, std::vector<U8> &)"
        // "virtual void Fx3DeviceFeatures::ReadEeprom(U16, U8 *, U16)"

        // Let me search by looking at the symbol table
        SymbolTable st = currentProgram.getSymbolTable();
        SymbolIterator iter = st.getAllSymbols(true);
        int count = 0;
        while (iter.hasNext() && count < 20) {
            Symbol sym = iter.next();
            String name = sym.getName();
            if (name.contains("Fx3") || name.contains("fx3")) {
                println("  Symbol: " + name + " @ " + sym.getAddress());
                count++;
            }
        }

        // Let me instead search for the "Fx3DeviceFeatures" assertion string
        // and find cross-references to locate the function body

        // Actually, a better approach: search memory for the string
        ghidra.program.model.mem.Memory mem = currentProgram.getMemory();
        byte[] searchBytes = "Fx3DeviceFeatures::WriteEeprom".getBytes("ASCII");
        Address start = af.getAddress("0");

        println("\n=== Searching memory for 'Fx3DeviceFeatures::WriteEeprom' ===");
        Address found = mem.findBytes(start, searchBytes, null, true, monitor);
        while (found != null) {
            println("  Found at " + found);

            // Get xrefs to this string
            ReferenceIterator refIter = refMgr.getReferencesTo(found);
            while (refIter.hasNext()) {
                Reference ref = refIter.next();
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func != null) {
                    println("    Referenced from " + fromAddr + " in " + func.getName() + " @ " + func.getEntryPoint());
                } else {
                    println("    Referenced from " + fromAddr + " (no function)");
                }
            }

            found = mem.findBytes(found.add(1), searchBytes, null, true, monitor);
        }

        // Also search for ReadEeprom
        searchBytes = "Fx3DeviceFeatures::ReadEeprom".getBytes("ASCII");
        println("\n=== Searching memory for 'Fx3DeviceFeatures::ReadEeprom' ===");
        found = mem.findBytes(start, searchBytes, null, true, monitor);
        while (found != null) {
            println("  Found at " + found);

            ReferenceIterator refIter = refMgr.getReferencesTo(found);
            while (refIter.hasNext()) {
                Reference ref = refIter.next();
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func != null) {
                    println("    Referenced from " + fromAddr + " in " + func.getName() + " @ " + func.getEntryPoint());
                } else {
                    println("    Referenced from " + fromAddr + " (no function)");
                }
            }

            found = mem.findBytes(found.add(1), searchBytes, null, true, monitor);
        }

        // Also search for "ReadEeprom cannot read"
        searchBytes = "ReadEeprom cannot read".getBytes("ASCII");
        println("\n=== Searching for 'ReadEeprom cannot read' ===");
        found = mem.findBytes(start, searchBytes, null, true, monitor);
        while (found != null) {
            println("  Found at " + found);
            ReferenceIterator refIter = refMgr.getReferencesTo(found);
            while (refIter.hasNext()) {
                Reference ref = refIter.next();
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func != null) {
                    println("    Referenced from " + fromAddr + " in " + func.getName() + " @ " + func.getEntryPoint());
                } else {
                    println("    Referenced from " + fromAddr + " (no function)");
                }
            }
            found = mem.findBytes(found.add(1), searchBytes, null, true, monitor);
        }

        // Decompile found functions
        println("\n\n========== DECOMPILING FX3 EEPROM FUNCTIONS ==========\n");
    }
}
