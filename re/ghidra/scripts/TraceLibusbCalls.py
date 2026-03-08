# TraceLibusbCalls.py -- Ghidra script to trace all libusb call sites in
# libgraph_server_shared.so and map the USB protocol boundaries.
#
# Since libusb is statically linked AND its symbols are exported in the
# dynamic symbol table, we can locate the actual libusb functions directly
# by symbol name. For each key function (control_transfer, bulk_transfer,
# submit_transfer, etc.) we find all callers (XREF-to) and extract what
# we can about the parameters being passed.
#
# Output: re/ghidra/reports/libusb-callsites.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceLibusbCalls.py
#
# @author  saleae_reverse project
# @category USB
# @keybinding
# @menupath
# @toolbar

from ghidra.program.model.symbol import SymbolType
from ghidra.program.model.block import BasicBlockModel
from ghidra.program.model.listing import CodeUnit
from ghidra.program.util import DefinedDataIterator
from ghidra.util.task import ConsoleTaskMonitor

import os
import time

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Key libusb functions we want to trace callers of.
# These are the protocol boundary functions -- every call to one of these
# represents a USB transaction.
LIBUSB_TRANSFER_FUNCTIONS = [
    "libusb_control_transfer",
    "libusb_bulk_transfer",
    "libusb_interrupt_transfer",
    "libusb_submit_transfer",
    "libusb_cancel_transfer",
]

# Additional libusb functions of interest for device lifecycle
LIBUSB_LIFECYCLE_FUNCTIONS = [
    "libusb_open",
    "libusb_open_device_with_vid_pid",
    "libusb_close",
    "libusb_claim_interface",
    "libusb_release_interface",
    "libusb_set_configuration",
    "libusb_set_interface_alt_setting",
    "libusb_reset_device",
    "libusb_clear_halt",
    "libusb_get_device_descriptor",
    "libusb_get_config_descriptor",
    "libusb_get_device_speed",
    "libusb_get_string_descriptor_ascii",
    "libusb_alloc_transfer",
    "libusb_free_transfer",
]

# Saleae wrapper functions we know about from string analysis.
# These are the C++ methods that wrap libusb calls.
SALEAE_USB_WRAPPERS = [
    "LinuxUsbDevice::SendControlTransfer",
    "LinuxUsbDevice::BulkRead",
    "LinuxUsbDevice::BulkWrite",
    "LinuxUsbDevice::Read",
    "LinuxUsbDevice::Write",
    "LinuxUsbDevice::ReseedDevice",
    "LinuxUsbDevice::ReadStream_QueueBuffersThread",
]

# Report output path (relative to user home or absolute)
REPORT_FILENAME = "libusb-callsites.txt"


def get_report_path():
    """Determine the output report path.

    Try to write to the project's re/ghidra/reports/ directory. Fall back
    to the user's home directory if that doesn't exist.
    """
    # Try a few likely locations
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
    # Also try searching in the function manager
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
        if ref.getReferenceType().isCall() or ref.getReferenceType().isUnconditionalFlow():
            caller_addr = ref.getFromAddress()
            caller_func = getFunctionContaining(caller_addr)
            if caller_func is not None and caller_func.getEntryPoint() not in seen:
                seen.add(caller_func.getEntryPoint())
                callers.append((caller_func, caller_addr))
    return callers


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


