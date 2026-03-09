# TraceDeviceStates.py -- Ghidra script to trace the Saleae device state
# machine in libgraph_server_shared.so.
#
# The Saleae Logic 2 application uses a typed state machine for device
# lifecycle management. From string analysis, the state machine is:
#
#   std::variant<Saleae::DSInitializing, Saleae::DSReady, Saleae::DSError>
#
# This script:
#   1. Finds GenericDevice::SetDeviceState and its callers
#   2. Finds all functions referencing DSInitializing/DSReady/DSError strings
#   3. Traces DeviceListNode::OnConnect (device attach handler)
#   4. Traces LogicDeviceNode handler methods (StartCapture, StopCapture, etc.)
#   5. Maps state transition enforcement strings
#   6. Finds LogicDeviceState serialization (IPC state broadcast)
#
# Output: re/ghidra/reports/device-states.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceDeviceStates.py
#
# @author  saleae_reverse project
# @category DeviceState
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

# State-related type/class names from RTTI and mangled symbols
STATE_TYPE_STRINGS = [
    "DSInitializing",
    "DSReady",
    "DSError",
    "MsoStateInitializing",
    "MsoStateReady",
    "MsoStateError",
    "LogicDeviceState",
    "DeviceListState",
    "NodeExecutionState",
]

# State transition enforcement strings (assertion messages)
STATE_TRANSITION_STRINGS = [
    "Cannot move from Error state to non-Error state state",
    "Cannot move from Ready state to Initializing state",
    "Device tried to move from one error state to another",
    "Cannot start capture while device is not in Ready state",
    "Device already initialized",
    "Attempting to initialize, but device is no longer initializing",
    "Only Stopped->Executing is a valid transition",
    "Only Stopped->Invalid is a valid transition",
    "Stopped->Stopped is not a valid NodeExecutionState transition",
]

# Key functions in the device lifecycle
LIFECYCLE_FUNCTIONS = [
    "SetDeviceState",
    "OnConnect",
    "OnDisconnect",
    "HandleStartCapture",
    "HandleStopCapture",
    "HandleAcquireLogicDeviceRequest",
    "HandleReleaseDeviceRequest",
    "OnInitialDeviceSearchComplete",
    "CommonDeviceInit",
    "DownloadFirmware",
    "ProgramFpgaBitstream",
    "IsDeviceResponsive",
    "IsFirmwareDownloaded",
    "IsFpgaResponsive",
    "TestFirmwareResponsivnessAndReseed",
    "AddNewDevice",
    "AcquireDeviceById",
    "Initialize_PreWallPowerDetected",
    "Initialize_PostWallPowerDetected",
    "HandleDeviceError",
    "SetMsoState",
    "UseState",
]

# JSON field paths related to device state (serialization keys)
STATE_JSON_FIELDS = [
    "/state",
    "/deviceState",
    "/deviceType",
    "/deviceId",
    "/connectedDevices",
    "/lastConnectionError",
    "/errorState",
    "/analogState",
    "/channelStates",
    "/hardwareRevision",
    "/isCapturing",
    "/capturing",
    "/overrideUsb2PowerLimit",
]

REPORT_FILENAME = "device-states.txt"


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
                    target_func = getFunctionAt(target)
                    if target_func and target_func.getEntryPoint() not in seen:
                        seen.add(target_func.getEntryPoint())
                        callees.append(target_func)
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


def find_strings_matching(program, substring):
    """Find all defined strings containing the given substring."""
    results = []
    for data in DefinedDataIterator.definedStrings(program):
        val = str(data.getValue())
        if substring in val:
            results.append((data.getAddress(), val))
    return results


def find_functions_referencing_string(program, string_addr):
    """Find all functions that reference a given string address."""
    funcs = []
    refs = getReferencesTo(string_addr)
    seen = set()
    for ref in refs:
        func = getFunctionContaining(ref.getFromAddress())
        if func and func.getEntryPoint() not in seen:
            seen.add(func.getEntryPoint())
            funcs.append(func)
    return funcs


# ---------------------------------------------------------------------------
# Main script
# ---------------------------------------------------------------------------

