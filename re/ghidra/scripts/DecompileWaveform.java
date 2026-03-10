// Decompile waveform persistence functions (WaveformStoreNode, WaveformDataCollection,
// WaveformSummaryNode, serialization/deserialization)
// @category Analysis
// @author claude

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceIterator;
import ghidra.program.model.symbol.ReferenceManager;

import java.io.File;
import java.io.PrintWriter;
import java.util.LinkedHashSet;
import java.util.Set;

public class DecompileWaveform extends GhidraScript {

    // Ghidra VAs = ELF string offset + 0x100000
    private static final long[] STRING_ADDRS = {
        // Source file paths (assert __FILE__)
        0x468ca9L, // waveform_store_node.cpp
        0x4e64c6L, // waveform_summary_node.cpp
        0x53aa23L, // waveform_data_collection.cpp

        // WaveformStoreNode log/error messages
        0x4620a9L, // WaveformStoreNode::OnStartDeserialization: File is empty
        0x462247L, // WaveformDataCollection::WaveformDataCollection(Configuration) PRETTY_FUNCTION
        0x46fb6fL, // WaveformStoreNode::OnStartSerialization: serialization complete. total size =
        0x488a24L, // WaveformStoreNode::CanStartSerialization: failed
        0x488a7cL, // WaveformStoreNode::OnStartDeserialization: failed
        0x4969e2L, // WaveformStoreNode::OnStartDeserialization: Deserialization exception encountered. unknown error
        0x4b6967L, // WaveformStoreNode::OnStartSerialization: File not found
        0x4b69a2L, // WaveformStoreNode::DeserializeOnThread: Done
        0x4c4035L, // WaveformStoreNode::OnStartDeserialization: exception
        0x4df58eL, // WaveformStoreNode::OnStartSerialization: Creating serialization thread
        0x4f43f4L, // WaveformStoreNode::OnStartSerialization: Serialization exception encountered. exception
        0x511339L, // WaveformStoreNode::OnStartDeserialization: File not found
        0x517d19L, // WaveformStoreNode::DeserializeOnThread -
        0x52648bL, // WaveformStoreNode::CanStartSerialization: joining previous thread
        0x52d316L, // WaveformStoreNode::OnStartSerialization: Serialization exception encountered
        0x53a882L, // WaveformStoreNode::OnStartDeserialization: Creating deserialization thread

        // WaveformDataCollection assertion strings
        0x488b96L, // PackedWaveformGetStats PRETTY_FUNCTION
        0x488c21L, // data.mSampleInterval.Length() == data.GetSampleCount()
        0x496ae8L, // ChunkIterator::GetStats PRETTY_FUNCTION
        0x4df6f5L, // offset_into_stats_bytes < data.mAnalogStats.size_bytes()
        0x4df72eL, // WaveformIterator::GetStats PRETTY_FUNCTION
        0x4e665bL, // data.mAnalogStats.size() > 0

        // WaveformDataCollection Writer PushBack
        0x4fb621L, // Writer::PushBack PRETTY_FUNCTION

        // waveform_store_api assertion
        0x468c57L, // waveform_store_api != nullptr

        // WaveformStoreState / AnalogStoreState2 typeinfo
        0x4c4093L, // Saleae::Graph::WaveformStoreState
        0x4ee8afL, // Saleae::Graph::AnalogStoreState2

        // Serialization infrastructure
        0x4599e8L, // StartSerializeCrcAndLength PRETTY_FUNCTION
        0x459a4dL, // FinishSerializeCrcAndLength PRETTY_FUNCTION
        0x534400L, // ByteBuffer_SerializeBytes
        0x476cddL, // SerializeMagicToken PRETTY_FUNCTION
        0x48fe01L, // Invalid magic token found in metadata
        0x4fb77bL, // strlen(expected_magic_token) == SaleaeMagicTokenLength
        0x4a4f24L, // InvalidMagicToken

        // Downsample / WaveformSummary
        0x4550b5L, // Setting sampling config, downsample=...
        0x46908cL, // new_sampling_config.mDownsampleRatio == ...
        0x4697e6L, // !mDownsampler
        0x4705f4L, // Received sampling config with downsampler already initialized
        0x47d889L, // Saleae::Graph::WaveformSummaryNode
        0x4a4c1eL, // HandleRenderRequestData PRETTY_FUNCTION
        0x4a55cbL, // Saleae::Graph::AnalogDownsamplerNode
        0x4b6e7dL, // /downsampleMode
        0x4c49f9L, // HandlePipeData for AnalogDownsamplerNode
        0x4d1998L, // /analogDownsample
        0x4e0279L, // Cannot set downsampler rate after execution
        0x4edd40L, // new_sampling_config.mDownsampleMode != DownsampleMode::MinMax
        0x5270c8L, // analog_downsampler_node.cpp
    };

