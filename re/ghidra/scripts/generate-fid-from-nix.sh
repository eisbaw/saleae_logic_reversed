#!/usr/bin/env bash
# generate-fid-from-nix.sh -- Build custom Ghidra FunctionID databases from
# nix store libraries.
#
# Ghidra's built-in .fidb files only cover Windows Visual Studio libraries.
# For our Linux target (libgraph_server_shared.so compiled with GCC), we need
# custom FID databases built from the same libraries that were statically
# linked into the binary.
#
# This script:
#   1. Locates the relevant library .so/.a files in the nix store
#   2. Imports each into a temporary Ghidra project
#   3. Runs full analysis on each
#   4. Builds a FunctionID database (.fidb) from the analyzed libraries
#
# The resulting .fidb can be placed in Ghidra's FunctionID data directory
# and will be used by ApplyFunctionID.py.
#
# Prerequisites:
#   - Run inside nix-shell (provides Ghidra, library paths)
#   - Ghidra's headless analyzer: ghidra-analyzeHeadless
#
# Known library versions in libgraph_server_shared.so (from strings):
#   - libusb 1.0.x     (libusb_* symbols present)
#   - OpenSSL 1.1.x    (OpenSSL_* strings, EVP/RSA/BIO patterns)
#   - Python 3.8.2     (cpython-38 string, "3.8.2" version)
#   - Tcl 8.6.10       ("8.6.10" version string)
#   - SQLite 3.31.1    ("3.31.1" version string)
#   - BerkeleyDB 6.0.19 / 12.1.6.0.19 (version strings)
#   - zlib 1.2.11      ("1.2.11" version string)
#   - lzma/xz          (lzma_* patterns)
#
# NOTE: The nix store may have different versions than what's in the binary.
# FunctionID matching is hash-based, so version mismatches reduce coverage.
# For best results, build matching versions from source (see comments below).
#
# Usage:
#   ./generate-fid-from-nix.sh [output_dir]
#
#   output_dir: where to write the .fidb file (default: re/ghidra/fid/)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
OUTPUT_DIR="${1:-$PROJECT_ROOT/re/ghidra/fid}"
WORK_DIR="$(mktemp -d)"
GHIDRA_PROJECT_DIR="$WORK_DIR/ghidra_fid_project"
GHIDRA_PROJECT_NAME="fid_libraries"

# Path to the CreateMultipleLibraries.java script shipped with Ghidra
GHIDRA_FID_SCRIPTS=""
if [ -n "${GHIDRA_INSTALL_DIR:-}" ]; then
    GHIDRA_FID_SCRIPTS="$GHIDRA_INSTALL_DIR/Ghidra/Features/FunctionID/ghidra_scripts"
fi
# Try to find it via which ghidra
if [ -z "$GHIDRA_FID_SCRIPTS" ]; then
    GHIDRA_BIN="$(which ghidra-analyzeHeadless 2>/dev/null || which ghidra 2>/dev/null || true)"
    if [ -n "$GHIDRA_BIN" ]; then
        GHIDRA_BASE="$(dirname "$(dirname "$(readlink -f "$GHIDRA_BIN")")")"
        GHIDRA_FID_SCRIPTS="$GHIDRA_BASE/lib/ghidra/Ghidra/Features/FunctionID/ghidra_scripts"
        if [ ! -d "$GHIDRA_FID_SCRIPTS" ]; then
            # NixOS layout
            GHIDRA_FID_SCRIPTS="$(find "$(dirname "$(readlink -f "$GHIDRA_BIN")")/.." -path "*/FunctionID/ghidra_scripts" -type d 2>/dev/null | head -1 || true)"
        fi
    fi
fi

