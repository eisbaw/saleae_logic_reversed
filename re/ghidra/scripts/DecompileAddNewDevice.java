// Decompile AddNewDevice and related functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileAddNewDevice extends GhidraScript {
    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Ghidra VA = ELF VA + 0x100000
        String[] targets = new String[] {
            "0x1c289b0",   // ELF 0x1c189b0 - LinuxDevicesManager::AddNewDevice (called from CheckForConnectionThread)
            "0x1c23eb0",   // ELF 0x1c13eb0 - DevicesManager::AddNewDevice? (called from AddNewDevice)
            "0x1c21fa0",   // ELF 0x1c11fa0 - DeviceTypeToString
            "0x1c254f0",   // ELF 0x1c154f0 - called from CheckForConnectionThread (processes disconnected devices)
        };

        java.util.Set<String> seen = new java.util.HashSet<>();

        for (String addrStr : targets) {
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("NO FUNCTION at Ghidra " + addrStr);
                continue;
            }

            String key = func.getEntryPoint().toString();
            if (seen.contains(key)) continue;
            seen.add(key);

            println("=== FUNCTION @ " + func.getEntryPoint() + " ===");
            println("    ELF VA: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000));
            println("    Size: " + func.getBody().getNumAddresses() + " bytes");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("");
        }
        decomp.dispose();
    }
}