def main():
    monitor = ConsoleTaskMonitor()
    program = currentProgram
    report_path = get_report_path()

    lines = []
    lines.append("=" * 78)
    lines.append("DEVICE STATE MACHINE ANALYSIS")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: State type references (RTTI / mangled symbol names)
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: STATE TYPE REFERENCES")
    lines.append("(RTTI strings and mangled symbols referencing state types)")
    lines.append("=" * 78)
    lines.append("")

    for type_name in STATE_TYPE_STRINGS:
        matches = find_strings_matching(program, type_name)
        lines.append("--- {} ---".format(type_name))
        lines.append("  {} string reference(s)".format(len(matches)))
        for addr, val in matches[:15]:
            # Truncate long mangled names
            display = val if len(val) < 120 else val[:117] + "..."
            lines.append("    @ {} : {}".format(addr, display))
            # Find who references this string
            ref_funcs = find_functions_referencing_string(program, addr)
            for rf in ref_funcs[:5]:
                lines.append("      -> referenced by {} @ {}".format(
                    rf.getName(), rf.getEntryPoint()))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 2: State transition enforcement
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 2: STATE TRANSITION ENFORCEMENT")
    lines.append("(Assertion/error messages that enforce valid transitions)")
    lines.append("=" * 78)
    lines.append("")

    for msg in STATE_TRANSITION_STRINGS:
        # Search for a substring (messages might be truncated differently)
        search_term = msg[:60]  # first 60 chars should be unique enough
        matches = find_strings_matching(program, search_term)
        if matches:
            for addr, val in matches:
                lines.append("  String: \"{}\"".format(val[:100]))
                lines.append("  Address: {}".format(addr))
                ref_funcs = find_functions_referencing_string(program, addr)
                for rf in ref_funcs:
                    lines.append("    -> in function: {} @ {}".format(
                        rf.getName(), rf.getEntryPoint()))
                    # Get other strings in the same function for context
                    other_strings = get_string_refs_in_function(program, rf)
                    state_related = [s for s in other_strings
                                     if any(kw in s for kw in
                                            ["state", "State", "DS", "Ready",
                                             "Error", "Initializing", "capture",
                                             "device", "Device"])]
                    if state_related:
                        lines.append("    Other state strings in function:")
                        for s in state_related[:10]:
                            lines.append("      \"{}\"".format(s[:100]))
                lines.append("")
        else:
            lines.append("  [!] NOT FOUND: \"{}\"".format(msg[:80]))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 3: Device lifecycle functions
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 3: DEVICE LIFECYCLE FUNCTIONS")
    lines.append("=" * 78)
    lines.append("")

    for func_name in LIFECYCLE_FUNCTIONS:
        matches = find_functions_containing(program, func_name)
        if matches:
            lines.append("--- {} ({} match(es)) ---".format(
                func_name, len(matches)))
            for m in matches[:5]:
                lines.append("  {} @ {}".format(m.getName(), m.getEntryPoint()))
                lines.append("    Size: {} bytes".format(
                    m.getBody().getNumAddresses()))

                # Callers
                callers = get_callers(program, m)
                if callers:
                    lines.append("    Callers ({}):" .format(len(callers)))
                    for c, addr in callers[:10]:
                        lines.append("      <- {} @ {}".format(
                            c.getName(), c.getEntryPoint()))

                # Callees (what does this function call?)
                callees = get_callees(program, m)
                lifecycle_callees = [c for c in callees
                                     if any(lf in c.getName()
                                            for lf in LIFECYCLE_FUNCTIONS)]
                if lifecycle_callees:
                    lines.append("    Lifecycle callees:")
                    for c in lifecycle_callees:
                        lines.append("      -> {} @ {}".format(
                            c.getName(), c.getEntryPoint()))

                # State-related strings
                str_refs = get_string_refs_in_function(program, m)
                state_strs = [s for s in str_refs
                              if any(kw in s for kw in
                                     ["state", "State", "DS", "Ready",
                                      "Error", "Initializing", "capture",
                                      "Capture", "device", "Device",
                                      "firmware", "Firmware", "FPGA",
                                      "fpga", "responsive", "endpoint",
                                      "Endpoint", "connected", "disconnect"])]
                if state_strs:
                    lines.append("    State-related strings:")
                    for s in state_strs[:10]:
                        lines.append("      \"{}\"".format(s[:100]))

                lines.append("")
        else:
            lines.append("--- {} --- [NOT FOUND]".format(func_name))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 4: Serialized state JSON fields
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 4: SERIALIZED STATE JSON FIELDS")
    lines.append("(JSON field paths used to serialize device state to IPC)")
    lines.append("=" * 78)
    lines.append("")

    for field in STATE_JSON_FIELDS:
        matches = find_strings_matching(program, field)
        if matches:
            for addr, val in matches:
                lines.append("  \"{}\" @ {}".format(val, addr))
                ref_funcs = find_functions_referencing_string(program, addr)
                for rf in ref_funcs[:5]:
                    lines.append("    -> {} @ {}".format(
                        rf.getName(), rf.getEntryPoint()))
        else:
            lines.append("  [!] \"{}\" -- not found".format(field))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 5: Device connection flow (DevicesManager -> DeviceListNode)
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 5: DEVICE CONNECTION FLOW")
    lines.append("=" * 78)
    lines.append("")

    connection_funcs = [
        "DevicesManager",
        "LinuxDevicesManager",
        "CheckForConnectionThread",
        "OnInitialDeviceSearchComplete",
        "hotplug",
    ]
    for cf in connection_funcs:
        matches = find_functions_containing(program, cf)
        if matches:
            lines.append("--- {} ({} match(es)) ---".format(cf, len(matches)))
            for m in matches[:10]:
                lines.append("  {} @ {}".format(m.getName(), m.getEntryPoint()))
            lines.append("")
        else:
            lines.append("--- {} --- [NOT FOUND]".format(cf))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 6: USB speed and endpoint validation
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 6: USB SPEED AND ENDPOINT VALIDATION")
    lines.append("=" * 78)
    lines.append("")

    speed_strings = [
        "USB Full Speed",
        "USB High Speed",
        "USB Super Speed",
        "Invalid USB speed",
        "Endpoint count is not 4",
        "Expected endpoint not found",
        "Device failed 'IsDeviceResponsive'",
    ]
    for ss in speed_strings:
        matches = find_strings_matching(program, ss)
        if matches:
            for addr, val in matches:
                lines.append("  \"{}\" @ {}".format(val[:100], addr))
                ref_funcs = find_functions_referencing_string(program, addr)
                for rf in ref_funcs:
                    lines.append("    -> {} @ {}".format(
                        rf.getName(), rf.getEntryPoint()))
            lines.append("")
        else:
            lines.append("  [!] \"{}\" -- not found".format(ss))
            lines.append("")

    # ------------------------------------------------------------------
    # Part 7: LogicDevice command handlers
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 7: LOGICDEVICE COMMAND HANDLERS")
    lines.append("(Methods registered as request handlers on LogicDeviceNode)")
    lines.append("=" * 78)
    lines.append("")

    handler_names = [
        "HandleStartCapture",
        "HandleStopCapture",  # mapped as StopCapture handler
        "HandleAcquireLogicDeviceRequest",
        "HandleReleaseDeviceRequest",
        "SetSampleRate",
        "EnableChannels",
        "SetDigitalVoltageThreshold",
        "SetAdcFullScaleGain",
        "SetLed",
        "GetDefaultLogicDeviceConfiguration",
        "OnDataBuffer",
        "StartSampling",
        "SetCaptureParameters",
    ]
    for hn in handler_names:
        matches = find_functions_containing(program, hn)
        if matches:
            lines.append("--- {} ({} match(es)) ---".format(hn, len(matches)))
            for m in matches[:3]:
                lines.append("  {} @ {} ({} bytes)".format(
                    m.getName(), m.getEntryPoint(),
                    m.getBody().getNumAddresses()))
            lines.append("")
        else:
            lines.append("--- {} --- [NOT FOUND]".format(hn))
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
    println("Report: {}".format(report_path))


# Entry point
if __name__ == "__main__":
    main()
else:
    main()
