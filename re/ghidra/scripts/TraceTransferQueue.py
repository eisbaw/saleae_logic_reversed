# TraceTransferQueue.py -- Ghidra script to trace the TransferQueue class that
# manages async USB bulk transfers for high-speed capture data streaming in
# libgraph_server_shared.so.
#
# TransferQueue is an anonymous-namespace class in LinuxUsbDevice.cpp that
# wraps libusb's async transfer API (libusb_submit_transfer / cancel_transfer)
# to maintain a pool of in-flight bulk transfers on EP2 IN (0x82).
#
# This script:
#   1. Locates the TransferQueue constructor and on_libusb_transfer_complete
#   2. Extracts constructor parameters and internal constants
#   3. Maps callers of submit/cancel to understand queue lifecycle
#   4. Traces the MemoryPool integration (DMA buffer allocation)
#   5. Identifies error handling paths and queue capacity reduction logic
#   6. Extracts transfer size computation constants from ComputeTransferSize
#
# Output: re/ghidra/reports/transfer-queue-trace.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceTransferQueue.py
#
# @author  saleae_reverse project
# @category USB
# @keybinding
# @menupath
# @toolbar

from ghidra.program.model.symbol import SymbolType
from ghidra.program.model.listing import CodeUnit
from ghidra.program.util import DefinedDataIterator
from ghidra.util.task import ConsoleTaskMonitor

import os
import time


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Functions directly related to TransferQueue and async transfer management
TRANSFER_QUEUE_FUNCTIONS = [
    # TransferQueue class
    ("TransferQueue", "TransferQueue constructor and methods"),

    # The async transfer completion callback
    ("on_libusb_transfer_complete", "Async transfer completion callback"),

    # The thread that drives the queue
    ("ReadStream_QueueBuffersThread",
     "Thread that submits transfers and processes completions"),

    # BulkReadStream lifecycle
    ("BulkReadStream_Read", "Bulk read stream read entry"),
    ("BulkReadStream_Request", "Bulk read stream request entry"),
    ("StopBulkReadStream", "Stop the bulk read stream"),

    # Transfer size computation (feeds into TransferQueue construction)
    ("ComputeTransferSize", "Compute USB bulk transfer buffer size"),
    ("AdjustTransferSize", "Adjust the computed transfer size"),

    # Memory pool (DMA buffer allocation)
    ("MemoryPool", "Memory pool for USB transfer buffers"),
]

# Libusb functions called by the TransferQueue
LIBUSB_ASYNC_FUNCTIONS = [
    "libusb_alloc_transfer",
    "libusb_free_transfer",
    "libusb_submit_transfer",
    "libusb_cancel_transfer",
    "libusb_handle_events",
    "libusb_handle_events_timeout",
    "libusb_handle_events_timeout_completed",
    "libusb_handle_events_completed",
    "libusb_handle_events_locked",
    "libusb_dev_mem_alloc",
    "libusb_dev_mem_free",
    "libusb_alloc_streams",
    "libusb_free_streams",
    "libusb_transfer_set_stream_id",
    "libusb_transfer_get_stream_id",
]

# String patterns revealing TransferQueue behavior
QUEUE_STRING_PATTERNS = [
    "TransferQueue",
    "transfer_queue",
    "queue_capacity",
    "queue_size",
    "Failed to submit USB transfer",
    "reducing queue capacity",
    "Done with usb stream",
    "successful transfers",
    "Freeing transfer",
    "total_complete",
    "Starting bulk read thread",
    "transfer_size=",
    "timeout=",
    "BulkReadThread: Waiting",
    "BulkReadThread: Timeout",
    "Cancelling transfers",
    "Cancelling {}",
    "transfer_count",
    "USB Transfer Error",
    "USB ReadStream Error",
    "ReadStream Error",
    "ReadQueueError",
    "TransferSubmissionFailure",
    "transfer.mBuffer",
    "transfer.mComplete",
    "mComplete",
    "detected endpoint stall",
    "USB Stall",
    "Device handle closed while transfer",
    "cancellation for an in-flight",
    "cancellation hasn't even been scheduled",
    "libusb_handle_events failed",
    "cancelling transfer and retrying",
    "memory_pool",
    "MemoryPool",
    "dev_mem",
    "alloc_streams",
    "free_streams",
    "LIBUSB_TRANSFER_",
    "LIBUSB_SUCCESS",
    "ideal transfer size",
    "Transfer size is",
    "capping transfer size",
    "SALEAE_USB_TRANSFER_LIMIT",
    "mReadStreamId",
    "stopping stream",
    "MsoInterruptReadStreamError",
    "MsoBulkReadStreamError",
]

