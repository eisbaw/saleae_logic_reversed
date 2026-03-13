---
name: reverse-engineer
description: >
  Reverse engineering workflow for libgraph_server_shared.so. Use when
  decompiling functions, analyzing USB captures, validating hypotheses
  against pcap/hardware, or documenting RE findings. Covers Ghidra
  headless usage, address translation, documentation tiers, and the
  hypothesis-to-knowledge pipeline.
user-invocable: true
---

# Reverse Engineering Workflow

## Project Context

- **Binary**: `libgraph_server_shared.so` (48MB, stripped, x86-64 ELF)
  - Statically links libusb, OpenSSL, Python, Tcl, SQLite, BerkeleyDB, lzma, zlib
  - Located at `$SALEAE_PATH/resources/linux-x64/libgraph_server_shared.so`
- **Source path** (from assertion strings): `/workspace/monorepo/graph-io/devices/usb_device/src/`
- **Hardware**: Logic Pro 8 (21a9:1005) attached locally (Bus 003, USB 2.0)

## Address Translation

Ghidra rebases the binary by **+0x100000** vs objdump/ELF virtual addresses.

```
ghidra_addr = elf_va + 0x100000
elf_va      = ghidra_addr - 0x100000
```

| Symbol                | objdump (ELF VA) | Ghidra VA    |
|-----------------------|------------------|--------------|
| CreateGraphServer     | 0x1a01780        | 0x1b01780    |
| SimpleEncryption wrap | 0x2d97ec0        | 0x2e97ec0    |

**Always state which address space** when documenting addresses. Prefer ELF VAs
in documentation and backlog tasks (they match objdump and are tool-independent).

## Ghidra Headless Usage

### Environment

Nix provides `ghidra-analyzeHeadless` (NOT `analyzeHeadless`).
The project is already imported and analyzed:

- **Project dir**: `ghidra_proj2/sal_rev2`
- **Scripts dir**: `re/ghidra/scripts/`

### Invocation

```bash
# Run a script against the already-imported project (no re-import, no re-analysis)
ghidra-analyzeHeadless ghidra_proj2 sal_rev2 \
  -process libgraph_server_shared.so -noanalysis \
  -scriptPath re/ghidra/scripts \
  -postScript MyScript.java
```

Key flags:
- `-process <binary>` -- work on already-imported binary (do NOT use `-import`)
- `-noanalysis` -- skip re-analysis (already done)
- `-scriptPath` -- directory containing scripts
- `-postScript` -- script to run after loading

### Writing Ghidra Scripts

Use Java (`.java` in `re/ghidra/scripts/`). Java is more reliable than Python
for headless Ghidra. All scripts extend `GhidraScript`.

#### Template: Decompile functions at known addresses

```java
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class MyDecompile extends GhidraScript {
    @Override
    public void run() throws Exception {
        // Use Ghidra addresses (ELF VA + 0x100000), zero-padded hex without 0x
        String[] addrs = new String[] { "02d97ec0", "02d987b0" };

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        for (String a : addrs) {
            Address addr = af.getAddress(a);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) { println("NO FUNCTION at " + a); continue; }
            println("=== " + func.getName() + " @ " + func.getEntryPoint() + " ===");
            DecompileResults res = decomp.decompileFunction(func, 120, monitor);
            if (res.decompileCompleted()) {
                println(res.getDecompiledFunction().getC());
            } else {
                println("FAILED: " + res.getErrorMessage());
            }
        }
        decomp.dispose();
    }
}
```

#### Template: Create function at address and rename

```java
//@category RE
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;

public class MyCreateFunc extends GhidraScript {
    @Override
    public void run() throws Exception {
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();
        Address addr = af.getAddress("02d97ec0");

        int tx = currentProgram.startTransaction("Create function");
        try {
            Function func = fm.getFunctionAt(addr);
            if (func == null) {
                CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
                cmd.applyTo(currentProgram);
                func = fm.getFunctionAt(addr);
            }
            if (func != null) {
                func.setName("my_function_name",
                    ghidra.program.model.symbol.SourceType.USER_DEFINED);
                println("Created: " + func.getName());
            }
        } finally {
            currentProgram.endTransaction(tx, true);
        }
    }
}
```

#### Template: Search for string references

```java
//@category RE
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.*;
import ghidra.program.model.data.*;
import ghidra.program.util.*;

public class MyStringSearch extends GhidraScript {
    @Override
    public void run() throws Exception {
        String target = "your search string";
        for (Data data : currentProgram.getListing().getDefinedData(true)) {
            if (data.getDataType() instanceof StringDataType ||
                data.getDataType().getName().contains("string")) {
                String val = data.getDefaultValueRepresentation();
                if (val != null && val.contains(target)) {
                    println("STRING: " + data.getAddress() + " = " + val);
                    // Show xrefs to this string
                    for (var ref : getReferencesTo(data.getAddress())) {
                        println("  XREF: " + ref.getFromAddress());
                    }
                }
            }
        }
    }
}
```

## Grounding in Reality

**Pcap is ground truth.** Decompilation output is a hypothesis until validated.

Validation chain:
1. **Decompile** -- generate hypothesis about what the code does
2. **objdump** -- verify exact instruction bytes, confirm Ghidra didn't misinterpret
3. **String references** -- assertion messages leak source paths and function names
4. **Pcap wire bytes** -- validate protocol formats, encryption, command sequences
5. **Hardware test** -- final arbiter (`just deploy && just run-remote`)

Cross-reference at least two sources before documenting something as fact.

Validation scripts go in `re/scripts/`:
- Rust for crypto/math validation (compile with `rustc`)
- Python for protocol parsing and pcap analysis

## Documentation Tiers

| Tier | Location | Contents | Certainty |
|------|----------|----------|-----------|
| Backlog tasks | Backlog MCP | Hypotheses, leads, open questions | Speculative |
| RE notes | `re/notes/` | Working analysis, may be wrong | Medium |
| Docs | `doc/` | Verified, established knowledge | High |

Rules:
- **Create a backlog task immediately** when discovering a new lead (unknown symbol, unexplained data, interesting pattern)
- Use backlog MCP tools -- never edit backlog `.md` files directly
- `re/notes/` files can be messy and contradictory -- that's fine
- `doc/` requires validation against pcap or hardware before promotion
- `doc/` files should include a confidence assessment table and key VAs table

## Hypothesis Pipeline

```
1. Discover clue
   (string ref, unusual pattern, pcap anomaly, unknown USB command)
        |
2. Create backlog task
   (hypothesis + expected evidence + what would confirm/refute)
        |
3. Investigate
   (Ghidra decompilation, objdump, pcap analysis, string search)
        |
4. Validate against pcap or hardware
   (cross-reference wire bytes, run on device)
        |
   +----+----+
   |         |
5a. Confirmed          5b. Refuted
   - Promote to doc/      - Update backlog notes
   - Mark task Done        - Record what was wrong and why
   - Update MEMORY.md      - Close or pivot task
```

## Key Project Paths

| Path | Purpose |
|------|---------|
| `ghidra_proj2/sal_rev2` | Ghidra project (imported+analyzed) |
| `re/ghidra/scripts/` | Ghidra headless scripts (Java) |
| `re/notes/` | Working RE analysis notes |
| `re/captures/` | USB pcap captures |
| `re/scripts/` | Validation scripts (Rust, Python) |
| `doc/` | Established, verified documentation |
| `$SALEAE_PATH/resources/linux-x64/` | Extracted Saleae binary + resources |
