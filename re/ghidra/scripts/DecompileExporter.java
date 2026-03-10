// Decompile export_node/exporter functions by tracing string references
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.mem.*;

import java.util.*;

public class DecompileExporter extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Key strings from export_node/exporter.cpp and related code
        // These are ELF VAs (= file offsets for .rodata) + 0x100000 = Ghidra VA
        // The strings tool reported file offsets; for this binary .rodata VA == file offset
        // Ghidra VA = ELF VA + 0x100000
        String[] stringAddrs = new String[] {
            // exporter.cpp assertion path
            "0x4b7410",  // /workspace/monorepo/graph-io/nodes/export_node/src/exporter.cpp
            // Key export strings
            "0x462e10",  // digital.csv
            "0x4f5106",  // analog.csv
            "0x4b06f5",  // cannot export data if some analog channels have no data
            "0x4bdced",  // at least 1 channel must be specified for export
            "0x4bdd1d",  // failed to export: there is no available data to export
            "0x4e0308",  // cannot start export while export in progress
            "0x4e0335",  // cannot export data, an analog store is not ready
            "0x50a7c5",  // cannot export data, a digital store is not ready
            "0x45574f",  // Exporter error: {}
            "0x4fc66d",  // start of export thread
            "0x49e3e7",  // end of export thread
            "0x518735",  // Exporter was canceled, deleting files
            "0x4032f0",  // Exporting
            "0x4706fc",  // export paths not available while export is still running
            "0x48947c",  // RunExportOnThread
            "0x45572b",  // Failed while exporting digital data
            "0x5032b9",  // Failed while writing to analog.csv
            // ExportNode/ExportNodeState strings
            "0x44eba3",  // Saleae::Graph::ExportNode
            "0x45c543",  // Saleae::Graph::ExportActions::ExportData
            "0x462df1",  // Saleae::Graph::ExportNodeState
            // ExportWaveformCSV
            "0x454ccb",  // ExportWaveformCSV not implemented
            // Export state fields
            "0x488b02",  // ExportReady
            "0x48fbfd",  // ExportNotReady
            // Key field paths used in serialization
            "0x4b6e8d",  // /csvPath
            "0x46fc5c",  // /outputPath
            "0x4bdbe1",  // /filepath
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();

        Set<String> decompiled = new HashSet<>();

        for (String addrStr : stringAddrs) {
            Address strAddr = af.getAddress(addrStr);
            if (strAddr == null) {
                println("--- INVALID ADDRESS: " + addrStr + " ---");
                continue;
            }

            // Get the string at this address for context
            Data data = getDataAt(strAddr);
            String strValue = "";
            if (data != null) {
                Object val = data.getValue();
                if (val != null) strValue = val.toString();
            }

            // Find all references TO this string address
            Reference[] refs = getReferencesTo(strAddr);
            if (refs == null || refs.length == 0) {
                println("--- NO XREFS to string at " + addrStr + " (" + strValue + ") ---");
                continue;
            }

            println("### String at " + addrStr + ": " + strValue);
            println("### " + refs.length + " cross-reference(s)");

            for (Reference ref : refs) {
                Address fromAddr = ref.getFromAddress();
                Function func = fm.getFunctionContaining(fromAddr);
                if (func == null) {
                    println("  xref from " + fromAddr + " -- NOT IN A FUNCTION");
                    continue;
                }

                String funcKey = func.getEntryPoint().toString();
                if (decompiled.contains(funcKey)) {
                    println("  xref from " + fromAddr + " in " + func.getName() + " @ " + funcKey + " (already decompiled)");
                    continue;
                }
                decompiled.add(funcKey);

                // Calculate ELF VA for documentation
                long ghidraVA = func.getEntryPoint().getOffset();
                long elfVA = ghidraVA - 0x100000;

                println("=== " + func.getName() + " @ Ghidra:" + func.getEntryPoint() +
                        " ELF:0x" + Long.toHexString(elfVA) +
                        " (xref from " + fromAddr + ") ===");
                println("  Referenced string: " + strValue);

                DecompileResults res = decomp.decompileFunction(func, 120, monitor);
                if (res.decompileCompleted()) {
                    println(res.getDecompiledFunction().getC());
                } else {
                    println("DECOMPILE FAILED: " + res.getErrorMessage());
                }
                println("");
            }
            println("");
        }
        decomp.dispose();
    }
}
