# Export decompiled C code for all non-library (Saleae-owned) functions.
#
# After ApplyFunctionID.py and LabelByStrings.py have labeled statically-linked
# library functions, this script decompiles the remaining functions and exports
# them as individual .c files for AI-assisted analysis.
#
# Output structure:
#   <output_dir>/
#     index.txt              # Master list: addr | name | size | file
#     by-address/
#       FUN_00xxxxxx.c       # Individual decompiled functions
#     by-class/              # Functions grouped by detected class name
#       ClassName/
#         MethodName.c -> ../by-address/FUN_00xxxxxx.c  (symlinks)
#     xrefs.txt              # Cross-reference graph: caller -> callee
#
# Usage (headless):
#   ghidra-analyzeHeadless re/ghidra saleae_re \
#       -process libgraph_server_shared.so \
#       -noanalysis \
#       -scriptPath re/ghidra/scripts \
#       -postScript ExportDecompiled.py re/ghidra/decompiled
#
# Usage (interactive):
#   Run from Ghidra Script Manager.  Pass output dir as script argument.
#
# @category Export
# @author saleae_reverse project

import os
import re
import time
from collections import defaultdict

from ghidra.app.decompiler import DecompInterface, DecompileOptions
from ghidra.program.model.symbol import RefType


# ── Known library labels applied by ApplyFunctionID.py / LabelByStrings.py ──
# Functions with plate comments containing any of these tags are skipped.
LIBRARY_LABELS = frozenset([
    "LIBUSB", "OPENSSL", "PYTHON", "TCL", "SQLITE", "BDB", "LZMA", "ZLIB",
    "UNKNOWN_LIB",
])

# Function name prefixes that indicate library functions even without plate
# comments (e.g., dynamic symbols or thunks).
LIBRARY_NAME_PREFIXES = [
    # libusb
    "libusb_", "usbi_",
    # OpenSSL
    "SSL_", "ssl_", "EVP_", "evp_", "RSA_", "rsa_", "BIO_", "bio_",
    "X509_", "x509_", "PEM_", "ASN1_", "asn1_", "PKCS", "HMAC_", "hmac_",
    "SHA256_", "sha256_", "SHA1_", "sha1_", "AES_", "aes_", "DES_", "des_",
    "BN_", "bn_", "EC_", "ec_", "ECDSA_", "DH_", "dh_", "DSA_",
    "CRYPTO_", "crypto_", "OPENSSL_", "openssl_", "ERR_", "OBJ_", "RAND_",
    "ENGINE_", "CONF_", "NCONF_", "OCSP_", "CMS_", "TS_", "CT_",
    # Python
    "Py", "_Py", "PySys_", "PyOS_",
    # Tcl
    "Tcl_", "TclP", "TclB", "Tcl8",
    # SQLite
    "sqlite3", "sqlite3_",
    # BerkeleyDB
    "__db_", "__bam_", "__ham_", "__heap_", "__lock_", "__log_",
    "__memp_", "__txn_", "__rep_", "__fop_", "__os_",
    # lzma
    "lzma_", "LZMA_",
    # zlib
    "deflate", "inflate", "gz_", "crc32", "adler32",
    # libc / compiler runtime (not interesting for RE)
    "__libc_", "__GI_", "__pthread_",
    # C++ exception handling / RTTI
    "__cxa_", "__gxx_",
]

# Decompiler timeout per function (seconds).
DECOMPILE_TIMEOUT = 60

# Progress reporting interval.
PROGRESS_INTERVAL = 200


def is_library_function(func):
    """Check if a function has been labeled as belonging to a known library.

    Returns the library label string if it is a library function, or None
    if it appears to be a Saleae-owned function.
    """
    # Check plate comment for library tags like [OPENSSL] or [LIBUSB,ZLIB]
    comment = func.getComment()
    if comment:
        # Extract tags from comments like "[OPENSSL] some other text"
        # or "[LIBUSB,PYTHON]"
        match = re.match(r'^\[([A-Z_,]+)\]', comment)
        if match:
            tags = match.group(1).split(",")
            for tag in tags:
                if tag.strip() in LIBRARY_LABELS:
                    return tag.strip()

    # Check function name against known library prefixes
    name = func.getName()
    for prefix in LIBRARY_NAME_PREFIXES:
        if name.startswith(prefix):
            return "NAME_PREFIX"

    # Thunks to library functions
    if name.startswith("thunk_"):
        # Check the thunked function
        thunked = func.getThunkedFunction(False)
        if thunked is not None:
            return is_library_function(thunked)

    return None


