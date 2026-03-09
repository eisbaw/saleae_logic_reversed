# TraceFirmwareUpload.py -- Ghidra script to trace the FX3 firmware upload
# protocol in libgraph_server_shared.so.
#
# Finds the DownloadFirmware function, ParseFirmwareImage, and the control
# transfer path used during firmware upload. Maps the firmware upload state
# machine and traces the USB vendor control transfer parameters.
#
# Output: re/ghidra/reports/firmware-upload-trace.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceFirmwareUpload.py
#
# @author  saleae_reverse project
# @category USB
# @keybinding
# @menupath
# @toolbar

from ghidra.program.model.symbol import SymbolType
from ghidra.program.util import DefinedDataIterator
from ghidra.util.task import ConsoleTaskMonitor

import os
import time

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Key firmware-related functions identified from string analysis
FIRMWARE_FUNCTIONS = [
    "DownloadFirmware",
    "ParseFirmwareImage",
    "GetFirmwareForDeviceType",
    "GetFirmwareVersion",
    "IsFirmwareDownloaded",
    "TestFirmwareResponsivnessAndReseed",
    "WriteFirmware",
]

# Firmware-related strings to locate and trace references to
FIRMWARE_STRINGS = [
    "Bad firmware signature",
    "DownloadFirmware",
    "Firmware download took",
    "LinuxUsbDevice::Init() Downloading firmware",
    "Loading custom firmware image to FX3",
    "Boot firmware must fit in 1 page",
    "firmware not available for device",
    "Unsupported firmware image type",
    "image_type == IMAGE_TYPE_NORMAL",
    "Endpoint count is not 4",
    "CY_U3P_USB_EVENT",
    "Can't reset the device seed and download the firmware",
    "ReseedDevice",
]

REPORT_FILENAME = "firmware-upload-trace.txt"


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
    """Find a function by exact or partial symbol name."""
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


def get_callees(program, func):
    """Get all functions that the given function calls."""
    callees = []
    if func is None:
        return callees
    body = func.getBody()
    listing = program.getListing()
    inst_iter = listing.getInstructions(body, True)
    seen = set()
    while inst_iter.hasNext():
        inst = inst_iter.next()
        if inst.getMnemonicString().startswith("CALL"):
            for i in range(inst.getNumOperands()):
                refs = inst.getOperandReferences(i)
                for ref in refs:
                    target = ref.getToAddress()
                    target_func = getFunctionAt(target)
                    if target_func is not None and target_func.getEntryPoint() not in seen:
                        seen.add(target_func.getEntryPoint())
                        callees.append((target_func, inst.getAddress()))
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


