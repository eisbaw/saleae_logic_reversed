// Decompile specific downsampler helper functions by address
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileDownsampler2 extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Ghidra addresses (ELF VA + 0x100000)
        // These are called from the downsampler HandlePipeData
        String[][] targets = new String[][] {
            // FUN_01bc7fd0: called to create the downsampler object
            // FUN_01bc7fd0(&local_60, sample_rate_as_long, (long)(double)downsample_ratio)
            {"01bc7fd0", "AnalogDownsampler_Create"},
            // FUN_01bc92e0: called to allocate output buffer
            // FUN_01bc92e0(&local_d8, uVar13, uVar12) -- output count, remainder
            {"01bc92e0", "AnalogDownsampler_AllocOutput"},
            // FUN_01bc90c0: fallthrough handler for unknown pipe data
            {"01bc90c0", "AnalogDownsampler_HandleUnknownPipeData"},
            // FUN_02d910f0: emits data downstream
            {"02d910f0", "Kernel_EmitData"},
            // FUN_02d90fd0: base class HandleRequest
            {"02d90fd0", "ConvenienceKernel_HandleRequest"},
            // DownsampleRatioFloor is used in assertions, try to find it
            // The assertion in FUN_02e4a280 references it, let's look at the code
            // near the check at line ~340: uVar1 = param_2[0x174], check alignment
        };

        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Create functions where missing
        int tx = currentProgram.startTransaction("Create downsampler helper functions");
        try {
            for (String[] t : targets) {
                Address addr = af.getAddress(t[0]);
                Function func = fm.getFunctionAt(addr);
                if (func == null) {
                    Function containing = fm.getFunctionContaining(addr);
                    if (containing != null) {
                        println("NOTE: " + t[0] + " (" + t[1] + ") inside " + containing.getName() + " @ " + containing.getEntryPoint());
                    } else {
                        CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                        cmd.applyTo(currentProgram);
                        func = fm.getFunctionAt(addr);
                        if (func != null) {
                            func.setName(t[1], SourceType.USER_DEFINED);
                            println("CREATED: " + t[1] + " @ " + t[0]);
                        } else {
                            println("FAILED: " + t[0] + " (" + t[1] + ")");
                        }
                    }
                } else {
                    println("EXISTS: " + func.getName() + " @ " + t[0]);
                }
            }
        } finally {
            currentProgram.endTransaction(tx, true);
        }

        // Decompile
        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            Function func = fm.getFunctionAt(addr);
            if (func == null) func = fm.getFunctionContaining(addr);
            if (func == null) {
                println("=== NO FUNCTION: " + t[0] + " (" + t[1] + ") ===\n");
                continue;
            }
            println("\n=== " + func.getName() + " @ " + func.getEntryPoint() + " (" + t[1] + ") ===");
            println("Size: " + func.getBody().getNumAddresses() + " bytes");
            DecompileResults res = decomp.decompileFunction(func, 180, monitor);
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