REPORT_FILENAME = "transfer-queue-trace.txt"


def get_report_path():
    """Determine the output report path."""
    candidates = [
        os.path.expanduser("~/nvidia/saleae_reverse/re/ghidra/reports"),
        os.path.expanduser("~/saleae_reverse/re/ghidra/reports"),
        os.path.expanduser("~"),
    ]
    for d in candidates:
        if os.path.isdir(d):
            return os.path.join(d, REPORT_FILENAME)
    return os.path.join(os.path.expanduser("~"), REPORT_FILENAME)


def find_function_by_name(program, name):
    """Find a function by its exact symbol name."""
    sym_table = program.getSymbolTable()
    symbols = sym_table.getGlobalSymbols(name)
    for sym in symbols:
        if sym.getSymbolType() == SymbolType.FUNCTION:
            return getFunctionAt(sym.getAddress())
    fm = program.getFunctionManager()
    funcs = fm.getFunctions(True)
    for func in funcs:
        if func.getName() == name:
            return func
    return None


def find_functions_containing(program, substring):
    """Find functions whose name contains the given substring."""
    results = []
    fm = program.getFunctionManager()
    funcs = fm.getFunctions(True)
    for func in funcs:
        if substring in func.getName():
            results.append(func)
    return results


def get_callers(program, func):
    """Get all functions that call the given function (via XREF)."""
    callers = []
    if func is None:
        return callers
    entry = func.getEntryPoint()
    refs = getReferencesTo(entry)
    seen = set()
    for ref in refs:
        if ref.getReferenceType().isCall() or \
           ref.getReferenceType().isUnconditionalFlow():
            caller_addr = ref.getFromAddress()
            caller_func = getFunctionContaining(caller_addr)
            if caller_func is not None and \
               caller_func.getEntryPoint() not in seen:
                seen.add(caller_func.getEntryPoint())
                callers.append((caller_func, caller_addr))
    return callers


def get_callees(program, func):
    """Get all functions called by the given function."""
    callees = []
    if func is None:
        return callees
    body = func.getBody()
    listing = program.getListing()
    inst_iter = listing.getInstructions(body, True)
    seen = set()
    while inst_iter.hasNext():
        inst = inst_iter.next()
        mnemonic = inst.getMnemonicString()
        if mnemonic.startswith("CALL"):
            for i in range(inst.getNumOperands()):
                refs = inst.getOperandReferences(i)
                for ref in refs:
                    target = ref.getToAddress()
                    callee = getFunctionAt(target)
                    if callee is not None and target not in seen:
                        seen.add(target)
                        callees.append((callee, inst.getAddress()))
    return callees


def get_string_refs_in_function(program, func):
    """Find string references within a function's body."""
    strings_found = []
    if func is None:
        return strings_found
    body = func.getBody()
    listing = program.getListing()
    inst_iter = listing.getInstructions(body, True)
    while inst_iter.hasNext():
        inst = inst_iter.next()
        for i in range(inst.getNumOperands()):
            refs = inst.getOperandReferences(i)
            for ref in refs:
                target = ref.getToAddress()
                data = listing.getDataAt(target)
                if data is not None and data.hasStringValue():
                    strings_found.append(str(data.getValue()))
    return strings_found


def find_strings_matching(program, patterns):
    """Find defined strings matching any of the given patterns."""
    results = []
    for data in DefinedDataIterator.definedStrings(program):
        val = str(data.getValue())
        for pat in patterns:
            if pat.lower() in val.lower():
                results.append((data.getAddress(), val, pat))
                break
    return results