cleanup() {
    echo "[FID-GEN] Cleaning up temporary directory: $WORK_DIR"
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

echo "================================================================"
echo "  Ghidra FunctionID Database Generator (from nix store)"
echo "================================================================"
echo ""
echo "Output directory: $OUTPUT_DIR"
echo "Work directory:   $WORK_DIR"
echo ""

mkdir -p "$OUTPUT_DIR"
mkdir -p "$GHIDRA_PROJECT_DIR"

# ── Locate libraries ────────────────────────────────────────────────────

# We try to find both .a (static archive, preferred) and .so (shared).
# For FID generation, we need the object code with symbols -- .so works
# if it is not stripped.

declare -A LIBS
LIBS_FOUND=0

find_lib() {
    local name="$1"
    local search_pattern="$2"
    local result=""

    # Try .a first (static archive, has individual .o files)
    result="$(find /nix/store -maxdepth 5 -name "$search_pattern.a" -not -path "*/ghidra/*" 2>/dev/null | head -1 || true)"

    # Fall back to .so
    if [ -z "$result" ]; then
        result="$(find /nix/store -maxdepth 5 -name "$search_pattern.so" -not -path "*/ghidra/*" -not -path "*-saleae-*" 2>/dev/null | head -1 || true)"
    fi

    # Try versioned .so
    if [ -z "$result" ]; then
        result="$(find /nix/store -maxdepth 5 -name "${search_pattern}.so.*" -not -path "*/ghidra/*" -not -path "*-saleae-*" 2>/dev/null | head -1 || true)"
    fi

    if [ -n "$result" ]; then
        LIBS["$name"]="$result"
        LIBS_FOUND=$((LIBS_FOUND + 1))
        printf "  [ok]   %-15s  %s\n" "$name" "$result"
    else
        printf "  [miss] %-15s  not found (pattern: %s)\n" "$name" "$search_pattern"
    fi
}

echo "Locating libraries in nix store..."
echo ""

find_lib "libusb"     "libusb-1.0"
find_lib "libssl"     "libssl"
find_lib "libcrypto"  "libcrypto"
find_lib "libpython"  "libpython3*"
find_lib "libtcl"     "libtcl8*"
find_lib "libsqlite"  "libsqlite3"
find_lib "libdb"      "libdb-*"
find_lib "liblzma"    "liblzma"
find_lib "libz"       "libz"

echo ""
echo "Found $LIBS_FOUND libraries."
echo ""

if [ "$LIBS_FOUND" -eq 0 ]; then
    echo "ERROR: No libraries found. Are you running inside nix-shell?" >&2
    exit 1
fi

# ── Check for analyzeHeadless ────────────────────────────────────────────

if ! command -v ghidra-analyzeHeadless &>/dev/null; then
    echo "ERROR: ghidra-analyzeHeadless not found in PATH." >&2
    echo "Are you running inside nix-shell?" >&2
    exit 1
fi

# ── Step 1: Import and analyze each library ──────────────────────────────

echo "================================================================"
echo "  Step 1: Import and analyze libraries"
echo "================================================================"
echo ""

for lib_name in "${!LIBS[@]}"; do
    lib_path="${LIBS[$lib_name]}"
    echo "--- Importing: $lib_name ($lib_path) ---"

    # Check if the library has symbols (not stripped)
    has_symbols="yes"
    if file "$lib_path" | grep -q "stripped"; then
        echo "  WARNING: $lib_path appears stripped. FID matching will be poor."
        has_symbols="no"
    fi

    # Import into Ghidra project with full analysis
    # -import: import the file
    # -overwrite: overwrite if already exists
    # -analysisTimeoutPerFile: give it plenty of time for large libraries
    ghidra-analyzeHeadless "$GHIDRA_PROJECT_DIR" "$GHIDRA_PROJECT_NAME" \
        -import "$lib_path" \
        -overwrite \
        -analysisTimeoutPerFile 600 \
        -log "$WORK_DIR/${lib_name}_import.log" \
        2>&1 | tail -5

    echo "  Import complete."
    echo ""
done

# ── Step 2: Build FunctionID database ────────────────────────────────────

echo "================================================================"
echo "  Step 2: Build FunctionID database"
echo "================================================================"
echo ""

FIDB_PATH="$OUTPUT_DIR/saleae_linux_libs.fidb"
FIDB_SCRIPT="$SCRIPT_DIR/BuildFidDb.py"

# Write a Ghidra Python script that builds the FID database from the
# imported programs.
cat > "$FIDB_SCRIPT" << 'PYEOF'
# BuildFidDb.py -- Ghidra script to populate a FunctionID database
# from all programs in the current project.
#
# This is run by generate-fid-from-nix.sh after importing reference
# libraries.  It is NOT intended for standalone use.
#
# @category FunctionID

import os
import sys
from java.io import File

from ghidra.feature.fid.db import FidFileManager, FidFile
from ghidra.feature.fid.service import FidService

def run():
    println("[BUILD-FID] Building FunctionID database...")

    args = getScriptArgs()
    if not args or len(args) < 1:
        printerr("[BUILD-FID] Usage: BuildFidDb.py <output.fidb>")
        return

    fidb_path = args[0]
    println("[BUILD-FID] Output: %s" % fidb_path)

    fid_service = FidService()
    fid_file_mgr = FidFileManager.getInstance()

    # Create a new FID file
    fidb_file = File(fidb_path)
    try:
        fid_db = fid_file_mgr.createNewFidDatabase(fidb_file)
        println("[BUILD-FID] Created new FID database.")
    except Exception as e:
        println("[BUILD-FID] FID database creation failed: %s" % str(e))
        println("[BUILD-FID] This may require running from the Ghidra GUI.")
        println("[BUILD-FID] See the manual workflow in re/notes/library-map.md.")
        return

    # Populate from current program
    try:
        lib_name = currentProgram.getName()
        lib_version = "1.0"  # placeholder
        lib_variant = "linux-x64"

        fid_db.beginTransaction()
        library = fid_db.createNewLibrary(lib_name, lib_version, lib_variant)

        fm = currentProgram.getFunctionManager()
        count = 0
        for func in fm.getFunctions(True):
            if monitor.isCancelled():
                break
            name = func.getName()
            # Skip thunks and unnamed functions
            if name.startswith("FUN_") or name.startswith("thunk_"):
                continue
            try:
                fid_db.addFunction(library, func, monitor)
                count += 1
            except:
                pass

        fid_db.endTransaction(True)
        println("[BUILD-FID] Added %d functions from %s" % (count, lib_name))
    except Exception as e:
        printerr("[BUILD-FID] Population failed: %s" % str(e))
    finally:
        try:
            fid_db.close()
        except:
            pass

    println("[BUILD-FID] Done.")

run()
PYEOF

echo "NOTE: Building a FunctionID database from imported programs requires"
echo "the Ghidra FID API which has limited headless support."
echo ""
echo "The recommended workflow is:"
echo ""
echo "  1. Import reference libraries (done above)."
echo "  2. Open the Ghidra project: $GHIDRA_PROJECT_DIR/$GHIDRA_PROJECT_NAME"
echo "  3. In Ghidra GUI: Tools -> Function ID -> Create new empty FidDb..."
echo "  4. For each imported library: Tools -> Function ID -> Populate FidDb from programs"
echo "  5. Save the .fidb to: $FIDB_PATH"
echo ""
echo "Alternatively, try the headless approach (may not work with all Ghidra versions):"
echo ""

# Attempt headless FID build -- this may fail depending on Ghidra version
# and the specific FID API available.
for lib_name in "${!LIBS[@]}"; do
    echo "  Processing $lib_name..."
    ghidra-analyzeHeadless "$GHIDRA_PROJECT_DIR" "$GHIDRA_PROJECT_NAME" \
        -process "${lib_name}*" \
        -noanalysis \
        -postScript "$FIDB_SCRIPT" "$FIDB_PATH" \
        -log "$WORK_DIR/${lib_name}_fid.log" \
        2>&1 | tail -3 || echo "  (headless FID build not available for $lib_name)"
    echo ""
done

echo "================================================================"
echo "  Summary"
echo "================================================================"
echo ""
echo "Ghidra project: $GHIDRA_PROJECT_DIR/$GHIDRA_PROJECT_NAME"
echo "Libraries imported: $LIBS_FOUND"

if [ -f "$FIDB_PATH" ]; then
    echo "FID database: $FIDB_PATH"
    echo ""
    echo "To use this database:"
    echo "  1. Copy to Ghidra's FunctionID data directory, OR"
    echo "  2. In Ghidra: Tools -> Function ID -> Attach existing FidDb..."
    echo "     and select: $FIDB_PATH"
else
    echo ""
    echo "Headless FID database generation was not successful."
    echo "Use the Ghidra GUI workflow described above."
fi

echo ""
echo "Next step: Run ApplyFunctionID.py and LabelByStrings.py on the target:"
echo ""
echo "  ghidra-analyzeHeadless re/ghidra saleae_re \\"
echo "    -process libgraph_server_shared.so \\"
echo "    -postScript ApplyFunctionID.py re/ghidra/reports/ \\"
echo "    -postScript LabelByStrings.py re/ghidra/reports/"
echo ""
echo "================================================================"
