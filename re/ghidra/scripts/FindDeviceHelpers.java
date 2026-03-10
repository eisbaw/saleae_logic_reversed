// Find DeviceHelpers functions by searching for the assertion string xref
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class FindDeviceHelpers extends GhidraScript {
    @Override
    public void run() throws Exception {
        Memory mem = currentProgram.getMemory();
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager refMgr = currentProgram.getReferenceManager();

        // Print the image base
        Address imageBase = currentProgram.getImageBase();
        println("Image base: " + imageBase);

        // Find the full DeviceHelpers.cpp path string
        byte[] assertPattern = "/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceHelpers.cpp".getBytes("ASCII");
        Address assertStr = mem.findBytes(af.getAddress("0x0"), assertPattern, null, true, monitor);
        println("Full DeviceHelpers.cpp path string at Ghidra: " + assertStr);
        if (assertStr != null) {
            long ghidraAddr = assertStr.getOffset();
            long elfAddr = 0x3c4d40;
            long rebase = ghidraAddr - elfAddr;
            println("Ghidra address: 0x" + Long.toHexString(ghidraAddr));
            println("ELF VA: 0x" + Long.toHexString(elfAddr));
            println("Rebase offset: 0x" + Long.toHexString(rebase));

            // Get references to this string
            ReferenceIterator refIter = refMgr.getReferencesTo(assertStr);
            println("References to DeviceHelpers.cpp path string:");
            Set<Function> helperFuncs = new HashSet<>();
            while (refIter.hasNext()) {
                Reference ref = refIter.next();
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                String funcName = (func != null) ? func.getName() + " @ " + func.getEntryPoint() : "NO FUNCTION";
                println("  From: " + fromAddr + " in " + funcName);
                if (func != null) helperFuncs.add(func);
            }

            // Decompile the functions that reference DeviceHelpers.cpp
            DecompInterface decomp = new DecompInterface();
            decomp.openProgram(currentProgram);
            for (Function func : helperFuncs) {
                long funcElf = func.getEntryPoint().getOffset() - rebase;
                println("=== " + func.getName() + " @ " + func.getEntryPoint() +
                        " (ELF: 0x" + Long.toHexString(funcElf) + ") ===");
                println("Size: " + func.getBody().getNumAddresses() + " bytes");
                DecompileResults res = decomp.decompileFunction(func, 60, monitor);
                if (res.decompileCompleted()) {
                    println(res.getDecompiledFunction().getC());
                } else {
                    println("DECOMPILE FAILED: " + res.getErrorMessage());
                }
                println("");
            }

            // Also search for nearby functions in the same translation unit
            // DeviceCalculateMixRate should be at ELF 0x1c19fa0 = Ghidra 0x1c19fa0 + rebase
            long mixRateGhidra = 0x1c19fa0 + rebase;
            println("Looking for DeviceCalculateMixRate at Ghidra: 0x" + Long.toHexString(mixRateGhidra));
            Address mixAddr = af.getAddress("0x" + Long.toHexString(mixRateGhidra));
            Function mixFunc = fm.getFunctionAt(mixAddr);
            if (mixFunc != null) {
                long funcElf = mixFunc.getEntryPoint().getOffset() - rebase;
                println("=== " + mixFunc.getName() + " @ " + mixFunc.getEntryPoint() +
                        " (ELF: 0x" + Long.toHexString(funcElf) + ") ===");
                println("Size: " + mixFunc.getBody().getNumAddresses() + " bytes");
                DecompileResults res = decomp.decompileFunction(mixFunc, 60, monitor);
                if (res.decompileCompleted()) {
                    println(res.getDecompiledFunction().getC());
                } else {
                    println("DECOMPILE FAILED: " + res.getErrorMessage());
                }
            } else {
                println("No function found at DeviceCalculateMixRate expected address");
                // Try getFunctionContaining instead
                Function containing = fm.getFunctionContaining(mixAddr);
                if (containing != null) {
                    println("  Containing function: " + containing.getName() + " @ " + containing.getEntryPoint());
                }
            }

            // DeviceCalculateSamplesCollected at ELF 0x1c1a130
            long samplesGhidra = 0x1c1a130 + rebase;
            println("Looking for DeviceCalculateSamplesCollected at Ghidra: 0x" + Long.toHexString(samplesGhidra));
            Address samplesAddr = af.getAddress("0x" + Long.toHexString(samplesGhidra));
            Function samplesFunc = fm.getFunctionAt(samplesAddr);
            if (samplesFunc != null) {
                long funcElf = samplesFunc.getEntryPoint().getOffset() - rebase;
                println("=== " + samplesFunc.getName() + " @ " + samplesFunc.getEntryPoint() +
                        " (ELF: 0x" + Long.toHexString(funcElf) + ") ===");
                println("Size: " + samplesFunc.getBody().getNumAddresses() + " bytes");
                DecompileResults res = decomp.decompileFunction(samplesFunc, 60, monitor);
                if (res.decompileCompleted()) {
                    println(res.getDecompiledFunction().getC());
                } else {
                    println("DECOMPILE FAILED: " + res.getErrorMessage());
                }
            } else {
                println("No function found at DeviceCalculateSamplesCollected expected address");
                Function containing = fm.getFunctionContaining(samplesAddr);
                if (containing != null) {
                    long funcElf = containing.getEntryPoint().getOffset() - rebase;
                    println("  Containing function: " + containing.getName() + " @ " + containing.getEntryPoint() +
                            " (ELF: 0x" + Long.toHexString(funcElf) + ")");
                    println("  Decompiling containing function...");
                    DecompileResults res = decomp.decompileFunction(containing, 60, monitor);
                    if (res.decompileCompleted()) {
                        println(res.getDecompiledFunction().getC());
                    }
                }
            }

            // Scan neighborhood for function boundaries
            long scanStartElf = 0x1c19800;
            long scanEndElf = 0x1c1a400;
            long scanStartGhidra = scanStartElf + rebase;
            long scanEndGhidra = scanEndElf + rebase;
            println("\n=== FUNCTION SCAN: ELF 0x" + Long.toHexString(scanStartElf) +
                    " to 0x" + Long.toHexString(scanEndElf) + " ===");
            Address scanStart = af.getAddress("0x" + Long.toHexString(scanStartGhidra));
            Address scanEnd = af.getAddress("0x" + Long.toHexString(scanEndGhidra));
            FunctionIterator fiter = fm.getFunctions(scanStart, true);
            while (fiter.hasNext()) {
                Function f = fiter.next();
                if (f.getEntryPoint().compareTo(scanEnd) > 0) break;
                long elfA = f.getEntryPoint().getOffset() - rebase;
                println("  FUNC: " + f.getName() +
                        " @ " + f.getEntryPoint() +
                        " (ELF: 0x" + Long.toHexString(elfA) +
                        ") size=" + f.getBody().getNumAddresses());
            }

            decomp.dispose();
        }
    }
}
