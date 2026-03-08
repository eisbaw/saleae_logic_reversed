# Apply FunctionID databases to label statically-linked library functions.
#
# This script is designed to run inside Ghidra's headless analyzer or the
# interactive Script Manager.  It:
#
#   1. Applies all enabled FunctionID databases (built-in + any custom .fidb
#      files placed in Ghidra's FunctionID data directory).
#   2. Labels matched functions with a "LIB_<library>" prefix/plate comment.
#   3. Exports a summary report of identified vs unidentified functions.
#
# Usage (headless):
#   ghidra-analyzeHeadless <project_dir> <project_name> \
#       -process libgraph_server_shared.so \
#       -postScript ApplyFunctionID.py [output_dir]
#
# Usage (interactive):
#   Run from Ghidra Script Manager -- output goes to the console.
#
# The optional argument is a directory for the summary report.  Defaults
# to the directory containing the analyzed program file.
#
# @category FunctionID
# @author saleae_reverse project

import os
import sys
import time
from collections import defaultdict, OrderedDict

from ghidra.app.script import GhidraScript
from ghidra.feature.fid.service import FidService
from ghidra.program.model.listing import FunctionIterator
from ghidra.program.model.symbol import SourceType


def get_output_dir(script):
    """Determine where to write the report."""
    args = script.getScriptArgs()
    if args and len(args) > 0 and args[0]:
        out = args[0]
        if not os.path.isdir(out):
            os.makedirs(out)
        return out
    # Default: same directory as the program file
    prog_path = script.currentProgram.getExecutablePath()
    if prog_path:
        d = os.path.dirname(prog_path)
        if os.path.isdir(d):
            return d
    return os.getcwd()


# ── Known library identification patterns ────────────────────────────────
# Maps FunctionID library name substrings -> canonical short names.
# FunctionID library names vary by .fidb source, so we match substrings.
LIBRARY_PATTERNS = OrderedDict([
    ("libusb",      "LIBUSB"),
    ("usb",         "LIBUSB"),
    ("openssl",     "OPENSSL"),
    ("libssl",      "OPENSSL"),
    ("libcrypto",   "OPENSSL"),
    ("ssl",         "OPENSSL"),
    ("crypto",      "OPENSSL"),
    ("python",      "PYTHON"),
    ("cpython",     "PYTHON"),
    ("tcl",         "TCL"),
    ("sqlite",      "SQLITE"),
    ("berkeleydb",  "BDB"),
    ("libdb",       "BDB"),
    ("db-",         "BDB"),
    ("lzma",        "LZMA"),
    ("liblzma",     "LZMA"),
    ("xz",          "LZMA"),
    ("zlib",        "ZLIB"),
    ("libz",        "ZLIB"),
    ("deflate",     "ZLIB"),
])


def classify_fid_match(library_name):
    """Map a FunctionID library name to a canonical label."""
    lower = library_name.lower()
    for pattern, label in LIBRARY_PATTERNS.items():
        if pattern in lower:
            return label
    return "UNKNOWN_LIB"


def run_function_id(script):
    """Trigger FunctionID analysis via the FidService."""
    fid_service = FidService()

    script.println("[FID] Populating FunctionID results...")
    try:
        # Apply FunctionID to all functions in the program.
        # This calls into Ghidra's FunctionID analyzer which matches
        # function hashes against all enabled .fidb databases.
        fid_service.analyzeProgram(
            script.currentProgram,
            script.monitor
        )
        script.println("[FID] FunctionID analysis complete.")
        return True
    except Exception as e:
        script.println("[FID] FunctionID analysis failed: %s" % str(e))
        script.println("[FID] Falling back to existing FID results (if any).")
        return False


def collect_results(script):
    """Walk all functions and classify them as identified or not."""
    fm = script.currentProgram.getFunctionManager()
    listing = script.currentProgram.getListing()

    total = 0
    identified = 0
    by_library = defaultdict(list)
    unidentified = []

    functions = fm.getFunctions(True)  # forward iterator
    for func in functions:
        total += 1
        name = func.getName()
        source = func.getSymbol().getSource()

        # FunctionID sets source to IMPORTED or ANALYSIS when it matches.
        # Default auto-analysis names like FUN_xxxxx have source USER_DEFINED
        # or DEFAULT.  We also check for plate comments set by FID.
        plate = func.getComment()

        is_identified = False
        lib_label = None

        # Check if FunctionID matched this function
        if source in (SourceType.IMPORTED, SourceType.ANALYSIS):
            is_identified = True
            # Try to determine which library from the plate comment or name
            if plate:
                lib_label = classify_fid_match(plate)
            if lib_label is None or lib_label == "UNKNOWN_LIB":
                lib_label = classify_fid_match(name)

        # Also check for functions that have meaningful names (not FUN_xxxx)
        elif not name.startswith("FUN_") and not name.startswith("thunk_FUN_"):
            # Could be a dynamic symbol or previously labeled
            if source == SourceType.IMPORTED:
                is_identified = True
                lib_label = classify_fid_match(name)

        if is_identified:
            identified += 1
            if lib_label is None:
                lib_label = "UNKNOWN_LIB"
            by_library[lib_label].append({
                "name": name,
                "addr": func.getEntryPoint().toString(),
                "size": func.getBody().getNumAddresses(),
            })
        else:
            unidentified.append({
                "name": name,
                "addr": func.getEntryPoint().toString(),
                "size": func.getBody().getNumAddresses(),
            })

    return {
        "total": total,
        "identified": identified,
        "unidentified_count": len(unidentified),
        "by_library": dict(by_library),
        "unidentified": unidentified,
    }


