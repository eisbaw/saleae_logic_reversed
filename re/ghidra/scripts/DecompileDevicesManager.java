// Decompile DevicesManager and LinuxDevicesManager functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileDevicesManager extends GhidraScript {
    @Override
    public void run() throws Exception {
        // All addresses are Ghidra VA (ELF VA + 0x100000)
        // These are addresses INSIDE the functions (at xref points to assertion strings)
        // Ghidra will resolve them to the containing function.
        String[] addrs = new String[] {
            // DevicesManager.cpp xrefs (8 refs to DevicesManager.cpp source path)
            "0x1c228db",  // ELF 0x1c128db
            "0x1c23a2c",  // ELF 0x1c13a2c - near AddFakeDevice
            "0x1c23d8a",  // ELF 0x1c13d8a - near RemoveFakeDataDevice
            "0x1c23e8e",  // ELF 0x1c13e8e - near GetVidPidForDevice
            "0x1c243b6",  // ELF 0x1c143b6
            "0x1c24f9c",  // ELF 0x1c14f9c
            "0x1c25171",  // ELF 0x1c15171 - near Total AddNewDevice
            "0x1c256c1",  // ELF 0x1c156c1

            // LinuxDevicesManager.cpp xrefs (4 refs)
            "0x1c28151",  // ELF 0x1c18151
            "0x1c281e5",  // ELF 0x1c181e5 - near CheckForConnectionThread
            "0x1c283f2",  // ELF 0x1c183f2 - near GetAllDevices
            "0x1c28bed",  // ELF 0x1c18bed

            // GetVidPidForDevice specific (assertion ref)
            "0x1c23e87",  // ELF 0x1c13e87

            // RemoveFakeDataDevice specific
            "0x1c23d83",  // ELF 0x1c13d83

            // Total AddNewDevice log
            "0x1c251a4",  // ELF 0x1c151a4

            // AddFakeDevice assertion
            "0x1c23a25",  // ELF 0x1c13a25

            // CheckForConnectionThread assertion
            "0x1c281de",  // ELF 0x1c181de

            // GetAllDevices assertion
            "0x1c283eb",  // ELF 0x1c183eb
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Track already-decompiled functions to avoid duplicates
        java.util.Set<String> seen = new java.util.HashSet<>();

        for (String addrStr : addrs) {
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("--- NO FUNCTION at " + addrStr + " (try creating function here) ---");
                continue;
            }
            String key = func.getEntryPoint().toString();
            if (seen.contains(key)) {
                continue;
            }
            seen.add(key);

            println("=== FUNCTION @ " + func.getEntryPoint() + " (name: " + func.getName() + ", queried: " + addrStr + ") ===");
            println("    ELF VA: 0x" + Long.toHexString(func.getEntryPoint().getOffset() - 0x100000));
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
