// Decompile SetScopeChannelConfig and WriteRegister_0x194 to decode bit fields
// TASK-0204: Decode FPGA register 0x0194 (SCOPE_CHANNEL_CONFIG)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;

public class DecompileScopeChannelConfig extends GhidraScript {
    @Override
    public void run() throws Exception {
        // All addresses are Ghidra VAs (ELF VA + 0x100000)
        String[][] targets = new String[][] {
            // SetScopeChannelConfig - writes reg from DAT_0089ad92 table
            // ELF 0x02d66b70 -> Ghidra 0x02e66b70
            {"02e66b70", "SetScopeChannelConfig"},

            // WriteRegister_0x194 - writes reg 0x194 with single uint32
            // ELF 0x02d662c0 -> Ghidra 0x02e662c0
            {"02e662c0", "WriteRegister_0x194"},

            // SetScopeConfigurationId - writes to reg table at DAT_0089ad80
            // ELF 0x02d661f0 -> Ghidra 0x02e661f0
            {"02e661f0", "SetScopeConfigurationId"},

            // SetCommitBankControl - writes reg 0x1C0 with enable bits
            // ELF 0x02d66590 -> Ghidra 0x02e66590
            {"02e66590", "SetCommitBankControl"},
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        StringBuilder sb = new StringBuilder();
        sb.append("// Decompilation output for SCOPE_CHANNEL_CONFIG (0x0194) investigation\n");
        sb.append("// TASK-0204\n");
        sb.append("// Ghidra addresses = ELF VA + 0x100000\n\n");

        java.util.Set<String> decompiled = new java.util.HashSet<>();

        // Phase 1: Decompile target functions
        sb.append("=== PHASE 1: Target Functions ===\n\n");
        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                String msg = "--- NO FUNCTION containing " + t[0] + " (" + t[1] + ") ---";
                println(msg);
                sb.append(msg + "\n\n");
                continue;
            }

            String funcKey = func.getEntryPoint().toString();
            if (decompiled.contains(funcKey)) {
                println("ALREADY DECOMPILED: " + func.getName() + " @ " + funcKey);
                continue;
            }
            decompiled.add(funcKey);

            String header = "--- " + t[1] + " = " + func.getName() + " @ " + func.getEntryPoint() +
                " (size: " + func.getBody().getNumAddresses() + " bytes) ---";
            println(header);
            sb.append(header + "\n\n");

            DecompileResults res = decomp.decompileFunction(func, 180, monitor);
            if (res.decompileCompleted()) {
                String code = res.getDecompiledFunction().getC();
                sb.append(code);
                String[] lines = code.split("\n");
                for (int i = 0; i < Math.min(80, lines.length); i++) {
                    println(lines[i]);
                }
                if (lines.length > 80) {
                    println("... (" + (lines.length - 80) + " more lines) ...");
                }
            } else {
                sb.append("DECOMPILE FAILED: " + res.getErrorMessage() + "\n");
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            sb.append("\n\n");
        }

        // Phase 2: Find callers of SetScopeChannelConfig (0x02e66b70) and WriteRegister_0x194 (0x02e662c0)
        sb.append("\n=== PHASE 2: Callers (cross-references) ===\n\n");
        String[] callTargets = {"02e66b70", "02e662c0"};
        String[] callNames = {"SetScopeChannelConfig", "WriteRegister_0x194"};

        for (int i = 0; i < callTargets.length; i++) {
            Address targetAddr = af.getAddress(callTargets[i]);
            Function targetFunc = fm.getFunctionContaining(targetAddr);
            if (targetFunc == null) {
                sb.append("No function at " + callTargets[i] + "\n\n");
                continue;
            }

            sb.append("--- Callers of " + callNames[i] + " @ " + targetFunc.getEntryPoint() + " ---\n");
            println("--- Callers of " + callNames[i] + " ---");

            // Get references TO this function's entry point
            ReferenceIterator refIter = rm.getReferencesTo(targetFunc.getEntryPoint());
            java.util.List<Reference> refList = new java.util.ArrayList<>();
            while (refIter.hasNext()) {
                refList.add(refIter.next());
            }
            java.util.Set<String> callerDecompiled = new java.util.HashSet<>();

            sb.append("  Found " + refList.size() + " references\n");
            println("  Found " + refList.size() + " references");

            for (Reference ref : refList) {
                Address fromAddr = ref.getFromAddress();
                Function callerFunc = fm.getFunctionContaining(fromAddr);
                if (callerFunc == null) continue;

                String callerKey = callerFunc.getEntryPoint().toString();
                if (callerDecompiled.contains(callerKey) || decompiled.contains(callerKey)) {
                    sb.append("  Already decompiled caller: " + callerFunc.getName() + " @ " + callerKey + "\n");
                    continue;
                }
                callerDecompiled.add(callerKey);
                decompiled.add(callerKey);

                String callerHeader = "--- Caller: " + callerFunc.getName() + " @ " + callerFunc.getEntryPoint() +
                    " (size: " + callerFunc.getBody().getNumAddresses() + " bytes, calls from " + fromAddr + ") ---";
                println(callerHeader);
                sb.append(callerHeader + "\n\n");

                // Only decompile if not too large (< 50KB)
                if (callerFunc.getBody().getNumAddresses() > 50000) {
                    sb.append("  SKIPPED: function too large (" + callerFunc.getBody().getNumAddresses() + " bytes)\n\n");
                    println("  SKIPPED: too large");
                    continue;
                }

                DecompileResults res = decomp.decompileFunction(callerFunc, 180, monitor);
                if (res.decompileCompleted()) {
                    String code = res.getDecompiledFunction().getC();
                    sb.append(code);
                    String[] lines = code.split("\n");
                    for (int j = 0; j < Math.min(60, lines.length); j++) {
                        println(lines[j]);
                    }
                    if (lines.length > 60) {
                        println("... (" + (lines.length - 60) + " more lines) ...");
                    }
                } else {
                    sb.append("DECOMPILE FAILED: " + res.getErrorMessage() + "\n");
                }
                sb.append("\n\n");
            }
            sb.append("\n");
        }

        // Phase 3: Inspect DAT_0089ad92 (register address table used by SetScopeChannelConfig)
        sb.append("\n=== PHASE 3: Data at DAT_0089ad92 (register address table) ===\n");
        try {
            Address datAddr = af.getAddress("0089ad92");
            sb.append("Address: " + datAddr + "\n");
            // Read 32 bytes from this location
            byte[] data = new byte[32];
            currentProgram.getMemory().getBytes(datAddr, data);
            sb.append("Raw bytes: ");
            for (int j = 0; j < data.length; j++) {
                sb.append(String.format("%02x ", data[j] & 0xff));
            }
            sb.append("\n");
            // Interpret as 16-bit register addresses
            sb.append("As uint16 LE: ");
            for (int j = 0; j + 1 < data.length; j += 2) {
                int val = (data[j] & 0xff) | ((data[j+1] & 0xff) << 8);
                sb.append(String.format("0x%04x ", val));
            }
            sb.append("\n");
            println("DAT_0089ad92 read OK");
        } catch (Exception e) {
            sb.append("Failed to read: " + e.getMessage() + "\n");
            println("Failed to read DAT_0089ad92: " + e.getMessage());
        }

        // Also inspect DAT_0089ad80 (used by SetScopeConfigurationId)
        sb.append("\n=== PHASE 3b: Data at DAT_0089ad80 (config ID register table) ===\n");
        try {
            Address datAddr = af.getAddress("0089ad80");
            byte[] data = new byte[32];
            currentProgram.getMemory().getBytes(datAddr, data);
            sb.append("Raw bytes: ");
            for (int j = 0; j < data.length; j++) {
                sb.append(String.format("%02x ", data[j] & 0xff));
            }
            sb.append("\n");
            sb.append("As uint16 LE: ");
            for (int j = 0; j + 1 < data.length; j += 2) {
                int val = (data[j] & 0xff) | ((data[j+1] & 0xff) << 8);
                sb.append(String.format("0x%04x ", val));
            }
            sb.append("\n");
            println("DAT_0089ad80 read OK");
        } catch (Exception e) {
            sb.append("Failed to read: " + e.getMessage() + "\n");
        }

        // Phase 4: Also read DAT_0089ad8a (used by SetDigitalDerivedVoltageThresholds)
        sb.append("\n=== PHASE 3c: Data at DAT_0089ad8a (voltage threshold register table) ===\n");
        try {
            Address datAddr = af.getAddress("0089ad8a");
            byte[] data = new byte[16];
            currentProgram.getMemory().getBytes(datAddr, data);
            sb.append("Raw bytes: ");
            for (int j = 0; j < data.length; j++) {
                sb.append(String.format("%02x ", data[j] & 0xff));
            }
            sb.append("\n");
            sb.append("As uint16 LE: ");
            for (int j = 0; j + 1 < data.length; j += 2) {
                int val = (data[j] & 0xff) | ((data[j+1] & 0xff) << 8);
                sb.append(String.format("0x%04x ", val));
            }
            sb.append("\n");
            println("DAT_0089ad8a read OK");
        } catch (Exception e) {
            sb.append("Failed to read: " + e.getMessage() + "\n");
        }

        decomp.dispose();

        // Write output
        java.io.FileWriter fw = new java.io.FileWriter(
            "/home/mpedersen/nvidia/saleae_reverse/re/ghidra/decompiled-scope-channel-config.txt");
        fw.write(sb.toString());
        fw.close();
        println("\nOutput written to re/ghidra/decompiled-scope-channel-config.txt");
    }
}