def try_extract_immediate_constants(program, func, max_instructions=300):
    """Extract immediate constants from a function that might be
    buffer sizes, queue depths, timeouts, or transfer sizes."""
    constants = []
    if func is None:
        return constants
    body = func.getBody()
    listing = program.getListing()
    inst_iter = listing.getInstructions(body, True)
    count = 0
    while inst_iter.hasNext() and count < max_instructions:
        inst = inst_iter.next()
        count += 1
        mnemonic = inst.getMnemonicString()
        if mnemonic.startswith("MOV") or mnemonic.startswith("CMP") or \
           mnemonic.startswith("LEA"):
            for i in range(inst.getNumOperands()):
                op = inst.getDefaultOperandRepresentation(i)
                try:
                    if op.startswith("0x"):
                        val = int(op, 16)
                        if 1 <= val <= 0x10000000:
                            constants.append((val, inst.getAddress(),
                                              mnemonic, op))
                    elif op.startswith("-0x"):
                        val = int(op, 16)
                        constants.append((val, inst.getAddress(),
                                          mnemonic, op))
                    elif op.lstrip("-").isdigit():
                        val = int(op)
                        if 1 <= abs(val) <= 0x10000000:
                            constants.append((val, inst.getAddress(),
                                              mnemonic, op))
                except (ValueError, AttributeError):
                    pass
    return constants


def extract_constructor_parameters(program, func):
    """For the TransferQueue constructor, try to find parameter usage
    patterns that reveal parameter meanings.

    TransferQueue(libusb_device_handle&, UsbEndpoint, uint32_t, uint32_t,
                  uint64_t, uint64_t)

    x86-64 SysV ABI:
      rdi = this (implicit)
      rsi = libusb_device_handle& (param 1)
      rdx = UsbEndpoint (param 2, likely a struct/enum)
      rcx = uint32_t (param 3 -- queue_depth or transfer_size)
      r8  = uint32_t (param 4 -- queue_depth or transfer_size)
      r9  = uint64_t (param 5 -- timeout or max_bytes)
      stack = uint64_t (param 6 -- timeout or max_bytes)

    We look at how these registers are stored to struct members to infer
    the field mapping.
    """
    info = {}
    if func is None:
        return info

    body = func.getBody()
    listing = program.getListing()
    inst_iter = listing.getInstructions(body, True)

    # Track register stores to [rbp+offset] or [rdi+offset] patterns
    # This reveals struct member assignments
    member_stores = []
    count = 0
    while inst_iter.hasNext() and count < 100:
        inst = inst_iter.next()
        count += 1
        mnemonic = inst.getMnemonicString()

        if mnemonic.startswith("MOV"):
            if inst.getNumOperands() >= 2:
                op0 = inst.getDefaultOperandRepresentation(0)
                op1 = inst.getDefaultOperandRepresentation(1)
                # Look for stores from parameter registers to memory
                src_regs = ["RSI", "RDX", "ECX", "R8D", "R9"]
                for reg in src_regs:
                    if op1.upper() == reg:
                        member_stores.append((op0, reg, inst.getAddress()))
                        break

    info["member_stores"] = member_stores
    return info


# ---------------------------------------------------------------------------
# Main script
# ---------------------------------------------------------------------------