def detect_class_name(func):
    """Try to detect a C++ class name from the function's demangled name or
    namespace.

    Returns (class_name, method_name) or (None, None).
    """
    # Check Ghidra namespace (set by demangler or user)
    ns = func.getParentNamespace()
    if ns is not None:
        ns_name = ns.getName()
        # Skip the global namespace
        if ns_name != "Global" and ns_name != currentProgram.getName():
            # Could be nested namespaces like Saleae::UsbDevice
            full_ns = ns_name
            parent = ns.getParentNamespace()
            parts = [ns_name]
            while parent is not None and parent.getName() != "Global":
                parts.insert(0, parent.getName())
                parent = parent.getParentNamespace()
            full_ns = "::".join(parts)
            return (full_ns, func.getName())

    # Check the function name itself for C++ patterns
    name = func.getName()

    # Demangled names might look like "ClassName::methodName"
    if "::" in name:
        parts = name.rsplit("::", 1)
        if len(parts) == 2 and parts[0] and parts[1]:
            return (parts[0], parts[1])

    return (None, None)


def get_callers_and_callees(func, ref_mgr, fm):
    """Get lists of (address, name) for callers and callees of a function."""
    callers = []
    callees = []

    # Find callers: use Ghidra's built-in API when available
    try:
        calling = func.getCallingFunctions(monitor)
        for caller in calling:
            callers.append((caller.getEntryPoint().toString(), caller.getName()))
    except Exception:
        # Fallback: manual reference iteration
        entry = func.getEntryPoint()
        refs_to = ref_mgr.getReferencesTo(entry)
        for ref in refs_to:
            if ref.getReferenceType().isCall():
                from_addr = ref.getFromAddress()
                caller = fm.getFunctionContaining(from_addr)
                if caller is not None:
                    callers.append((caller.getEntryPoint().toString(), caller.getName()))

    # Find callees: use Ghidra's built-in API which is faster than
    # manually iterating every address in the function body
    try:
        called = func.getCalledFunctions(monitor)
        for callee in called:
            callees.append((callee.getEntryPoint().toString(), callee.getName()))
    except Exception:
        # Fallback: manual iteration if getCalledFunctions is unavailable
        body = func.getBody()
        addr_set_iter = body.getAddresses(True)
        while addr_set_iter.hasNext():
            addr = addr_set_iter.next()
            refs_from = ref_mgr.getReferencesFrom(addr)
            for ref in refs_from:
                if ref.getReferenceType().isCall():
                    target = ref.getToAddress()
                    callee = fm.getFunctionAt(target)
                    if callee is not None:
                        pair = (callee.getEntryPoint().toString(), callee.getName())
                        if pair not in callees:
                            callees.append(pair)

    return callers, callees


def safe_filename(name):
    """Convert a function/class name to a safe filename component."""
    # Replace characters that are problematic in filenames
    safe = re.sub(r'[<>:"/\\|?*\s]', '_', name)
    # Collapse multiple underscores
    safe = re.sub(r'_+', '_', safe)
    # Trim length
    if len(safe) > 200:
        safe = safe[:200]
    return safe