    @Override
    public void run() throws Exception {
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        FunctionManager funcMgr = currentProgram.getFunctionManager();
        ReferenceManager refMgr = currentProgram.getReferenceManager();

        Set<Function> functions = new LinkedHashSet<>();

        // Find all functions referencing these strings
        for (long addr : STRING_ADDRS) {
            Address strAddr = toAddr(addr);
            ReferenceIterator refs = refMgr.getReferencesTo(strAddr);
            while (refs.hasNext()) {
                Reference ref = refs.next();
                Address fromAddr = ref.getFromAddress();
                Function func = funcMgr.getFunctionContaining(fromAddr);
                if (func != null) {
                    functions.add(func);
                }
            }
        }

        println("Found " + functions.size() + " functions referencing waveform strings");

        // Also find callers (one level up)
        Set<Function> callers = new LinkedHashSet<>();
        for (Function func : functions) {
            Address entryPoint = func.getEntryPoint();
            ReferenceIterator refs = refMgr.getReferencesTo(entryPoint);
            while (refs.hasNext()) {
                Reference ref = refs.next();
                if (ref.getReferenceType().isCall()) {
                    Function caller = funcMgr.getFunctionContaining(ref.getFromAddress());
                    if (caller != null && !functions.contains(caller)) {
                        callers.add(caller);
                    }
                }
            }
        }

        println("Found " + callers.size() + " direct callers");

        // Output file
        String outPath = System.getProperty("user.dir") + "/re/ghidra/decompiled-waveform.txt";
        File outFile = new File(outPath);
        PrintWriter pw = new PrintWriter(outFile);

        pw.println("// ================================================================");
        pw.println("// Decompiled waveform persistence functions");
        pw.println("// WaveformStoreNode, WaveformDataCollection, WaveformSummaryNode");
        pw.println("// Serialization: SaleaeBinaryInputArchive / SaleaeBinaryOutputArchive");
        pw.println("// Saleae Logic 2 v2.4.40 - libgraph_server_shared.so");
        pw.println("// Generated by DecompileWaveform.java");
        pw.println("// ================================================================");
        pw.println();

        // Decompile core functions
        pw.println("// ================================================================");
        pw.println("// CORE FUNCTIONS (directly reference waveform/serialization strings)");
        pw.println("// ================================================================");
        pw.println();

        int count = 0;
        for (Function func : functions) {
            long elfVA = func.getEntryPoint().getOffset() - 0x100000L;
            pw.println("// ----------------------------------------------------------------");
            pw.println("// Function at Ghidra VA 0x" + Long.toHexString(func.getEntryPoint().getOffset()));
            pw.println("// ELF VA: 0x" + Long.toHexString(elfVA));
            pw.println("// Name: " + func.getName());
            pw.println("// Size: " + func.getBody().getNumAddresses() + " bytes");
            pw.println("// ----------------------------------------------------------------");

            DecompileResults results = decomp.decompileFunction(func, 120, monitor);
            if (results.decompileCompleted()) {
                pw.println(results.getDecompiledFunction().getC());
            } else {
                pw.println("// DECOMPILE FAILED: " + results.getErrorMessage());
            }
            pw.println();
            count++;
        }

        // Decompile callers (limit to reasonable number)
        pw.println("// ================================================================");
        pw.println("// CALLERS (one level up from core functions)");
        pw.println("// ================================================================");
        pw.println();

        int callerCount = 0;
        for (Function func : callers) {
            if (callerCount >= 60) {
                pw.println("// ... truncated " + (callers.size() - callerCount) + " more callers");
                break;
            }
            long elfVA = func.getEntryPoint().getOffset() - 0x100000L;
            pw.println("// ----------------------------------------------------------------");
            pw.println("// Caller at Ghidra VA 0x" + Long.toHexString(func.getEntryPoint().getOffset()));
            pw.println("// ELF VA: 0x" + Long.toHexString(elfVA));
            pw.println("// Name: " + func.getName());
            pw.println("// Size: " + func.getBody().getNumAddresses() + " bytes");
            pw.println("// ----------------------------------------------------------------");

            DecompileResults results = decomp.decompileFunction(func, 120, monitor);
            if (results.decompileCompleted()) {
                pw.println(results.getDecompiledFunction().getC());
            } else {
                pw.println("// DECOMPILE FAILED: " + results.getErrorMessage());
            }
            pw.println();
            count++;
            callerCount++;
        }

        pw.close();
        decomp.dispose();
        println("Decompiled " + count + " functions to: " + outPath);
    }
}