def try_extract_control_transfer_params(program, call_addr):
    """Try to extract libusb_control_transfer parameters from the
    instructions leading up to the call.

    libusb_control_transfer(dev_handle, bmRequestType, bRequest,
                            wValue, wIndex, data, wLength, timeout)

    On x86-64 SysV ABI:
      rdi = dev_handle
      rsi = bmRequestType
      rdx = bRequest
      rcx = wValue
      r8  = wIndex
      r9  = data
      stack[0] = wLength
      stack[1] = timeout
    """
    params = {}
    listing = program.getListing()

    # Walk backwards from call_addr up to 30 instructions looking for
    # register assignments via mov immediate instructions
    reg_map = {
        "ESI": "bmRequestType", "SIL": "bmRequestType", "RSI": "bmRequestType",
        "EDX": "bRequest", "DL": "bRequest", "RDX": "bRequest",
        "ECX": "wValue", "CL": "wValue", "RCX": "wValue",
        "R8D": "wIndex", "R8": "wIndex", "R8W": "wIndex",
    }

    addr = call_addr
    for _ in range(30):
        inst = listing.getInstructionBefore(addr)
        if inst is None:
            break
        addr = inst.getAddress()
        mnemonic = inst.getMnemonicString()

        if mnemonic.startswith("MOV") or mnemonic.startswith("LEA"):
            if inst.getNumOperands() >= 2:
                op0 = inst.getDefaultOperandRepresentation(0)
                op1 = inst.getDefaultOperandRepresentation(1)
                for reg, param_name in reg_map.items():
                    if op0.upper() == reg:
                        # Try to parse immediate value
                        try:
                            if op1.startswith("0x") or op1.startswith("-0x"):
                                params[param_name] = op1
                            elif op1.isdigit():
                                params[param_name] = "0x{:x}".format(int(op1))
                        except (ValueError, AttributeError):
                            pass
                        break

        # Stop if we hit a call or branch (parameter setup is over)
        if mnemonic.startswith("CALL") or mnemonic.startswith("J"):
            break

    return params


def try_extract_bulk_transfer_params(program, call_addr):
    """Try to extract libusb_bulk_transfer parameters.

    libusb_bulk_transfer(dev_handle, endpoint, data, length,
                         transferred, timeout)

    On x86-64 SysV ABI:
      rdi = dev_handle
      rsi = endpoint (uint8_t)
      rdx = data
      rcx = length
      r8  = transferred (pointer)
      r9  = timeout
    """
    params = {}
    listing = program.getListing()

    reg_map = {
        "ESI": "endpoint", "SIL": "endpoint", "RSI": "endpoint",
    }

    addr = call_addr
    for _ in range(20):
        inst = listing.getInstructionBefore(addr)
        if inst is None:
            break
        addr = inst.getAddress()
        mnemonic = inst.getMnemonicString()

        if mnemonic.startswith("MOV"):
            if inst.getNumOperands() >= 2:
                op0 = inst.getDefaultOperandRepresentation(0)
                op1 = inst.getDefaultOperandRepresentation(1)
                for reg, param_name in reg_map.items():
                    if op0.upper() == reg:
                        try:
                            if op1.startswith("0x") or op1.startswith("-0x"):
                                val = int(op1, 16)
                                direction = "IN" if (val & 0x80) else "OUT"
                                ep_num = val & 0x7F
                                params["endpoint"] = "0x{:02x} (EP{} {})".format(
                                    val, ep_num, direction
                                )
                            elif op1.isdigit():
                                val = int(op1)
                                direction = "IN" if (val & 0x80) else "OUT"
                                ep_num = val & 0x7F
                                params["endpoint"] = "0x{:02x} (EP{} {})".format(
                                    val, ep_num, direction
                                )
                        except (ValueError, AttributeError):
                            pass
                        break

        if mnemonic.startswith("CALL") or mnemonic.startswith("J"):
            break

    return params


def classify_caller(func_name, string_refs):
    """Attempt to classify what protocol operation a caller performs,
    based on its name and string references."""
    categories = []

    # Name-based classification
    name_lower = func_name.lower()
    if "firmware" in name_lower or "download" in name_lower:
        categories.append("FIRMWARE")
    if "bitstream" in name_lower or "fpga" in name_lower:
        categories.append("FPGA")
    if "eeprom" in name_lower or "i2c" in name_lower:
        categories.append("EEPROM/I2C")
    if "capture" in name_lower or "sampling" in name_lower:
        categories.append("CAPTURE")
    if "read" in name_lower and "stream" in name_lower:
        categories.append("DATA_STREAM")
    if "register" in name_lower:
        categories.append("REGISTER_IO")
    if "reseed" in name_lower:
        categories.append("RESEED")
    if "calibrat" in name_lower:
        categories.append("CALIBRATION")
    if "crypto" in name_lower:
        categories.append("CRYPTO")
    if "led" in name_lower:
        categories.append("LED_CONTROL")

    # String-reference-based classification
    for s in string_refs:
        sl = s.lower()
        if "firmware" in sl and "FIRMWARE" not in categories:
            categories.append("FIRMWARE")
        if "bitstream" in sl and "FPGA" not in categories:
            categories.append("FPGA")
        if "eeprom" in sl and "EEPROM/I2C" not in categories:
            categories.append("EEPROM/I2C")
        if "capture" in sl and "CAPTURE" not in categories:
            categories.append("CAPTURE")
        if "reseed" in sl and "RESEED" not in categories:
            categories.append("RESEED")

    if not categories:
        categories.append("UNKNOWN")

    return categories