def label_functions(script, results):
    """Add plate comments and namespace prefixes to identified functions."""
    fm = script.currentProgram.getFunctionManager()
    sym_table = script.currentProgram.getSymbolTable()
    tx_id = script.currentProgram.startTransaction("Label library functions")
    try:
        for lib_label, funcs in results["by_library"].items():
            for finfo in funcs:
                addr = script.currentProgram.getAddressFactory().getAddress(
                    finfo["addr"]
                )
                func = fm.getFunctionAt(addr)
                if func is None:
                    continue

                # Set plate comment with library attribution
                existing_comment = func.getComment() or ""
                lib_comment = "[%s]" % lib_label
                if lib_comment not in existing_comment:
                    if existing_comment:
                        func.setComment("%s %s" % (lib_comment, existing_comment))
                    else:
                        func.setComment(lib_comment)

        script.currentProgram.endTransaction(tx_id, True)
        script.println("[FID] Labeled %d functions across %d libraries." % (
            results["identified"],
            len(results["by_library"]),
        ))
    except Exception as e:
        script.currentProgram.endTransaction(tx_id, False)
        script.printerr("[FID] Labeling failed: %s" % str(e))


def write_report(script, results, output_dir):
    """Write a summary report to disk."""
    report_path = os.path.join(output_dir, "fid-report.txt")
    try:
        with open(report_path, "w") as f:
            f.write("FunctionID Analysis Report\n")
            f.write("=" * 60 + "\n")
            f.write("Binary: %s\n" % script.currentProgram.getName())
            f.write("Date:   %s\n" % time.strftime("%Y-%m-%d %H:%M:%S"))
            f.write("\n")
            f.write("Total functions:      %d\n" % results["total"])
            f.write("Identified (FID):     %d\n" % results["identified"])
            f.write("Unidentified:         %d\n" % results["unidentified_count"])
            if results["total"] > 0:
                pct = 100.0 * results["identified"] / results["total"]
                f.write("Coverage:             %.1f%%\n" % pct)
            f.write("\n")

            f.write("Library Breakdown\n")
            f.write("-" * 60 + "\n")
            for lib_label in sorted(results["by_library"].keys()):
                funcs = results["by_library"][lib_label]
                total_bytes = sum(fi["size"] for fi in funcs)
                f.write("  %-20s  %5d functions  %10d bytes\n" % (
                    lib_label, len(funcs), total_bytes
                ))
            f.write("\n")

            # Top unidentified functions by size (likely Saleae business logic)
            f.write("Top 50 Unidentified Functions (by size)\n")
            f.write("-" * 60 + "\n")
            sorted_unid = sorted(
                results["unidentified"],
                key=lambda x: x["size"],
                reverse=True,
            )
            for fi in sorted_unid[:50]:
                f.write("  %-40s  %s  %6d bytes\n" % (
                    fi["name"][:40], fi["addr"], fi["size"]
                ))
            f.write("\n")

            # Full list of identified functions per library
            f.write("Identified Functions (by library)\n")
            f.write("-" * 60 + "\n")
            for lib_label in sorted(results["by_library"].keys()):
                f.write("\n[%s]\n" % lib_label)
                funcs = sorted(
                    results["by_library"][lib_label],
                    key=lambda x: x["name"],
                )
                for fi in funcs:
                    f.write("  %-50s  %s  %6d bytes\n" % (
                        fi["name"][:50], fi["addr"], fi["size"]
                    ))

        script.println("[FID] Report written to: %s" % report_path)
    except Exception as e:
        script.printerr("[FID] Failed to write report: %s" % str(e))


# ── Main entry point (GhidraScript.run()) ────────────────────────────────

