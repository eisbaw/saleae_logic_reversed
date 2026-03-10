// Find and decompile functions related to graph.cpp, node.cpp, subscription_manager.cpp
// Uses string references to find functions
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.mem.*;
import java.util.*;

public class DecompileGraphArch extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Key strings to search for -- these appear in graph.cpp, node.cpp, subscription_manager.cpp
        // ELF VA addresses (add 0x100000 for Ghidra VA)
        // From strings -t x output:
        // 3e022d = graph.cpp path
        // 43ca6a = node.cpp path
        // 3ee5e0 = subscription_manager.cpp path
        //
        // Key assertion/error strings near these files:
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

        // ELF VAs of the strings -> Ghidra VAs (+0x100000)
        String[] stringAddrs = new String[] {
            "0x4f01a5",  // "A node cannot be given Id 0; this Id is reserved for the Graph"
            "0x4a068c",  // "Cannot remove node that still has pipe connections"
            "0x4a06bf",  // "it != mNodes.end()"
            "0x4a0665",  // "Graph input port is already occupied: "
            "0x4bdc80",  // "Graph output port is already occupied: "
            "0x521f35",  // "Node with specified id already exists: "
            "0x521f5d",  // "void Saleae::Graph::Graph::SyncNodeState..."
            "0x4f01f6",  // "failed to locate node in graph to issue stop execution"
            "0x4f0263",  // "Unknown kernel type: "
            "0x53caa4",  // "A pipe in mConnectedOutputs should always exist in mExternalPipeDock"
            "0x4d18d9",  // "Node with specified id does not exist"
            "0x5370a1",  // "Node does not exist at the provided id"
            "0x4924d1",  // "SubscriptionManager::Poll"
            "0x4af6d4",  // "Only Stopped->Invalid is a valid transition..."
            "0x4d9a60",  // "Stopped->Stopped is not a valid..."
            "0x4f1aea",  // "Only Stopped->Executing is a valid transition..."
            "0x4876bf",  // "Node with the specified mNodeId does not exist"
            "0x4f0077",  // "Graph Actions: "
            "0x4ee593",  // "GraphServer"
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager refMgr = currentProgram.getReferenceManager();

        // Collect unique functions that reference these strings
        Set<String> decompiled = new LinkedHashSet<>();
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

            // Calculate ELF VA
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