# ---------------------------------------------------------------------------
# Main script
# ---------------------------------------------------------------------------

def main():
    monitor = ConsoleTaskMonitor()
    program = currentProgram
    report_path = get_report_path()

    lines = []
    lines.append("=" * 78)
    lines.append("LIBUSB CALL SITE ANALYSIS")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: Direct libusb transfer function callers
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: CALLERS OF LIBUSB TRANSFER FUNCTIONS")
    lines.append("=" * 78)
    lines.append("")

    all_callers = {}  # func_name -> set of called libusb functions

    for libusb_name in LIBUSB_TRANSFER_FUNCTIONS:
        func = find_function_by_name(program, libusb_name)
        if func is None:
            lines.append("[!] {} -- NOT FOUND".format(libusb_name))
            lines.append("")
            continue

        callers = get_callers(program, func)
        lines.append("--- {} ({}) ---".format(libusb_name, func.getEntryPoint()))
        lines.append("    {} direct caller(s)".format(len(callers)))
        lines.append("")

        for caller_func, call_addr in callers:
            caller_name = caller_func.getName()

            # Track for summary
            if caller_name not in all_callers:
                all_callers[caller_name] = set()
            all_callers[caller_name].add(libusb_name)

            # Get string refs for classification
            str_refs = get_string_refs_in_function(program, caller_func)
            categories = classify_caller(caller_name, str_refs)

            lines.append("    Caller: {} @ {}".format(caller_name,
                                                       caller_func.getEntryPoint()))
            lines.append("    Call at: {}".format(call_addr))
            lines.append("    Category: {}".format(", ".join(categories)))

            # Extract parameters
            if "control_transfer" in libusb_name:
                params = try_extract_control_transfer_params(program, call_addr)
                if params:
                    lines.append("    Control Transfer Params:")
                    for k, v in sorted(params.items()):
                        lines.append("      {}: {}".format(k, v))
                    # Decode bmRequestType if available
                    if "bmRequestType" in params:
                        try:
                            brt = int(params["bmRequestType"], 16)
                            direction = "Device-to-Host" if (brt & 0x80) else "Host-to-Device"
                            req_type = (brt >> 5) & 0x03
                            type_names = {0: "Standard", 1: "Class", 2: "Vendor", 3: "Reserved"}
                            recipient = brt & 0x1F
                            recip_names = {0: "Device", 1: "Interface", 2: "Endpoint", 3: "Other"}
                            lines.append("      -> Direction: {}".format(direction))
                            lines.append("      -> Type: {} ({})".format(
                                type_names.get(req_type, "?"), req_type))
                            lines.append("      -> Recipient: {} ({})".format(
                                recip_names.get(recipient, "?"), recipient))
                        except (ValueError, TypeError):
                            pass

            elif "bulk_transfer" in libusb_name:
                params = try_extract_bulk_transfer_params(program, call_addr)
                if params:
                    lines.append("    Bulk Transfer Params:")
                    for k, v in sorted(params.items()):
                        lines.append("      {}: {}".format(k, v))

            # Show relevant strings found in the function
            relevant = [s for s in str_refs
                        if any(kw in s.lower()
                               for kw in ["usb", "bulk", "control", "endpoint",
                                          "firmware", "bitstream", "fpga",
                                          "eeprom", "register", "transfer",
                                          "capture", "reseed", "error"])]
            if relevant:
                lines.append("    Relevant strings:")
                for s in relevant[:10]:  # Limit to avoid noise
                    lines.append("      \"{}\"".format(s[:100]))

            lines.append("")

    # ------------------------------------------------------------------
    # Part 2: Callers of libusb lifecycle functions
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 2: CALLERS OF LIBUSB LIFECYCLE FUNCTIONS")
    lines.append("=" * 78)
    lines.append("")

    for libusb_name in LIBUSB_LIFECYCLE_FUNCTIONS:
        func = find_function_by_name(program, libusb_name)
        if func is None:
            lines.append("[!] {} -- NOT FOUND".format(libusb_name))
            continue

        callers = get_callers(program, func)
        lines.append("--- {} ({}) ---".format(libusb_name, func.getEntryPoint()))
        lines.append("    {} caller(s): {}".format(
            len(callers),
            ", ".join("{}@{}".format(c[0].getName(), c[1]) for c in callers[:20])
        ))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 3: Summary -- all unique functions that touch libusb
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 3: SUMMARY -- ALL FUNCTIONS CALLING LIBUSB TRANSFER API")
    lines.append("=" * 78)
    lines.append("")

    for caller_name in sorted(all_callers.keys()):
        called = sorted(all_callers[caller_name])
        lines.append("  {} -> {}".format(caller_name, ", ".join(called)))

    lines.append("")
    lines.append("Total unique callers of transfer functions: {}".format(
        len(all_callers)))

    # ------------------------------------------------------------------
    # Part 4: Identify Saleae USB wrapper functions
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 4: SALEAE USB WRAPPER FUNCTIONS")
    lines.append("(Functions matching known wrapper names from string analysis)")
    lines.append("=" * 78)
    lines.append("")

    for wrapper in SALEAE_USB_WRAPPERS:
        # Search by substring since names might be mangled
        short_name = wrapper.split("::")[-1]
        matches = find_functions_containing(program, short_name)
        if matches:
            for m in matches:
                callers = get_callers(program, m)
                lines.append("  {} @ {}".format(m.getName(), m.getEntryPoint()))
                lines.append("    {} caller(s)".format(len(callers)))
                for c, addr in callers[:20]:
                    cats = classify_caller(c.getName(), [])
                    lines.append("      <- {} @ {} [{}]".format(
                        c.getName(), c.getEntryPoint(), ", ".join(cats)))
                lines.append("")
        else:
            lines.append("  [!] No match for: {}".format(wrapper))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 5: TransferQueue analysis (async bulk transfer management)
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 5: TRANSFERQUEUE (ASYNC BULK TRANSFER MANAGEMENT)")
    lines.append("=" * 78)
    lines.append("")

    tq_matches = find_functions_containing(program, "TransferQueue")
    if tq_matches:
        for m in tq_matches:
            lines.append("  {} @ {}".format(m.getName(), m.getEntryPoint()))
            callers = get_callers(program, m)
            for c, addr in callers[:10]:
                lines.append("    <- {} @ {}".format(c.getName(), c.getEntryPoint()))
            lines.append("")
    else:
        lines.append("  [!] No TransferQueue functions found")
        lines.append("")

    # ------------------------------------------------------------------
    # Part 6: UsbControlSetupPacket struct references
    # ------------------------------------------------------------------
    lines.append("")
    lines.append("=" * 78)
    lines.append("PART 6: UsbControlSetupPacket REFERENCES")
    lines.append("(Identifies all code paths that construct USB control packets)")
    lines.append("=" * 78)
    lines.append("")

    # Search for the string "UsbControlSetupPacket" in defined strings
    # and then find who references those strings
    string_matches = []
    for data in DefinedDataIterator.definedStrings(program):
        val = str(data.getValue())
        if "UsbControlSetupPacket" in val or "SendControlTransfer" in val:
            string_matches.append((data.getAddress(), val))

    if string_matches:
        for addr, val in string_matches:
            lines.append("  String: \"{}\" @ {}".format(val[:80], addr))
            refs = getReferencesTo(addr)
            for ref in refs:
                func = getFunctionContaining(ref.getFromAddress())
                if func:
                    lines.append("    Referenced by: {} @ {}".format(
                        func.getName(), func.getEntryPoint()))
            lines.append("")
    else:
        lines.append("  [!] No UsbControlSetupPacket string references found")
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
        println("Printing report to console instead:")
        println(report)

    # Also print a summary to the console
    println("")
    println("=== SUMMARY ===")
    println("Total libusb transfer function callers: {}".format(len(all_callers)))
    println("Report: {}".format(report_path))


# Entry point
if __name__ == "__main__":
    main()
else:
    main()
