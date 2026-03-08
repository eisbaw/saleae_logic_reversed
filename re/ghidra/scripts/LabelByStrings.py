# Label functions by tracing string references to known library patterns.
#
# Ghidra's built-in FunctionID databases (.fidb) ship only with Windows
# Visual Studio signatures.  For a Linux GCC-compiled binary like
# libgraph_server_shared.so, FunctionID will have very low coverage.
#
# This script supplements FunctionID by:
#   1. Scanning all defined strings for known library-specific patterns.
#   2. Tracing XREF from each matched string back to the containing function.
#   3. Labeling those functions (and their callers, up to a configurable depth)
#      with plate comments attributing them to a library.
#   4. Exporting a report of what was labeled and what remains.
#
# This is a heuristic approach -- it will not catch every library function,
# but it reliably identifies the "core" of each statically-linked library
# which is sufficient to carve them out as noise.
#
# Usage (headless):
#   ghidra-analyzeHeadless <project_dir> <project_name> \
#       -process libgraph_server_shared.so \
#       -postScript LabelByStrings.py [output_dir]
#
# Usage (interactive):
#   Run from Ghidra Script Manager.
#
# @category FunctionID
# @author saleae_reverse project

import os
import time
from collections import defaultdict, OrderedDict

from ghidra.program.model.symbol import RefType, SourceType


# ── Library string signatures ────────────────────────────────────────────
# Each entry: (pattern, library_label, is_prefix)
# is_prefix=True means the string must START with the pattern.
# is_prefix=False means the string must CONTAIN the pattern.
#
# Patterns are case-sensitive to reduce false positives.

STRING_SIGNATURES = [
    # libusb
    ("libusb: error",               "LIBUSB",   True),
    ("libusb: warning",             "LIBUSB",   True),
    ("libusb: info",                "LIBUSB",   True),
    ("libusb: debug",               "LIBUSB",   True),
    ("libusb_",                     "LIBUSB",   True),
    ("LIBUSB_",                     "LIBUSB",   True),
    ("/libusb/",                    "LIBUSB",   False),

    # OpenSSL / libcrypto / libssl
    ("OpenSSL ",                    "OPENSSL",  True),
    ("openssl/",                    "OPENSSL",  False),
    ("OPENSSL_",                    "OPENSSL",  True),
    ("SSL_",                        "OPENSSL",  True),
    ("EVP_",                        "OPENSSL",  True),
    ("RSA_",                        "OPENSSL",  True),
    ("BIO_",                        "OPENSSL",  True),
    ("X509_",                       "OPENSSL",  True),
    ("PEM_",                        "OPENSSL",  True),
    ("ASN1_",                       "OPENSSL",  True),
    ("PKCS",                        "OPENSSL",  True),
    ("HMAC_",                       "OPENSSL",  True),
    ("SHA256_",                     "OPENSSL",  True),
    ("AES_",                        "OPENSSL",  True),

    # Python / CPython
    ("Python ",                     "PYTHON",   True),
    ("cpython",                     "PYTHON",   False),
    ("PyObject",                    "PYTHON",   False),
    ("PyErr_",                      "PYTHON",   True),
    ("PyDict_",                     "PYTHON",   True),
    ("PyList_",                     "PYTHON",   True),
    ("PyTuple_",                    "PYTHON",   True),
    ("PyLong_",                     "PYTHON",   True),
    ("PyUnicode_",                  "PYTHON",   True),
    ("PyModule_",                   "PYTHON",   True),
    ("PyImport_",                   "PYTHON",   True),
    ("PyType_",                     "PYTHON",   True),
    ("Py_Initialize",               "PYTHON",   True),
    ("Py_Finalize",                 "PYTHON",   True),
    ("_Py",                         "PYTHON",   True),
    ("Python/C API",                "PYTHON",   False),
    ("Modules/",                    "PYTHON",   True),
    ("Objects/",                    "PYTHON",   True),
    ("Python/",                     "PYTHON",   True),
    ("python3.",                    "PYTHON",   False),
    ("importlib",                   "PYTHON",   False),

    # Tcl
    ("Tcl_",                        "TCL",      True),
    ("TCL_",                        "TCL",      True),
    ("tclVar.c",                    "TCL",      False),
    ("tclProc.c",                   "TCL",      False),
    ("tclBasic.c",                  "TCL",      False),
    ("tclExecute.c",                "TCL",      False),
    ("tclIO.c",                     "TCL",      False),
    ("generic/tcl",                 "TCL",      False),

    # SQLite
    ("sqlite3",                     "SQLITE",   True),
    ("SQLITE_",                     "SQLITE",   True),

    # BerkeleyDB
    ("Berkeley DB",                 "BDB",      True),
    ("BDB",                         "BDB",      True),
    ("__db_",                       "BDB",      True),
    ("__bam_",                      "BDB",      True),
    ("__ham_",                      "BDB",      True),
    ("__heap_",                     "BDB",      True),
    ("__lock_",                     "BDB",      True),
    ("__log_",                      "BDB",      True),
    ("__memp_",                     "BDB",      True),
    ("__txn_",                      "BDB",      True),
    ("__rep_",                      "BDB",      True),

    # lzma / xz
    ("lzma_",                       "LZMA",     True),
    ("LZMA_",                       "LZMA",     True),
    ("liblzma",                     "LZMA",     False),
    ("XZ Utils",                    "LZMA",     False),

    # zlib
    ("zlib",                        "ZLIB",     False),
    ("deflate",                     "ZLIB",     True),
    ("inflate",                     "ZLIB",     True),
    ("gzip",                        "ZLIB",     True),
    (" 1.2.",                       "ZLIB",     False),  # version string
]

