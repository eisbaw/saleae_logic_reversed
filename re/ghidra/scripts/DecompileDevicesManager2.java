// Decompile DevicesManager and LinuxDevicesManager functions (targeted)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class DecompileDevicesManager2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // All addresses are Ghidra VA (ELF VA + 0x100000)
        String[] addrs = new String[] {
            // LinuxDevicesManager::GetAllDevices -- the VID/PID dispatch function
            // ELF 0x1c18240, contains the VID/PID -> DeviceType mapping
            "0x1c28240",

            // LinuxDevicesManager::CheckForConnectionThread -- hotplug polling
            // Starts right after GetAllDevices epilog
            // ELF 0x1c18440
            "0x1c28440",

            // DevicesManager::GetVidPidForDevice -- reverse mapping
            // Contains assertion at Ghidra 0x1c23e87
            // Search for function start before 0x1c23e87
            // Let's try a range of potential starts
            "0x1c23da0",
            "0x1c23dc0",
            "0x1c23de0",
            "0x1c23e00",

            // DevicesManager::AddNewDevice
            // "Total AddNewDevice" log ref at 0x1c251a4 (Ghidra)
            // Function likely starts well before that
            "0x1c24e00",
            "0x1c24f00",
            "0x1c25000",

            // DevicesManager::RemoveFakeDataDevice
            // Assertion at Ghidra 0x1c23d83
            "0x1c23d10",
            "0x1c23d40",

            // LinuxDevicesManager::AddNewDevice
            // From mangled symbol, near hotplug code
            // Look near 0x1c28bed (Ghidra) area
            "0x1c28b20",

            // Functions adjacent to GetAllDevices (constructor area?)
            "0x1c28840",
            "0x1c28900",
            "0x1c28a00",
            "0x1c28a80",

            // DevicesManager constructor/init area
            // Near 0x1c228db ref
            "0x1c22780",
            "0x1c22b90",
            "0x1c22d00",

            // Functions between AddFakeDevice ref and GetVidPid
            "0x1c23a00",
            "0x1c23a40",
            "0x1c23b00",
            "0x1c23c00",

            // Additional: look at what calls GetAllDevices
            // The function at 0x1c18440 (CheckForConnectionThread) likely calls it

            // DevicesManager::Initialize or constructor
            // Look near start of DevicesManager.cpp code area
            "0x1c228db",
            "0x1c22600",
            "0x1c22700",
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        java.util.Set<String> seen = new java.util.HashSet<>();

        for (String addrStr : addrs) {
            Address addr = af.getAddress(addrStr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                continue; // silently skip missing functions
            }
            String key = func.getEntryPoint().toString();
            if (seen.contains(key)) {
                continue;
            }
            seen.add(key);

            println("=== FUNCTION @ " + func.getEntryPoint() + " (name: " + func.getName() + ") ===");
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
