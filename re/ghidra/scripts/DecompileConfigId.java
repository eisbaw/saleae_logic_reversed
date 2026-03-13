// Decompile functions related to config_id assignment and start packet emission
// Target: Determine how config_id (bits [31:16] of chunk header) is assigned,
// and when start packets (is_start=1) are emitted.
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileConfigId extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Ghidra VA = ELF VA + 0x100000
        String[] addrs = new String[] {
            // SetConfigurationIds in mso_fpga.cpp -- writes config_id to FPGA registers
            // Contains "Setting scope configuration IDs: {} {} {} {}" log
            // and 4 FPGA WriteRegister calls afterward
            "0x2e64264",  // ELF: 0x2d64264, start of SetConfigurationIds function body

            // SetConfiguration return type is ConfigurationId
            // References assertion about ConfigurationId at ELF 0x2d47a0c
            "0x2e47a0c",  // ELF: 0x2d47a0c

            // DigitalCreatorNode handler -- where config_id is compared/stored
            // per-channel state management, "Got start packet" log
            "0x2e51710",  // ELF: 0x2d51710, DigitalCreatorNode pipe handler

            // The SamplingConfigurationData handler in oscilloscope_instrument.cpp
            // operator() -- emits start config before capture
            "0x2e4a280",  // ELF: 0x2d4a280, operator() in oscilloscope_instrument.cpp

            // HandleStartCapture in logic_device_node.cpp
            "0x1ce63c0",  // ELF: 0x1be63c0, HandleStartCapture (already in capture-start)

            // "reading back capture settings" xref -- SetCaptureParameters area
            "0x1d260d9",  // ELF: 0x1c260d9
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        // Increase timeout for large functions
        decomp.setSimplificationStyle("decompile");

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        StringBuilder sb = new StringBuilder();
        sb.append("// Decompilation output for config_id investigation\n");
        sb.append("// Ghidra addresses = ELF VA + 0x100000\n\n");

        for (String addrStr : addrs) {
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                sb.append("--- NO FUNCTION at " + addrStr + " ---\n\n");
                println("NO FUNCTION at " + addrStr);
                continue;
            }
            String header = "=== " + func.getName() + " @ " + func.getEntryPoint() +
                " (queried: " + addrStr + ", size: " + func.getBody().getNumAddresses() + ") ===";
            println(header);
            sb.append(header + "\n\n");

            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                String code = res.getDecompiledFunction().getC();
                sb.append(code);
                // Also print first 200 lines to Ghidra console
                String[] lines = code.split("\n");
                for (int i = 0; i < Math.min(50, lines.length); i++) {
                    println(lines[i]);
                }
                if (lines.length > 50) {
                    println("... (" + (lines.length - 50) + " more lines) ...");
                }
            } else {
                sb.append("DECOMPILE FAILED: " + res.getErrorMessage() + "\n");
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            sb.append("\n\n");
        }
        decomp.dispose();

        // Write output
        java.io.FileWriter fw = new java.io.FileWriter(
            "/home/mpedersen/nvidia/saleae_reverse/re/ghidra/decompiled-config-id.txt");
        fw.write(sb.toString());
        fw.close();
        println("Output written to re/ghidra/decompiled-config-id.txt");
    }
}
