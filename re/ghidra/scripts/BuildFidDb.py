# Build a FunctionID database from the current program's analyzed functions.
#
# This script is called by generate-fid-from-nix.sh for each reference
# library imported into the Ghidra project.  It adds all named (non-thunk,
# non-default) functions to a .fidb database.
#
# Usage:
#   ghidra-analyzeHeadless <project> <name> \
#       -process <library> \
#       -noanalysis \
#       -postScript BuildFidDb.py <output.fidb>
#
# Arguments:
#   output.fidb -- path to the FunctionID database file (created if missing)
#
# NOTE: The FidFileManager API has limited headless support in some Ghidra
# versions.  If this script fails, use the Ghidra GUI workflow:
#   Tools -> Function ID -> Create new empty FidDb...
#   Tools -> Function ID -> Populate FidDb from programs...
#
# @category FunctionID
# @author saleae_reverse project

import os
from java.io import File

try:
    from ghidra.feature.fid.db import FidFileManager
    from ghidra.feature.fid.service import FidService
    HAS_FID_API = True
except ImportError:
    HAS_FID_API = False


def run():
    args = getScriptArgs()
    if not args or len(args) < 1:
        printerr("[BUILD-FID] Usage: BuildFidDb.py <output.fidb>")
        return

    fidb_path = args[0]
    println("[BUILD-FID] Output: %s" % fidb_path)
    println("[BUILD-FID] Program: %s" % currentProgram.getName())

    if not HAS_FID_API:
        printerr("[BUILD-FID] FunctionID API not available in this Ghidra build.")
        printerr("[BUILD-FID] Use the GUI workflow instead.")
        return

    fid_file_mgr = FidFileManager.getInstance()
    fidb_file = File(fidb_path)

    # Open existing or create new
    fid_db = None
    try:
        if fidb_file.exists():
            println("[BUILD-FID] Opening existing FID database...")
            fid_db = fid_file_mgr.openFidDatabase(fidb_file)
        else:
            println("[BUILD-FID] Creating new FID database...")
            fid_db = fid_file_mgr.createNewFidDatabase(fidb_file)
    except Exception as e:
        printerr("[BUILD-FID] Failed to open/create FID database: %s" % str(e))
        printerr("[BUILD-FID] Use the GUI workflow instead:")
        printerr("[BUILD-FID]   Tools -> Function ID -> Create new empty FidDb...")
        printerr("[BUILD-FID]   Tools -> Function ID -> Populate FidDb from programs...")
        return

    lib_name = currentProgram.getName()
    # Strip extension for cleaner library name
    if "." in lib_name:
        lib_name = lib_name.rsplit(".", 1)[0]
    lib_version = "1.0"
    lib_variant = "linux-x64-gcc"

    try:
        fid_db.beginTransaction()
        library = fid_db.createNewLibrary(lib_name, lib_version, lib_variant)

        fm = currentProgram.getFunctionManager()
        added = 0
        skipped = 0
        for func in fm.getFunctions(True):
            if monitor.isCancelled():
                println("[BUILD-FID] Cancelled.")
                break

            name = func.getName()
            # Skip auto-named and thunk functions
            if name.startswith("FUN_") or name.startswith("thunk_"):
                skipped += 1
                continue
            # Skip tiny functions (likely padding or error stubs)
            if func.getBody().getNumAddresses() < 6:
                skipped += 1
                continue

            try:
                fid_db.addFunction(library, func, monitor)
                added += 1
            except Exception as e:
                skipped += 1

        fid_db.endTransaction(True)
        println("[BUILD-FID] Added %d functions, skipped %d from %s" % (
            added, skipped, currentProgram.getName()
        ))
    except Exception as e:
        printerr("[BUILD-FID] Population failed: %s" % str(e))
        try:
            fid_db.endTransaction(False)
        except:
            pass
    finally:
        try:
            fid_db.close()
        except:
            pass

    println("[BUILD-FID] Done.")


run()