def run():
    println("[EXPORT] Starting decompiled C export for: %s" % currentProgram.getName())
    start_time = time.time()

    # Parse arguments
    args = getScriptArgs()
    if args and len(args) > 0 and args[0]:
        output_dir = args[0]
    else:
        output_dir = "re/ghidra/decompiled"
    println("[EXPORT] Output directory: %s" % output_dir)

    # Create output directories
    addr_dir = os.path.join(output_dir, "by-address")
    class_dir = os.path.join(output_dir, "by-class")
    for d in [output_dir, addr_dir, class_dir]:
        if not os.path.isdir(d):
            os.makedirs(d)

    # Initialize the decompiler
    println("[EXPORT] Initializing decompiler...")
    decomp = DecompInterface()
    opts = DecompileOptions()
    decomp.setOptions(opts)
    decomp.openProgram(currentProgram)

    fm = currentProgram.getFunctionManager()
    ref_mgr = currentProgram.getReferenceManager()

    # First pass: identify which functions to export
    println("[EXPORT] Phase 1: Identifying non-library functions...")
    all_funcs = []
    skipped_lib = 0
    skipped_thunk = 0
    skipped_tiny = 0
    total = 0

    for func in fm.getFunctions(True):
        if monitor.isCancelled():
            println("[EXPORT] Cancelled.")
            decomp.dispose()
            return
        total += 1

        # Skip external/thunk-only functions with no real body
        if func.isExternal():
            skipped_thunk += 1
            continue

        # Skip library-labeled functions
        lib = is_library_function(func)
        if lib is not None:
            skipped_lib += 1
            continue

        # Skip tiny functions (< 4 bytes) -- likely data or stubs
        body_size = func.getBody().getNumAddresses()
        if body_size < 4:
            skipped_tiny += 1
            continue

        all_funcs.append(func)

    println("[EXPORT] Total functions:     %d" % total)
    println("[EXPORT] Skipped (library):   %d" % skipped_lib)
    println("[EXPORT] Skipped (external):  %d" % skipped_thunk)
    println("[EXPORT] Skipped (tiny):      %d" % skipped_tiny)
    println("[EXPORT] To export:           %d" % len(all_funcs))

    # Second pass: decompile and export
    println("[EXPORT] Phase 2: Decompiling and exporting...")
    index_entries = []       # (addr, name, size, filename, class_name)
    xref_entries = []        # (caller_addr, caller_name, callee_addr, callee_name)
    class_functions = defaultdict(list)  # class_name -> [(method_name, filename)]
    decompile_failures = []  # (addr, name, error)
    exported = 0

    for i, func in enumerate(all_funcs):
        if monitor.isCancelled():
            println("[EXPORT] Cancelled at function %d/%d." % (i, len(all_funcs)))
            break

        # Progress reporting
        if (i + 1) % PROGRESS_INTERVAL == 0 or i == 0:
            elapsed = time.time() - start_time
            pct = 100.0 * (i + 1) / len(all_funcs)
            rate = (i + 1) / elapsed if elapsed > 0 else 0
            eta = (len(all_funcs) - i - 1) / rate if rate > 0 else 0
            println("[EXPORT] Progress: %d/%d (%.1f%%) - %.0f funcs/sec - ETA %.0fs" % (
                i + 1, len(all_funcs), pct, rate, eta,
            ))

        name = func.getName()
        addr_str = func.getEntryPoint().toString()
        body_size = func.getBody().getNumAddresses()

        # Determine filename
        filename = "%s.c" % safe_filename(name)

        # Decompile
        try:
            result = decomp.decompileFunction(func, DECOMPILE_TIMEOUT, monitor)

            if result is None or not result.decompileCompleted():
                err_msg = "decompilation did not complete"
                if result is not None:
                    err_msg = result.getErrorMessage() or err_msg
                decompile_failures.append((addr_str, name, err_msg))
                continue

            c_code = result.getDecompiledFunction()
            if c_code is None:
                decompile_failures.append((addr_str, name, "no decompiled output"))
                continue

            c_text = c_code.getC()
            if c_text is None or len(c_text.strip()) == 0:
                decompile_failures.append((addr_str, name, "empty decompiled output"))
                continue

        except Exception as e:
            decompile_failures.append((addr_str, name, str(e)))
            continue

        # Build cross-references
        callers, callees = get_callers_and_callees(func, ref_mgr, fm)
        for caller_addr, caller_name in callers:
            xref_entries.append((caller_addr, caller_name, addr_str, name))

        # Build header comment with metadata
        header_lines = []
        header_lines.append("/* ======================================================")
        header_lines.append(" * Function: %s" % name)
        header_lines.append(" * Address:  %s" % addr_str)
        header_lines.append(" * Size:     %d bytes" % body_size)
        header_lines.append(" *")
        if callers:
            header_lines.append(" * Called by:")
            # Deduplicate callers
            seen = set()
            for ca, cn in callers:
                if ca not in seen:
                    header_lines.append(" *   %s  (%s)" % (cn, ca))
                    seen.add(ca)
        else:
            header_lines.append(" * Called by: (none found)")
        header_lines.append(" *")
        if callees:
            header_lines.append(" * Calls:")
            for ca, cn in callees:
                header_lines.append(" *   %s  (%s)" % (cn, ca))
        else:
            header_lines.append(" * Calls: (none found)")
        header_lines.append(" * ======================================================")
        header_lines.append(" */")
        header_lines.append("")

        full_text = "\n".join(header_lines) + c_text

        # Write the file
        filepath = os.path.join(addr_dir, filename)
        # Handle filename collisions (unlikely but possible with mangled names)
        if os.path.exists(filepath):
            base, ext = os.path.splitext(filename)
            filename = "%s_%s%s" % (base, addr_str.replace(":", "_"), ext)
            filepath = os.path.join(addr_dir, filename)

        try:
            with open(filepath, "w") as f:
                f.write(full_text)
        except Exception as e:
            decompile_failures.append((addr_str, name, "write failed: %s" % str(e)))
            continue

        exported += 1

        # Detect class grouping
        class_name, method_name = detect_class_name(func)
        if class_name:
            class_functions[class_name].append((method_name, filename))

        index_entries.append((addr_str, name, body_size, filename,
                              class_name if class_name else ""))

    println("[EXPORT] Phase 2 complete: %d exported, %d failed." % (
        exported, len(decompile_failures),
    ))

    # Phase 3: Write index file
    println("[EXPORT] Phase 3: Writing index...")
    index_path = os.path.join(output_dir, "index.txt")
    try:
        with open(index_path, "w") as f:
            f.write("# Decompiled Function Index\n")
            f.write("# Generated: %s\n" % time.strftime("%Y-%m-%d %H:%M:%S"))
            f.write("# Binary: %s\n" % currentProgram.getName())
            f.write("# Total exported: %d\n" % exported)
            f.write("# Decompile failures: %d\n" % len(decompile_failures))
            f.write("#\n")
            f.write("# Format: address | name | size | file | class\n")
            f.write("#\n")
            # Sort by address for easy browsing
            index_entries.sort(key=lambda x: x[0])
            for addr, name, size, filename, cls in index_entries:
                f.write("%s | %s | %d | %s | %s\n" % (
                    addr, name, size, filename, cls,
                ))
        println("[EXPORT] Index written: %s (%d entries)" % (index_path, len(index_entries)))
    except Exception as e:
        printerr("[EXPORT] Failed to write index: %s" % str(e))

    # Phase 4: Write cross-reference file
    println("[EXPORT] Phase 4: Writing cross-references...")
    xref_path = os.path.join(output_dir, "xrefs.txt")
    try:
        with open(xref_path, "w") as f:
            f.write("# Cross-Reference Graph\n")
            f.write("# Generated: %s\n" % time.strftime("%Y-%m-%d %H:%M:%S"))
            f.write("# Binary: %s\n" % currentProgram.getName())
            f.write("#\n")
            f.write("# Format: caller_addr | caller_name | callee_addr | callee_name\n")
            f.write("# Read as: caller -> callee (caller calls callee)\n")
            f.write("#\n")
            # Deduplicate and sort
            unique_xrefs = sorted(set(xref_entries))
            for caller_addr, caller_name, callee_addr, callee_name in unique_xrefs:
                f.write("%s | %s | %s | %s\n" % (
                    caller_addr, caller_name, callee_addr, callee_name,
                ))
        println("[EXPORT] Xrefs written: %s (%d edges)" % (xref_path, len(unique_xrefs)))
    except Exception as e:
        printerr("[EXPORT] Failed to write xrefs: %s" % str(e))

    # Phase 5: Create class directory structure with symlinks
    println("[EXPORT] Phase 5: Creating class groupings...")
    class_count = 0
    for class_name, methods in sorted(class_functions.items()):
        safe_class = safe_filename(class_name)
        cls_dir = os.path.join(class_dir, safe_class)
        if not os.path.isdir(cls_dir):
            os.makedirs(cls_dir)
        class_count += 1
        for method_name, filename in methods:
            safe_method = safe_filename(method_name)
            link_name = "%s.c" % safe_method
            link_path = os.path.join(cls_dir, link_name)
            # Handle collisions
            if os.path.exists(link_path):
                link_name = "%s_%d.c" % (safe_method, hash(filename) % 10000)
                link_path = os.path.join(cls_dir, link_name)
            # Create relative symlink to by-address/
            target = os.path.join("..", "by-address", filename)
            try:
                os.symlink(target, link_path)
            except Exception:
                # Fallback: copy the filename reference into a text file
                try:
                    with open(link_path + ".ref", "w") as f:
                        f.write("-> by-address/%s\n" % filename)
                except:
                    pass
    println("[EXPORT] Created %d class directories with %d total method links." % (
        class_count,
        sum(len(m) for m in class_functions.values()),
    ))

    # Phase 6: Write class index
    class_index_path = os.path.join(output_dir, "classes.txt")
    try:
        with open(class_index_path, "w") as f:
            f.write("# Detected Class/Namespace Groupings\n")
            f.write("# Generated: %s\n" % time.strftime("%Y-%m-%d %H:%M:%S"))
            f.write("#\n")
            for class_name in sorted(class_functions.keys()):
                methods = class_functions[class_name]
                f.write("\n[%s] (%d methods)\n" % (class_name, len(methods)))
                for method_name, filename in sorted(methods):
                    f.write("  %s -> %s\n" % (method_name, filename))
        println("[EXPORT] Class index written: %s" % class_index_path)
    except Exception as e:
        printerr("[EXPORT] Failed to write class index: %s" % str(e))

    # Phase 7: Write failure log
    if decompile_failures:
        fail_path = os.path.join(output_dir, "failures.txt")
        try:
            with open(fail_path, "w") as f:
                f.write("# Decompilation Failures\n")
                f.write("# Generated: %s\n" % time.strftime("%Y-%m-%d %H:%M:%S"))
                f.write("# Total failures: %d\n" % len(decompile_failures))
                f.write("#\n")
                f.write("# Format: address | name | error\n")
                f.write("#\n")
                for addr, name, err in sorted(decompile_failures):
                    f.write("%s | %s | %s\n" % (addr, name, err))
            println("[EXPORT] Failure log: %s (%d entries)" % (fail_path, len(decompile_failures)))
        except Exception as e:
            printerr("[EXPORT] Failed to write failure log: %s" % str(e))

    # Cleanup
    decomp.dispose()

    # Summary
    elapsed = time.time() - start_time
    println("")
    println("[EXPORT] ============ Export Complete ============")
    println("[EXPORT] Output directory: %s" % output_dir)
    println("[EXPORT] Functions exported:   %d" % exported)
    println("[EXPORT] Decompile failures:   %d" % len(decompile_failures))
    println("[EXPORT] Classes detected:     %d" % class_count)
    println("[EXPORT] Xref edges:           %d" % len(set(xref_entries)))
    println("[EXPORT] Time elapsed:         %.1f seconds" % elapsed)
    println("[EXPORT] Files:")
    println("[EXPORT]   %s" % index_path)
    println("[EXPORT]   %s" % xref_path)
    println("[EXPORT]   %s" % class_index_path)
    println("[EXPORT]   %s/  (%d files)" % (addr_dir, exported))
    if class_count > 0:
        println("[EXPORT]   %s/  (%d directories)" % (class_dir, class_count))
    if decompile_failures:
        println("[EXPORT]   %s" % os.path.join(output_dir, "failures.txt"))
    println("[EXPORT] =============================================")


# Ghidra Python scripts execute at module level
run()