def try_extract_control_transfer_params(program, call_addr):
    """Try to extract libusb_control_transfer parameters.

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

    reg_map = {
        "ESI": "bmRequestType", "SIL": "bmRequestType", "RSI": "bmRequestType",
        "EDX": "bRequest", "DL": "bRequest", "RDX": "bRequest",
        "ECX": "wValue", "CL": "wValue", "RCX": "wValue",
        "R8D": "wIndex", "R8": "wIndex", "R8W": "wIndex",
    }

    addr = call_addr
    for _ in range(40):
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
                        try:
                            if op1.startswith("0x") or op1.startswith("-0x"):
                                params[param_name] = op1
                            elif op1.isdigit():
                                params[param_name] = "0x{:x}".format(int(op1))
                        except (ValueError, AttributeError):
                            pass
                        break

        if mnemonic.startswith("CALL") or mnemonic.startswith("J"):
            break

    return params


def find_string_address(program, target_string):
    """Find the address of a defined string in the binary."""
    for data in DefinedDataIterator.definedStrings(program):
        val = str(data.getValue())
        if target_string in val:
            return data.getAddress(), val
    return None, None


# ---------------------------------------------------------------------------
# Main script
# ---------------------------------------------------------------------------

def main():
    monitor = ConsoleTaskMonitor()
    program = currentProgram
    report_path = get_report_path()

    lines = []
    lines.append("=" * 78)
    lines.append("FX3 FIRMWARE UPLOAD TRACE")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: Locate firmware-related functions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: FIRMWARE-RELATED FUNCTIONS")
    lines.append("=" * 78)
    lines.append("")

    found_funcs = {}
    for name in FIRMWARE_FUNCTIONS:
        matches = find_functions_containing(program, name)
        if matches:
            for m in matches:
                found_funcs[m.getName()] = m
                lines.append("  {} @ {}".format(m.getName(), m.getEntryPoint()))
                lines.append("    Size: {} bytes".format(m.getBody().getNumAddresses()))
        else:
            lines.append("  [!] {} -- NOT FOUND".format(name))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 2: Trace DownloadFirmware call chain
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 2: DownloadFirmware CALL CHAIN")
    lines.append("=" * 78)
    lines.append("")

    for name, func in found_funcs.items():
        if "DownloadFirmware" not in name:
            continue

        lines.append("--- {} @ {} ---".format(name, func.getEntryPoint()))
        lines.append("")

        # Callers
        callers = get_callers(program, func)
        lines.append("  CALLERS ({}):" .format(len(callers)))
        for caller_func, call_addr in callers:
            lines.append("    {} @ {} (call at {})".format(
                caller_func.getName(), caller_func.getEntryPoint(), call_addr))
            # Show strings in the caller for context
            str_refs = get_string_refs_in_function(program, caller_func)
            relevant = [s for s in str_refs if any(kw in s.lower()
                        for kw in ["firmware", "download", "init", "device", "usb",
                                   "fpga", "endpoint", "reset"])]
            for s in relevant[:5]:
                lines.append("      \"{}\"".format(s[:80]))
        lines.append("")

        # Callees
        callees = get_callees(program, func)
        lines.append("  CALLEES ({}):" .format(len(callees)))
        for callee_func, call_addr in callees:
            callee_name = callee_func.getName()
            lines.append("    {} @ {} (call at {})".format(
                callee_name, callee_func.getEntryPoint(), call_addr))

            # If this callee is SendControlTransfer or libusb_control_transfer,
            # try to extract the parameters
            if "control_transfer" in callee_name.lower() or \
               "SendControlTransfer" in callee_name:
                params = try_extract_control_transfer_params(program, call_addr)
                if params:
                    lines.append("      Control Transfer Params:")
                    for k, v in sorted(params.items()):
                        lines.append("        {}: {}".format(k, v))
        lines.append("")

        # String references in DownloadFirmware itself
        str_refs = get_string_refs_in_function(program, func)
        lines.append("  STRING REFERENCES ({}):" .format(len(str_refs)))
        for s in str_refs:
            lines.append("    \"{}\"".format(s[:100]))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 3: Trace ParseFirmwareImage
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 3: ParseFirmwareImage ANALYSIS")
    lines.append("=" * 78)
    lines.append("")

    for name, func in found_funcs.items():
        if "ParseFirmwareImage" not in name:
            continue

        lines.append("--- {} @ {} ---".format(name, func.getEntryPoint()))
        lines.append("  Size: {} bytes".format(func.getBody().getNumAddresses()))
        lines.append("")

        # String references (should include "Bad firmware signature, must be CY")
        str_refs = get_string_refs_in_function(program, func)
        lines.append("  STRING REFERENCES:")
        for s in str_refs:
            lines.append("    \"{}\"".format(s[:100]))
        lines.append("")

        # Callers
        callers = get_callers(program, func)
        lines.append("  CALLERS ({}):" .format(len(callers)))
        for caller_func, call_addr in callers:
            lines.append("    {} @ {}".format(
                caller_func.getName(), caller_func.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 4: Firmware string references and their containing functions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 4: FIRMWARE STRING REFERENCES")
    lines.append("=" * 78)
    lines.append("")

    for target_str in FIRMWARE_STRINGS:
        addr, full_val = find_string_address(program, target_str)
        if addr is None:
            lines.append("  [!] \"{}\" -- NOT FOUND".format(target_str))
            continue

        lines.append("  \"{}\" @ {}".format(
            full_val[:80] if full_val else target_str, addr))
        refs = getReferencesTo(addr)
        for ref in refs:
            func = getFunctionContaining(ref.getFromAddress())
            if func:
                lines.append("    Referenced by: {} @ {}".format(
                    func.getName(), func.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 5: Control transfer call sites in firmware-related code paths
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 5: CONTROL TRANSFER PARAMETERS IN FIRMWARE CODE PATHS")
    lines.append("=" * 78)
    lines.append("")

    # Find libusb_control_transfer
    ctrl_func = find_function_by_name(program, "libusb_control_transfer")
    if ctrl_func:
        callers = get_callers(program, ctrl_func)
        for caller_func, call_addr in callers:
            caller_name = caller_func.getName()
            # Check if this caller is in a firmware-related code path
            str_refs = get_string_refs_in_function(program, caller_func)
            is_fw = any("firmware" in s.lower() or "download" in s.lower()
                        or "upload" in s.lower() or "0xa0" in s.lower()
                        for s in str_refs)

            # Also check if any known firmware function calls this function
            # (indirect firmware path)
            is_fw_caller = any(fw in caller_name
                              for fw in ["DownloadFirmware", "firmware", "Firmware",
                                         "SendControlTransfer"])

            if is_fw or is_fw_caller:
                params = try_extract_control_transfer_params(program, call_addr)
                lines.append("  {} @ {} (call at {})".format(
                    caller_name, caller_func.getEntryPoint(), call_addr))
                if params:
                    for k, v in sorted(params.items()):
                        lines.append("    {}: {}".format(k, v))
                    # Decode bmRequestType
                    if "bmRequestType" in params:
                        try:
                            brt = int(params["bmRequestType"], 16)
                            direction = "OUT" if not (brt & 0x80) else "IN"
                            req_type = (brt >> 5) & 0x03
                            type_names = {0: "Standard", 1: "Class",
                                          2: "Vendor", 3: "Reserved"}
                            lines.append("    -> Direction: {}".format(direction))
                            lines.append("    -> Type: {}".format(
                                type_names.get(req_type, "?")))
                        except (ValueError, TypeError):
                            pass
                else:
                    lines.append("    (parameters not statically resolvable)")
                lines.append("")

    # ------------------------------------------------------------------
    # Part 6: GetFirmwareForDeviceType analysis
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 6: GetFirmwareForDeviceType -- DEVICE-TO-FIRMWARE MAPPING")
    lines.append("=" * 78)
    lines.append("")

    for name, func in found_funcs.items():
        if "GetFirmwareForDeviceType" not in name:
            continue

        lines.append("--- {} @ {} ---".format(name, func.getEntryPoint()))
        lines.append("  Size: {} bytes".format(func.getBody().getNumAddresses()))
        lines.append("")

        # Callees (should show what firmware data it accesses)
        callees = get_callees(program, func)
        lines.append("  CALLEES ({}):" .format(len(callees)))
        for callee_func, call_addr in callees:
            lines.append("    {} @ {}".format(
                callee_func.getName(), callee_func.getEntryPoint()))
        lines.append("")

        # Callers
        callers = get_callers(program, func)
        lines.append("  CALLERS ({}):" .format(len(callers)))
        for caller_func, call_addr in callers:
            lines.append("    {} @ {}".format(
                caller_func.getName(), caller_func.getEntryPoint()))
        lines.append("")

        # String references
        str_refs = get_string_refs_in_function(program, func)
        lines.append("  STRING REFERENCES:")
        for s in str_refs:
            lines.append("    \"{}\"".format(s[:100]))
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

    println("")
    println("=== SUMMARY ===")
    println("Functions found: {}".format(len(found_funcs)))
    println("Report: {}".format(report_path))


if __name__ == "__main__":
    main()
else:
    main()
