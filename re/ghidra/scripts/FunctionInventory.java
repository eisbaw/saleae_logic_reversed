//@category RE
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import java.util.*;

public class FunctionInventory extends GhidraScript {
    @Override
    public void run() throws Exception {
        FunctionManager fm = currentProgram.getFunctionManager();

        int total = 0;
        int named = 0;  // user-defined or imported names
        int autonamed = 0;  // FUN_xxxxx
        int thunks = 0;
        Map<String, Integer> prefixCounts = new TreeMap<>();

        FunctionIterator iter = fm.getFunctions(true);
        while (iter.hasNext()) {
            Function f = iter.next();
            total++;
            String name = f.getName();

            if (f.isThunk()) {
                thunks++;
            } else if (name.startsWith("FUN_")) {
                autonamed++;
            } else {
                named++;
                // Count by prefix (first word or namespace)
                String prefix;
                if (name.contains("::")) {
                    prefix = name.substring(0, name.indexOf("::"));
                } else if (name.contains("_")) {
                    prefix = name.substring(0, name.indexOf("_"));
                } else {
                    prefix = name;
                }
                prefixCounts.merge(prefix, 1, Integer::sum);
            }
        }

        println("=== FUNCTION INVENTORY ===");
        println("Total functions: " + total);
        println("Named (user/import): " + named);
        println("Auto-named (FUN_): " + autonamed);
        println("Thunks: " + thunks);
        println("Coverage: " + (named * 100 / total) + "% named");

        println("\n=== TOP 50 NAMED PREFIXES ===");
        prefixCounts.entrySet().stream()
            .sorted(Map.Entry.<String, Integer>comparingByValue().reversed())
            .limit(50)
            .forEach(e -> println(String.format("  %4d  %s", e.getValue(), e.getKey())));
    }
}
