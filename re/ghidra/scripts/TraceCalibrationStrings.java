// TraceCalibrationStrings.java - Find xrefs to calibration-related strings
// @category Analysis

import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.mem.*;
import ghidra.program.model.data.*;
import ghidra.app.decompiler.*;
import ghidra.app.decompiler.flatapi.*;

import java.util.*;

public class TraceCalibrationStrings extends GhidraScript {

    private static final String[] TARGET_STRINGS = {
        "reading calibration from {}",
        "SALEAE_MSO_CALIBRATION_PATH",
        "Scope calibration is missing constants",
        "/calibration",
        "/calibrationSets",
        "/df1SosCoefficients",
        "/fullScaleVoltageRanges",
        "/hittiteAdcGainRegisterValue",
        "Ch {} transform, gain={}, offset={}",
        "cal.mAdcToVoltageTransform.mScale != 0.0",
        "cal.mAdcToDacVoltageScale != 0.0",
        "/attn100Transform",
        "/attn10Transform",
        "/adcToVoltageTransform",
        "/adcToDacVoltageScale",
        "The provided sample rate does not match",
    };

    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setOptions(new DecompileOptions());

        Memory mem = currentProgram.getMemory();
        Listing listing = currentProgram.getListing();
        ReferenceManager refMgr = currentProgram.getReferenceManager();

        for (String target : TARGET_STRINGS) {
            println("=== Searching for: \"" + target + "\" ===");

            DataIterator dataIter = listing.getDefinedData(true);
            boolean found = false;

            while (dataIter.hasNext()) {
                Data data = dataIter.next();
                if (data.hasStringValue()) {
                    String val = data.getDefaultValueRepresentation();
                    if (val.startsWith("\"")) val = val.substring(1);
                    if (val.endsWith("\"")) val = val.substring(0, val.length() - 1);

                    if (val.contains(target)) {
                        found = true;
                        Address strAddr = data.getAddress();
                        long elfVA = strAddr.getOffset() - 0x100000;
                        println("  String found at Ghidra:" + strAddr + " (ELF:0x" + Long.toHexString(elfVA) + ")");

                        // Use ReferenceIterator (not array)
                        ReferenceIterator refIter = refMgr.getReferencesTo(strAddr);
                        boolean hasRefs = false;

                        while (refIter.hasNext()) {
                            hasRefs = true;
                            Reference ref = refIter.next();
                            Address fromAddr = ref.getFromAddress();
                            long fromElfVA = fromAddr.getOffset() - 0x100000;
                            Function func = listing.getFunctionContaining(fromAddr);
                            String funcName = func != null ? func.getName() : "<unknown>";
                            long funcElfVA = func != null ? func.getEntryPoint().getOffset() - 0x100000 : 0;

                            println("  Xref from Ghidra:" + fromAddr +
                                    " (ELF:0x" + Long.toHexString(fromElfVA) + ")" +
                                    " in function: " + funcName +
                                    " (ELF:0x" + Long.toHexString(funcElfVA) + ")");
                        }

                        if (!hasRefs) {
                            println("  No xrefs found");
                        }
                    }
                }
            }

            if (!found) {
                println("  String NOT found in defined data. Searching raw bytes...");
                byte[] bytes = target.getBytes("UTF-8");
                Address addr = mem.findBytes(mem.getMinAddress(), bytes, null, true, monitor);
                if (addr != null) {
                    long elfVA = addr.getOffset() - 0x100000;
                    println("  Raw bytes found at Ghidra:" + addr + " (ELF:0x" + Long.toHexString(elfVA) + ")");

                    ReferenceIterator refIter = refMgr.getReferencesTo(addr);
                    while (refIter.hasNext()) {
                        Reference ref = refIter.next();
                        Address fromAddr = ref.getFromAddress();
                        Function func = listing.getFunctionContaining(fromAddr);
                        String funcName = func != null ? func.getName() : "<unknown>";
                        println("  Xref from " + fromAddr + " in function: " + funcName);
                    }
                } else {
                    println("  NOT found even in raw bytes");
                }
            }
            println("");
        }

        decomp.dispose();
    }
}