def run():
    """Main script logic -- called by Ghidra's script runner."""
    # 'currentProgram', 'monitor', 'println', etc. are injected by Ghidra
    # into the module namespace when running as a GhidraScript.

    println("[FID] Starting FunctionID analysis on: %s" % currentProgram.getName())

    output_dir = "."
    args = getScriptArgs()
    if args and len(args) > 0 and args[0]:
        output_dir = args[0]
        if not os.path.isdir(output_dir):
            os.makedirs(output_dir)

    # Step 1: Run FunctionID analysis
    println("[FID] Step 1: Running FunctionID matching...")
    fid_service = FidService()
    try:
        fid_service.analyzeProgram(currentProgram, monitor)
        println("[FID] FunctionID matching complete.")
    except Exception as e:
        println("[FID] FunctionID matching failed (may need custom .fidb): %s" % str(e))
        println("[FID] Proceeding with existing analysis results...")

    # Step 2: Collect and classify results
    println("[FID] Step 2: Collecting results...")
    fm = currentProgram.getFunctionManager()
    total = 0
    identified = 0
    by_library = defaultdict(list)
    unidentified = []

    for func in fm.getFunctions(True):
        total += 1
        name = func.getName()
        source = func.getSymbol().getSource()
        plate = func.getComment()

        is_identified = False
        lib_label = None

        if source in (SourceType.IMPORTED, SourceType.ANALYSIS):
            is_identified = True
            if plate:
                lib_label = classify_fid_match(plate)
            if lib_label is None or lib_label == "UNKNOWN_LIB":
                lib_label = classify_fid_match(name)
        elif not name.startswith("FUN_") and not name.startswith("thunk_FUN_"):
            if source == SourceType.IMPORTED:
                is_identified = True
                lib_label = classify_fid_match(name)

        if is_identified:
            identified += 1
            if lib_label is None:
                lib_label = "UNKNOWN_LIB"
            by_library[lib_label].append({
                "name": name,
                "addr": func.getEntryPoint().toString(),
                "size": func.getBody().getNumAddresses(),
            })
        else:
            unidentified.append({
                "name": name,
                "addr": func.getEntryPoint().toString(),
                "size": func.getBody().getNumAddresses(),
            })

    results = {
        "total": total,
        "identified": identified,
        "unidentified_count": len(unidentified),
        "by_library": dict(by_library),
        "unidentified": unidentified,
    }

    println("[FID] Found %d total functions, %d identified (%.1f%%)" % (
        total, identified,
        100.0 * identified / total if total > 0 else 0,
    ))
    for lib_label in sorted(by_library.keys()):
        println("[FID]   %-20s: %d functions" % (lib_label, len(by_library[lib_label])))

    # Step 3: Label functions with library plate comments
    println("[FID] Step 3: Labeling functions...")
    tx_id = currentProgram.startTransaction("Label library functions")
    try:
        for lib_label, funcs in by_library.items():
            for finfo in funcs:
                addr = currentProgram.getAddressFactory().getAddress(finfo["addr"])
                func = fm.getFunctionAt(addr)
                if func is None:
                    continue
                existing_comment = func.getComment() or ""
                lib_comment = "[%s]" % lib_label
                if lib_comment not in existing_comment:
                    if existing_comment:
                        func.setComment("%s %s" % (lib_comment, existing_comment))
                    else:
                        func.setComment(lib_comment)
        currentProgram.endTransaction(tx_id, True)
        println("[FID] Labeling complete.")
    except Exception as e:
        currentProgram.endTransaction(tx_id, False)
        printerr("[FID] Labeling failed: %s" % str(e))

    # Step 4: Write report
    println("[FID] Step 4: Writing report...")
    report_path = os.path.join(output_dir, "fid-report.txt")
    try:
        with open(report_path, "w") as f:
            f.write("FunctionID Analysis Report\n")
            f.write("=" * 60 + "\n")
            f.write("Binary: %s\n" % currentProgram.getName())
            f.write("Date:   %s\n" % time.strftime("%Y-%m-%d %H:%M:%S"))
            f.write("\n")
            f.write("Total functions:      %d\n" % total)
            f.write("Identified (FID):     %d\n" % identified)
            f.write("Unidentified:         %d\n" % results["unidentified_count"])
            if total > 0:
                pct = 100.0 * identified / total
                f.write("Coverage:             %.1f%%\n" % pct)
            f.write("\n")

            f.write("Library Breakdown\n")
            f.write("-" * 60 + "\n")
            for lib_label in sorted(by_library.keys()):
                funcs = by_library[lib_label]
                total_bytes = sum(fi["size"] for fi in funcs)
                f.write("  %-20s  %5d functions  %10d bytes\n" % (
                    lib_label, len(funcs), total_bytes
                ))
            f.write("\n")

            f.write("Top 50 Unidentified Functions (by size)\n")
            f.write("-" * 60 + "\n")
            sorted_unid = sorted(unidentified, key=lambda x: x["size"], reverse=True)
            for fi in sorted_unid[:50]:
                f.write("  %-40s  %s  %6d bytes\n" % (
                    fi["name"][:40], fi["addr"], fi["size"]
                ))
            f.write("\n")

            f.write("Identified Functions (by library)\n")
            f.write("-" * 60 + "\n")
            for lib_label in sorted(by_library.keys()):
                f.write("\n[%s]\n" % lib_label)
                funcs_sorted = sorted(by_library[lib_label], key=lambda x: x["name"])
                for fi in funcs_sorted:
                    f.write("  %-50s  %s  %6d bytes\n" % (
                        fi["name"][:50], fi["addr"], fi["size"]
                    ))

        println("[FID] Report written to: %s" % report_path)
    except Exception as e:
        printerr("[FID] Failed to write report: %s" % str(e))

    println("[FID] Done.")


# Ghidra Python scripts execute at module level
run()