# How many caller levels to propagate the label upward.
# 0 = only the function directly referencing the string.
# 1 = also label its immediate callers.
PROPAGATION_DEPTH = 0


def match_string(s):
    """Check if a string matches any library signature. Returns label or None."""
    for pattern, label, is_prefix in STRING_SIGNATURES:
        if is_prefix:
            if s.startswith(pattern):
                return label
        else:
            if pattern in s:
                return label
    return None


def run():
    println("[STR] Starting string-based library labeling on: %s" % currentProgram.getName())

    output_dir = "."
    args = getScriptArgs()
    if args and len(args) > 0 and args[0]:
        output_dir = args[0]
        if not os.path.isdir(output_dir):
            os.makedirs(output_dir)

    listing = currentProgram.getListing()
    fm = currentProgram.getFunctionManager()
    ref_mgr = currentProgram.getReferenceManager()
    mem = currentProgram.getMemory()

    # Step 1: Scan all defined strings
    println("[STR] Step 1: Scanning defined strings...")
    data_iter = listing.getDefinedData(True)
    string_matches = []  # (addr, string_value, library_label)

    count = 0
    for data in data_iter:
        if monitor.isCancelled():
            println("[STR] Cancelled by user.")
            return
        dt = data.getDataType()
        dt_name = dt.getName() if dt else ""
        if "string" not in dt_name.lower() and "char" not in dt_name.lower():
            continue
        try:
            val = data.getValue()
            if val is None:
                continue
            s = str(val)
            if len(s) < 4:
                continue
            label = match_string(s)
            if label:
                string_matches.append((data.getAddress(), s, label))
                count += 1
        except:
            continue

    println("[STR] Found %d library-associated strings." % count)

    # Step 2: Trace XREFs from matched strings to functions
    println("[STR] Step 2: Tracing string references to functions...")
    func_labels = {}  # addr_str -> (func, set_of_labels)

    for str_addr, str_val, lib_label in string_matches:
        if monitor.isCancelled():
            return
        refs = ref_mgr.getReferencesTo(str_addr)
        for ref in refs:
            from_addr = ref.getFromAddress()
            func = fm.getFunctionContaining(from_addr)
            if func is None:
                continue
            entry = func.getEntryPoint().toString()
            if entry not in func_labels:
                func_labels[entry] = (func, set())
            func_labels[entry][1].add(lib_label)

    println("[STR] Identified %d functions via string references." % len(func_labels))

    # Step 3: Optionally propagate labels to callers
    if PROPAGATION_DEPTH > 0:
        println("[STR] Step 3: Propagating labels to callers (depth=%d)..." % PROPAGATION_DEPTH)
        for depth in range(PROPAGATION_DEPTH):
            new_labels = {}
            for addr_str, (func, labels) in func_labels.items():
                # Find callers of this function
                entry = func.getEntryPoint()
                refs = ref_mgr.getReferencesTo(entry)
                for ref in refs:
                    if ref.getReferenceType().isCall():
                        caller_addr = ref.getFromAddress()
                        caller_func = fm.getFunctionContaining(caller_addr)
                        if caller_func is None:
                            continue
                        caller_entry = caller_func.getEntryPoint().toString()
                        if caller_entry not in func_labels and caller_entry not in new_labels:
                            new_labels[caller_entry] = (caller_func, set(labels))
                        elif caller_entry in new_labels:
                            new_labels[caller_entry][1].update(labels)
            func_labels.update(new_labels)
            println("[STR]   Depth %d: %d new functions" % (depth + 1, len(new_labels)))
    else:
        println("[STR] Step 3: Skipped (propagation depth = 0).")

    # Step 4: Apply labels as plate comments
    println("[STR] Step 4: Applying labels...")
    tx_id = currentProgram.startTransaction("Label functions by string references")
    labeled_count = 0
    try:
        for addr_str, (func, labels) in func_labels.items():
            existing = func.getComment() or ""
            # Build label tag like [OPENSSL,ZLIB]
            sorted_labels = sorted(labels)
            tag = "[%s]" % ",".join(sorted_labels)
            if tag not in existing:
                if existing:
                    func.setComment("%s %s" % (tag, existing))
                else:
                    func.setComment(tag)
                labeled_count += 1
        currentProgram.endTransaction(tx_id, True)
        println("[STR] Labeled %d functions." % labeled_count)
    except Exception as e:
        currentProgram.endTransaction(tx_id, False)
        printerr("[STR] Labeling failed: %s" % str(e))

    # Step 5: Generate summary
    println("[STR] Step 5: Generating summary...")
    total_funcs = fm.getFunctionCount()

    # Count functions by label
    by_lib = defaultdict(int)
    by_lib_bytes = defaultdict(int)
    for addr_str, (func, labels) in func_labels.items():
        size = func.getBody().getNumAddresses()
        for lbl in labels:
            by_lib[lbl] += 1
            by_lib_bytes[lbl] += size

    # Also count functions with plate comments from ApplyFunctionID.py
    already_labeled = 0
    for func in fm.getFunctions(True):
        comment = func.getComment() or ""
        if comment.startswith("[") and "]" in comment:
            already_labeled += 1

    println("[STR] Summary:")
    println("[STR]   Total functions:           %d" % total_funcs)
    println("[STR]   Labeled (this pass):       %d" % labeled_count)
    println("[STR]   Total with library labels: %d" % already_labeled)
    println("[STR]   Remaining unlabeled:       %d" % (total_funcs - already_labeled))
    for lbl in sorted(by_lib.keys()):
        println("[STR]     %-20s: %5d funcs, %10d bytes" % (
            lbl, by_lib[lbl], by_lib_bytes[lbl]
        ))

    # Write report
    report_path = os.path.join(output_dir, "string-label-report.txt")
    try:
        with open(report_path, "w") as f:
            f.write("String-Based Library Labeling Report\n")
            f.write("=" * 60 + "\n")
            f.write("Binary: %s\n" % currentProgram.getName())
            f.write("Date:   %s\n" % time.strftime("%Y-%m-%d %H:%M:%S"))
            f.write("\n")
            f.write("Total functions:           %d\n" % total_funcs)
            f.write("Labeled (this pass):       %d\n" % labeled_count)
            f.write("Total with library labels: %d\n" % already_labeled)
            f.write("Remaining unlabeled:       %d\n" % (total_funcs - already_labeled))
            f.write("\n")
            f.write("Library Breakdown\n")
            f.write("-" * 60 + "\n")
            for lbl in sorted(by_lib.keys()):
                f.write("  %-20s  %5d functions  %10d bytes\n" % (
                    lbl, by_lib[lbl], by_lib_bytes[lbl]
                ))
            f.write("\n")

            # List all labeled functions grouped by library
            f.write("Labeled Functions (by library)\n")
            f.write("-" * 60 + "\n")
            by_lib_funcs = defaultdict(list)
            for addr_str, (func, labels) in func_labels.items():
                for lbl in labels:
                    by_lib_funcs[lbl].append((func.getName(), addr_str))
            for lbl in sorted(by_lib_funcs.keys()):
                f.write("\n[%s]\n" % lbl)
                for name, addr in sorted(by_lib_funcs[lbl]):
                    f.write("  %-50s  %s\n" % (name[:50], addr))

        println("[STR] Report written to: %s" % report_path)
    except Exception as e:
        printerr("[STR] Failed to write report: %s" % str(e))

    println("[STR] Done.")


run()
