// Count functions and print first 10
//@category RE
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;

public class CountFunctions extends GhidraScript {
    @Override
    public void run() throws Exception {
        FunctionManager fm = currentProgram.getFunctionManager();
        println("Function count: " + fm.getFunctionCount());
        FunctionIterator it = fm.getFunctions(true);
        int i = 0;
        while (it.hasNext() && i < 10) {
            Function f = it.next();
            println("  " + f.getName() + " @ " + f.getEntryPoint());
            i++;
        }
    }
}
