// DecompileCalibration.java - Decompile key calibration functions
// @category Analysis

import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.app.decompiler.*;

public class DecompileCalibration extends GhidraScript {

    // Key functions identified from string xref analysis (Ghidra VAs = ELF + 0x100000)
    private static final long[][] TARGETS = {
        // {ghidra_addr, description}
        {0x02dd0200L},  // SetCalibration deserializer (/calibration, /calibrationSets, etc.)
        {0x02dd2010L},  // df1SosCoefficients parser
        {0x01bd2aa0L},  // "The provided sample rate does not match" - coefficient lookup
        {0x02dd00e0L},  // /calibration JSON path handler
        {0x01bd6130L},  // /calibration xref (second function)
    };

    private static final String[] DESCRIPTIONS = {
        "SetCalibration deserializer",
        "df1SosCoefficients parser",
        "Coefficient lookup (sample rate match)",
        "Calibration JSON path handler",
        "Calibration path handler (second)",
    };

    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        DecompileOptions opts = new DecompileOptions();
        decomp.setOptions(opts);

        Listing listing = currentProgram.getListing();

        for (int i = 0; i < TARGETS.length; i++) {
            long ghidraAddr = TARGETS[i][0];
            String desc = DESCRIPTIONS[i];

            Address addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(ghidraAddr);
            Function func = listing.getFunctionAt(addr);

            if (func == null) {
                println("=== " + desc + " at Ghidra:0x" + Long.toHexString(ghidraAddr) +
                        " (ELF:0x" + Long.toHexString(ghidraAddr - 0x100000) + ") ===");
                println("  Function not found at this address");
                println("");
                continue;
            }

            long elfVA = func.getEntryPoint().getOffset() - 0x100000;
            println("=== " + desc + " ===");
            println("  Function: " + func.getName());
            println("  Ghidra: 0x" + Long.toHexString(ghidraAddr));
            println("  ELF VA: 0x" + Long.toHexString(elfVA));
            println("  Size: " + func.getBody().getNumAddresses() + " bytes");

            DecompileResults result = decomp.decompileFunction(func, 60, monitor);
            if (result.decompileCompleted()) {
                String code = result.getDecompiledFunction().getC();
                // Limit output to 4000 chars per function
                if (code.length() > 4000) {
                    code = code.substring(0, 4000) + "\n... (truncated at 4000 chars)";
                }
                println("  Decompiled code:");
                println(code);
            } else {
                println("  Decompilation failed");
            }
            println("");
        }

        decomp.dispose();
    }
}
