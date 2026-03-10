// Find and decompile functions related to graph.cpp, node.cpp, subscription_manager.cpp
// Uses string references to find functions
// String file offsets == ELF VA for rodata section (LOAD segment 0).
// Ghidra VA = ELF VA + 0x100000
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.mem.*;
import java.util.*;

public class DecompileGraphArch2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // File offset = ELF VA for rodata (first LOAD segment maps offset 0 -> VA 0).
        // Ghidra VA = ELF VA + 0x100000.
        //
        // String file offsets from `strings -t x`:
        // 3e01a5 = "A node cannot be given Id 0; this Id is reserved for the Graph"
        // 39068c = "Cannot remove node that still has pipe connections"
        // 3906bf = "it != mNodes.end()"
        // 390665 = "Graph input port is already occupied: "
        // 3bdc80 = "Graph output port is already occupied: "
        // 411f35 = "Node with specified id already exists: "
        // 411f5d = "void Saleae::Graph::Graph::SyncNodeState(Saleae::Graph::NodeId)"
        // 3e01f6 = "failed to locate node in graph to issue stop execution"
        // 3e0263 = "Unknown kernel type: "
        // 43caa4 = "A pipe in mConnectedOutputs should always exist in mExternalPipeDock"
        // 3d18d9 = "Node with specified id does not exist"
        // 4270a1 = "Node does not exist at the provided id"
        // 3824d1 = "SubscriptionManager::Poll"
        // 39f6d4 = "Only Stopped->Invalid..."
        // 3d9a60 = "Stopped->Stopped is not a valid..."
        // 3e1aea = "Only Stopped->Executing..."
        // 3776bf = "Node with the specified mNodeId does not exist"
        // 3e0077 = "Graph Actions: "
        // 4186b8 = "port_name == \"in\""
        // 3d8533 = "Task (RemoveNode)"

        // Convert: Ghidra VA = file_offset + 0x100000
        String[] stringAddrs = new String[] {
            "0x4e01a5",  // "A node cannot be given Id 0; this Id is reserved for the Graph"
            "0x49068c",  // "Cannot remove node that still has pipe connections"
            "0x4906bf",  // "it != mNodes.end()"
            "0x490665",  // "Graph input port is already occupied: "
            "0x4bdc80",  // "Graph output port is already occupied: "
            "0x511f35",  // "Node with specified id already exists: "
            "0x511f5d",  // "void Saleae::Graph::Graph::SyncNodeState..."
            "0x4e01f6",  // "failed to locate node in graph to issue stop execution"
            "0x4e0263",  // "Unknown kernel type: "
            "0x53caa4",  // "A pipe in mConnectedOutputs should always exist in mExternalPipeDock"
            "0x4d18d9",  // "Node with specified id does not exist"
            "0x5270a1",  // "Node does not exist at the provided id"
            "0x4824d1",  // "SubscriptionManager::Poll"
            "0x49f6d4",  // "Only Stopped->Invalid is a valid transition..."
            "0x4d9a60",  // "Stopped->Stopped is not a valid..."
            "0x4e1aea",  // "Only Stopped->Executing is a valid transition..."
            "0x4876bf",  // "Node with the specified mNodeId does not exist"
            "0x4e0077",  // "Graph Actions: "
            "0x5186b8",  // "port_name == \"in\""
            "0x4d8533",  // "Task (RemoveNode)"
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager refMgr = currentProgram.getReferenceManager();

        // Collect unique functions that reference these strings
        Map<String, Set<String>> funcToStrings = new LinkedHashMap<>();

        for (String addrStr : stringAddrs) {
            Address strAddr = af.getAddress(addrStr);
            if (strAddr == null) {
                println("--- Cannot parse address: " + addrStr + " ---");
                continue;
            }

            // Read string at this address for labeling
            Memory mem = currentProgram.getMemory();
            byte[] buf = new byte[120];
            try {
                mem.getBytes(strAddr, buf);
            } catch (Exception e) {
                println("--- Cannot read memory at " + addrStr + " ---");
                continue;
            }
            String label = new String(buf).split("\0")[0];
            if (label.length() > 80) label = label.substring(0, 80) + "...";

            // Find all references TO this string address
            ReferenceIterator refIter = refMgr.getReferencesTo(strAddr);
            List<Reference> refList = new ArrayList<>();
            while (refIter.hasNext()) {
                refList.add(refIter.next());
            }
            if (refList.isEmpty()) {
                println("--- No references to " + addrStr + " (\"" + label + "\") ---");
                continue;
            }

            println("// String at " + addrStr + ": \"" + label + "\"");
            println("//   Referenced by " + refList.size() + " location(s)");

            for (Reference ref : refList) {
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func == null) {
                    println("//   ref from " + fromAddr + " - no function");
                    continue;
                }

                String funcKey = func.getEntryPoint().toString();
                if (!funcToStrings.containsKey(funcKey)) {
                    funcToStrings.put(funcKey, new LinkedHashSet<>());
                }
                funcToStrings.get(funcKey).add(label);
                println("//   ref from " + fromAddr + " -> " + func.getName() + " @ " + func.getEntryPoint());
            }
        }

        // Decompile each unique function
        println("\n\n========== DECOMPILED FUNCTIONS ==========\n");
        for (Map.Entry<String, Set<String>> entry : funcToStrings.entrySet()) {
            String funcAddr = entry.getKey();
            Set<String> strings = entry.getValue();

            Address addr = af.getAddress(funcAddr);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) continue;

            // Calculate ELF VA (Ghidra VA - 0x100000)
            long ghidraVA = func.getEntryPoint().getOffset();
            long elfVA = ghidraVA - 0x100000;

            println("=== " + func.getName() + " @ Ghidra:" + func.getEntryPoint()
                    + " ELF:0x" + Long.toHexString(elfVA) + " ===");
            println("// Referenced strings:");
            for (String s : strings) {
                println("//   \"" + s + "\"");
            }

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