def main():
    monitor = ConsoleTaskMonitor()
    program = currentProgram
    report_path = get_report_path()

    lines = []
    lines.append("=" * 78)
    lines.append("TRANSFERQUEUE ANALYSIS -- ASYNC USB BULK TRANSFER MANAGEMENT")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")
    lines.append("TransferQueue is an anonymous-namespace class defined in")
    lines.append("LinuxUsbDevice.cpp. It manages a pool of pre-allocated")
    lines.append("libusb async transfers for high-throughput USB streaming.")
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: Locate TransferQueue and related functions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: TRANSFERQUEUE AND RELATED FUNCTIONS")
    lines.append("=" * 78)
    lines.append("")

    found_functions = {}

    for search_name, description in TRANSFER_QUEUE_FUNCTIONS:
        matches = find_functions_containing(program, search_name)
        if matches:
            found_functions[search_name] = matches
            for m in matches:
                lines.append("  [+] {} @ {}".format(
                    m.getName()[:120], m.getEntryPoint()))
                lines.append("      Purpose: {}".format(description))
                lines.append("      Size: {} bytes".format(
                    m.getBody().getNumAddresses()))

                # Show callers
                callers = get_callers(program, m)
                if callers:
                    lines.append("      Callers ({}):" .format(len(callers)))
                    for c, addr in callers[:15]:
                        lines.append("        <- {} @ {}".format(
                            c.getName()[:100], c.getEntryPoint()))

                # Show callees (what does this function call?)
                callees = get_callees(program, m)
                if callees:
                    lines.append("      Callees ({}):" .format(len(callees)))
                    for c, addr in callees[:15]:
                        lines.append("        -> {} @ {}".format(
                            c.getName()[:100], c.getEntryPoint()))

                # Show string references
                str_refs = get_string_refs_in_function(program, m)
                if str_refs:
                    lines.append("      Strings referenced:")
                    for s in str_refs[:20]:
                        lines.append("        \"{}\"".format(s[:100]))

                lines.append("")
        else:
            lines.append("  [-] {} -- NOT FOUND".format(search_name))
            lines.append("      Expected: {}".format(description))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 2: TransferQueue constructor analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 2: TRANSFERQUEUE CONSTRUCTOR PARAMETER ANALYSIS")
    lines.append("=" * 78)
    lines.append("")
    lines.append("Signature: TransferQueue(libusb_device_handle&, UsbEndpoint,")
    lines.append("           uint32_t, uint32_t, uint64_t, uint64_t)")
    lines.append("")

    tq_constructors = find_functions_containing(program, "TransferQueue")
    for f in tq_constructors:
        if "TransferQueue" in f.getName():
            lines.append("  Constructor: {} @ {}".format(
                f.getName()[:120], f.getEntryPoint()))

            # Extract parameter usage
            param_info = extract_constructor_parameters(program, f)
            if param_info.get("member_stores"):
                lines.append("  Parameter-to-member stores:")
                for dest, src_reg, addr in param_info["member_stores"]:
                    lines.append("    {} <- {} @ {}".format(
                        dest, src_reg, addr))

            # Extract constants
            constants = try_extract_immediate_constants(program, f)
            if constants:
                lines.append("  Constants in constructor:")
                for val, addr, mnemonic, op in constants:
                    lines.append("    {} {} = {} (0x{:x}) @ {}".format(
                        mnemonic, op, val, val, addr))

            lines.append("")

    # ------------------------------------------------------------------
    # Part 3: on_libusb_transfer_complete callback analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 3: on_libusb_transfer_complete CALLBACK ANALYSIS")
    lines.append("=" * 78)
    lines.append("")

    callback_funcs = find_functions_containing(
        program, "on_libusb_transfer_complete")
    for f in callback_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        lines.append("  Size: {} bytes".format(
            f.getBody().getNumAddresses()))

        # What does the callback call?
        callees = get_callees(program, f)
        if callees:
            lines.append("  Callees ({}):" .format(len(callees)))
            for c, addr in callees[:20]:
                lines.append("    -> {} @ {}".format(
                    c.getName()[:100], c.getEntryPoint()))

        # What strings does it reference?
        str_refs = get_string_refs_in_function(program, f)
        if str_refs:
            lines.append("  String references:")
            for s in str_refs[:30]:
                lines.append("    \"{}\"".format(s[:100]))

        # Constants (potential status code checks)
        constants = try_extract_immediate_constants(program, f)
        if constants:
            lines.append("  Constants (potential status codes, sizes):")
            for val, addr, mnemonic, op in constants[:30]:
                lines.append("    {} {} = {} (0x{:x}) @ {}".format(
                    mnemonic, op, val, val, addr))

        lines.append("")

    # ------------------------------------------------------------------
    # Part 4: ReadStream_QueueBuffersThread analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 4: ReadStream_QueueBuffersThread ANALYSIS")
    lines.append("(The main loop that drives the TransferQueue)")
    lines.append("=" * 78)
    lines.append("")

    thread_funcs = find_functions_containing(
        program, "ReadStream_QueueBuffersThread")
    for f in thread_funcs:
        if not f.getName().startswith("_Z"):
            continue  # Skip non-mangled matches (string refs)
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        lines.append("  Size: {} bytes".format(
            f.getBody().getNumAddresses()))

        callees = get_callees(program, f)
        if callees:
            lines.append("  Callees ({}):" .format(len(callees)))
            for c, addr in callees[:30]:
                lines.append("    -> {} @ {}".format(
                    c.getName()[:100], c.getEntryPoint()))

        str_refs = get_string_refs_in_function(program, f)
        if str_refs:
            lines.append("  String references:")
            for s in str_refs[:30]:
                lines.append("    \"{}\"".format(s[:100]))

        constants = try_extract_immediate_constants(program, f, 500)
        if constants:
            lines.append("  Constants:")
            for val, addr, mnemonic, op in constants[:40]:
                lines.append("    {} {} = {} (0x{:x}) @ {}".format(
                    mnemonic, op, val, val, addr))

        lines.append("")

    # Also look at all functions whose name contains
    # "ReadStream_QueueBuffersThread"
    # (some might be lambdas like $_1)
    lambda_funcs = find_functions_containing(program,
                                             "QueueBuffersThread")
    if lambda_funcs:
        lines.append("  Related lambdas/functors:")
        for f in lambda_funcs:
            lines.append("    {} @ {} ({} bytes)".format(
                f.getName()[:120], f.getEntryPoint(),
                f.getBody().getNumAddresses()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 5: libusb async API usage
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 5: LIBUSB ASYNC API CALL SITES")
    lines.append("=" * 78)
    lines.append("")

    for libusb_name in LIBUSB_ASYNC_FUNCTIONS:
        func = find_function_by_name(program, libusb_name)
        if func is None:
            lines.append("  [!] {} -- NOT FOUND".format(libusb_name))
            continue

        callers = get_callers(program, func)
        lines.append("  --- {} @ {} ---".format(
            libusb_name, func.getEntryPoint()))
        lines.append("      {} direct caller(s)".format(len(callers)))
        for c, addr in callers[:15]:
            lines.append("      <- {} @ {} (call at {})".format(
                c.getName()[:80], c.getEntryPoint(), addr))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 6: Transfer size computation
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 6: TRANSFER SIZE COMPUTATION")
    lines.append("(Constants from ComputeTransferSize and AdjustTransferSize)")
    lines.append("=" * 78)
    lines.append("")

    size_funcs = find_functions_containing(program, "ComputeTransferSize")
    size_funcs += find_functions_containing(program, "AdjustTransferSize")
    for f in size_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        lines.append("  Size: {} bytes".format(
            f.getBody().getNumAddresses()))

        constants = try_extract_immediate_constants(program, f, 200)
        if constants:
            lines.append("  Constants (potential transfer sizes, limits):")
            for val, addr, mnemonic, op in constants[:30]:
                # Annotate commonly seen USB sizes
                note = ""
                if val == 512:
                    note = " [USB 2.0 max packet]"
                elif val == 1024:
                    note = " [USB 3.0 max packet]"
                elif val == 16384:
                    note = " [16 x 1024, USB 3.0 burst]"
                elif val % 1024 == 0 and val >= 4096:
                    note = " [{} KB]".format(val // 1024)
                lines.append("    {} {} = {} (0x{:x}) @ {}{}".format(
                    mnemonic, op, val, val, addr, note))

        str_refs = get_string_refs_in_function(program, f)
        if str_refs:
            lines.append("  Strings:")
            for s in str_refs[:10]:
                lines.append("    \"{}\"".format(s[:100]))

        lines.append("")

    # ------------------------------------------------------------------
    # Part 7: MemoryPool analysis
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 7: MEMORYPOOL (DMA BUFFER ALLOCATION)")
    lines.append("=" * 78)
    lines.append("")

    pool_funcs = find_functions_containing(program, "MemoryPool")
    for f in pool_funcs:
        lines.append("  {} @ {}".format(f.getName()[:120],
                                         f.getEntryPoint()))
        lines.append("  Size: {} bytes".format(
            f.getBody().getNumAddresses()))

        constants = try_extract_immediate_constants(program, f, 200)
        if constants:
            lines.append("  Constants (buffer sizes, page sizes):")
            for val, addr, mnemonic, op in constants[:20]:
                note = ""
                if val == 4096:
                    note = " [page size]"
                elif val % 4096 == 0:
                    note = " [{} pages]".format(val // 4096)
                lines.append("    {} {} = {} (0x{:x}) @ {}{}".format(
                    mnemonic, op, val, val, addr, note))

        str_refs = get_string_refs_in_function(program, f)
        if str_refs:
            lines.append("  Strings:")
            for s in str_refs[:10]:
                lines.append("    \"{}\"".format(s[:100]))

        lines.append("")

    # ------------------------------------------------------------------
    # Part 8: Queue behavior strings
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 8: QUEUE BEHAVIOR STRINGS AND THEIR REFERENCES")
    lines.append("=" * 78)
    lines.append("")

    queue_strings = find_strings_matching(program, QUEUE_STRING_PATTERNS)
    for addr, val, pattern in sorted(queue_strings, key=lambda x: x[2]):
        lines.append("  [{:40s}] \"{}\"".format(pattern[:40], val[:100]))
        lines.append("  {:42s} @ {}".format("", addr))
        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("  {:42s} Referenced by: {} @ {}".format(
                    "", func.getName()[:80], func.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 9: Error handling paths
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 9: ERROR HANDLING AND RECOVERY PATHS")
    lines.append("=" * 78)
    lines.append("")

    error_patterns = [
        "LIBUSB_TRANSFER_ERROR",
        "LIBUSB_TRANSFER_STALL",
        "LIBUSB_TRANSFER_NO_DEVICE",
        "LIBUSB_TRANSFER_TIMED_OUT",
        "LIBUSB_TRANSFER_OVERFLOW",
        "LIBUSB_TRANSFER_CANCELLED",
        "TransferSubmissionFailure",
        "ReadQueueError",
        "USB Transfer Error",
        "USB ReadStream Error",
        "USB Stall",
        "detected endpoint stall",
        "Device handle closed while transfer",
    ]

    error_strings = find_strings_matching(program, error_patterns)
    for addr, val, pattern in sorted(error_strings, key=lambda x: x[2]):
        lines.append("  \"{}\" @ {}".format(val[:100], addr))
        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("    In: {} @ {}".format(
                    func.getName()[:80], func.getEntryPoint()))
                # Check for related strings in the same function
                func_strs = get_string_refs_in_function(program, func)
                related = [s for s in func_strs
                           if any(kw in s.lower()
                                  for kw in ["error", "cancel", "stall",
                                             "timeout", "retry", "failed",
                                             "queue", "transfer", "capacity",
                                             "reduce", "stream"])]
                if related:
                    lines.append("    Related strings in function:")
                    for s in related[:8]:
                        lines.append("      \"{}\"".format(s[:80]))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 10: ReadStreamError enum values
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 10: ReadStreamError ENUM AND ERROR CALLBACKS")
    lines.append("=" * 78)
    lines.append("")

    rse_funcs = find_functions_containing(program, "ReadStreamError")
    if rse_funcs:
        lines.append("  Functions referencing ReadStreamError:")
        for f in rse_funcs:
            lines.append("    {} @ {} ({} bytes)".format(
                f.getName()[:120], f.getEntryPoint(),
                f.getBody().getNumAddresses()))
        lines.append("")
    else:
        lines.append("  (ReadStreamError appears in type names but not")
        lines.append("   as standalone function names)")
        lines.append("")

    # Look for the error callback bindings per device type
    error_callback_patterns = [
        "ReadStreamError_impl",
        "MsoInterruptReadStreamError",
        "MsoBulkReadStreamError",
    ]
    ecb_strings = find_strings_matching(program, error_callback_patterns)
    for addr, val, pattern in ecb_strings:
        lines.append("  \"{}\"".format(val[:120]))
        lines.append("    @ {}".format(addr))
    lines.append("")

    # ------------------------------------------------------------------
    # Write report
    # ------------------------------------------------------------------
    report = "\n".join(lines)

    try:
        with open(report_path, "w") as f:
            f.write(report)
        println("Report written to: {}".format(report_path))
    except IOError as e:
        println("ERROR writing report: {}".format(e))
        println("Printing to console instead:")
        println(report)

    # Summary
    println("")
    println("=== SUMMARY ===")
    println("TransferQueue functions found: {}".format(
        sum(len(v) for v in found_functions.values())))
    println("Queue behavior strings found: {}".format(len(queue_strings)))
    println("Error handling strings found: {}".format(len(error_strings)))
    println("Report: {}".format(report_path))


# Entry point
if __name__ == "__main__":
    main()
else:
    main()
