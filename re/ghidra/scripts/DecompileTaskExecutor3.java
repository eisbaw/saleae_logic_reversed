// Decompile worker thread function and task scheduling
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.SourceType;

public class DecompileTaskExecutor3 extends GhidraScript {
    @Override
    public void run() throws Exception {
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        // Ghidra VAs = ELF + 0x100000
        String[][] targets = new String[][] {
            // Worker thread function called from StartThreads
            {"02de0c40", "WorkerThreadFunc"},
            // Notify/schedule helper
            {"02de0220", "TaskExecutor_Notify"},
            // Thread creation wrapper
            {"02de11b0", "TaskExecutor_CreateThread"},
            // ScheduleTask from TaskHandleInternals
            {"02dac040", "TaskExecutor_EnqueueTask"},
            // Dequeue helper called from task enqueue (FUN_02dae3c0)
            {"02dae3c0", "TaskQueue_Grow"},
            // Task state management
            {"02dad630", "TaskQueue_Lock"},
        };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        decomp.setSimplificationStyle("decompile");

        for (String[] t : targets) {
            Address addr = af.getAddress(t[0]);
            Function func = fm.getFunctionAt(addr);
            if (func == null) {
                int tx = currentProgram.startTransaction("Create function");
                try {
                    CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                    cmd.applyTo(currentProgram);
                    func = fm.getFunctionAt(addr);
                    if (func != null) {
                        func.setName(t[1], SourceType.USER_DEFINED);
                    }
                } finally {
                    currentProgram.endTransaction(tx, true);
                }
            }
            if (func == null) {
                func = fm.getFunctionContaining(addr);
                if (func != null) {
                    println("=== " + t[0] + " inside " + func.getName() + " @ " + func.getEntryPoint() + " ===");
                } else {
                    println("=== NO FUNCTION at " + t[0] + " (" + t[1] + ") ===");
                    continue;
                }
            }

            long elfVA = func.getEntryPoint().getOffset() - 0x100000;
            println("=== " + func.getName() + " @ Ghidra:" + func.getEntryPoint() +
                    " ELF:0x" + Long.toHexString(elfVA) +
                    " (" + t[1] + ")" +
                    " (size:" + func.getBody().getNumAddresses() + ") ===");

            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("");
        }

        decomp.dispose();
        println("=== DONE ===");
    }
}
