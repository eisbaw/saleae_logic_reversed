# Library Map: libgraph_server_shared.so

Status: template -- to be populated after running FunctionID analysis.

## Binary Overview

| Property         | Value                                                 |
|------------------|-------------------------------------------------------|
| File             | libgraph_server_shared.so                             |
| Size             | 49,762,176 bytes (~48 MB)                             |
| Type             | ELF64, x86-64, shared object                          |
| Stripped          | Yes (no debug symbols)                                |
| Compiler          | GCC (Linux, likely Ubuntu/CentOS build environment)   |
| Nix store path   | /nix/store/brcaw41j62pjki70ahf7ffd2f3bhaa3d-.../resources/linux-x64/ |

## Statically-Linked Libraries

Identified from string analysis and symbol patterns.

| Library       | Version in Binary | Nix Store Version | Version Match | Notes                        |
|---------------|-------------------|--------------------|---------------|------------------------------|
| libusb        | 1.0.x             | 1.0.27             | Close         | libusb_* symbols visible     |
| OpenSSL       | 1.1.x             | 3.3.2              | MISMATCH      | OpenSSL 3.x is a major rewrite; low FID coverage expected |
| Python        | 3.8.2             | 3.12.8             | MISMATCH      | cpython-38 string confirms 3.8.x |
| Tcl           | 8.6.10            | (not in nix)       | N/A           | Tcl_* patterns visible       |
| SQLite        | 3.31.1            | 3.50.1             | MISMATCH      | sqlite3* strings visible     |
| BerkeleyDB    | 6.0.19 (12.1.6)   | (not in nix)       | N/A           | Berkeley DB version strings  |
| zlib          | 1.2.11            | 1.3.1              | Close         | "1.2.11" version string      |
| lzma/xz       | unknown           | (not in nix)       | N/A           | lzma_* patterns visible      |

### Version Mismatch Impact

FunctionID matching is hash-based: it hashes the function body bytes and
compares against a database. Even minor compiler flag differences or version
bumps will change the hash. The version mismatches above (especially OpenSSL
1.1 vs 3.x, Python 3.8 vs 3.12) mean FunctionID will have very low coverage
for those libraries.

Mitigation strategies:
1. **String-based labeling** (LabelByStrings.py) -- does not depend on binary hashes.
2. **Build matching versions** -- use nix to build the exact library versions and
   generate FID databases from those.
3. **Symbol-based matching** -- some functions are exported or have partial DWARF
   info that Ghidra can use.

## Address Ranges

To be populated after analysis. Format:

```
Start       End         Size        Library        Functions
----------  ----------  ----------  -------------  ---------
0x????????  0x????????  ??? KB      LIBUSB         ??
0x????????  0x????????  ??? KB      OPENSSL        ??
0x????????  0x????????  ??? KB      PYTHON         ??
0x????????  0x????????  ??? KB      TCL            ??
0x????????  0x????????  ??? KB      SQLITE         ??
0x????????  0x????????  ??? KB      BDB            ??
0x????????  0x????????  ??? KB      ZLIB           ??
0x????????  0x????????  ??? KB      LZMA           ??
0x????????  0x????????  ??? KB      SALEAE         ??
```

Note: statically-linked libraries are NOT necessarily contiguous in the
binary. The linker interleaves sections (.text, .rodata, etc.) across all
object files. The "address ranges" above are approximations based on the
majority of each library's functions.

## Function Counts

To be populated from fid-report.txt and string-label-report.txt.

| Category          | Count | % of Total | Bytes       |
|-------------------|-------|------------|-------------|
| Total functions   |       |            |             |
| Identified (FID)  |       |            |             |
| Labeled (strings) |       |            |             |
| Unlabeled         |       |            |             |

### By Library

| Library   | Functions | Bytes      | Identification Method |
|-----------|-----------|------------|-----------------------|
| LIBUSB    |           |            |                       |
| OPENSSL   |           |            |                       |
| PYTHON    |           |            |                       |
| TCL       |           |            |                       |
| SQLITE    |           |            |                       |
| BDB       |           |            |                       |
| ZLIB      |           |            |                       |
| LZMA      |           |            |                       |
| OTHER     |           |            |                       |

## Analysis Workflow

### Step 1: Generate FID databases (optional, low expected yield)

```bash
just ghidra-gen-sigs
```

This imports nix store library .so files into Ghidra, analyzes them, and
attempts to build a FunctionID database. Due to version mismatches, expect
low coverage.

### Step 2: Import and analyze the target binary

```bash
just ghidra-import
```

This imports libgraph_server_shared.so into a Ghidra project and runs
auto-analysis (including any enabled FunctionID databases).

### Step 3: Run FunctionID matching

```bash
just ghidra-fid
```

This runs ApplyFunctionID.py and LabelByStrings.py on the target binary.
Both scripts label functions with plate comments like `[OPENSSL]`,
`[PYTHON]`, etc.

### Step 4: Review reports

Reports are written to `re/ghidra/reports/`:
- `fid-report.txt` -- FunctionID matching results
- `string-label-report.txt` -- string-based labeling results

### Step 5: Update this file

After running the analysis, update the address ranges and function counts
in this file with actual data from the reports.

## Building Matching Library Versions

For maximum FID coverage, build the exact library versions from source using
nix. Example for Python 3.8.2:

```nix
# In shell.nix or a separate nix expression:
let
  pkgs = import <nixpkgs> {};
  python38 = pkgs.python38.overrideAttrs (old: {
    version = "3.8.2";
    src = pkgs.fetchurl {
      url = "https://www.python.org/ftp/python/3.8.2/Python-3.8.2.tar.xz";
      sha256 = "...";
    };
  });
in ...
```

This is a significant effort and should only be done if string-based
labeling proves insufficient for isolating Saleae business logic.
