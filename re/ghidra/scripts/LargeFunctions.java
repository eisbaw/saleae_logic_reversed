//@category RE
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import java.util.*;

public class LargeFunctions extends GhidraScript {
    @Override
    public void run() throws Exception {
        FunctionManager fm = currentProgram.getFunctionManager();
        List<Function> large = new ArrayList<>();

        FunctionIterator iter = fm.getFunctions(true);
        while (iter.hasNext()) {
            Function f = iter.next();
            if (f.getName().startsWith("FUN_") && f.getBody().getNumAddresses() > 500) {
                large.add(f);
            }
        }

        large.sort((a, b) -> Long.compare(b.getBody().getNumAddresses(), a.getBody().getNumAddresses()));

        println("=== TOP 100 LARGE UNNAMED FUNCTIONS ===");
        int count = 0;
        for (Function f : large) {
            if (count++ >= 100) break;
            println(String.format("  %8d bytes  %s @ %s",
                f.getBody().getNumAddresses(), f.getName(), f.getEntryPoint()));
        }
        println("Total large unnamed (>500 bytes): " + large.size());
    }
}
