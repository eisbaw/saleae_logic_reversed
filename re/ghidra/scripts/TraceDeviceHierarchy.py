# TraceDeviceHierarchy.py -- Ghidra script to trace the device class hierarchy
# in libgraph_server_shared.so and extract per-device virtual method
# overrides, device constants, and configuration differences.
#
# This script:
# 1. Locates RTTI typeinfo objects for all device classes
# 2. Follows base class pointers to reconstruct the inheritance tree
# 3. Finds and compares vtables to identify overridden methods
# 4. Extracts string references from per-device virtual methods to
#    identify device-specific behaviors (ADC types, channel counts, etc.)
#
# Output: re/ghidra/reports/device-hierarchy.txt
#
# Usage in Ghidra:
#   Script Manager -> Run -> TraceDeviceHierarchy.py
#
# @author  saleae_reverse project
# @category DeviceAnalysis
# @keybinding
# @menupath
# @toolbar

from ghidra.program.model.symbol import SymbolType
from ghidra.program.model.data import PointerDataType
from ghidra.program.util import DefinedDataIterator
from ghidra.util.task import ConsoleTaskMonitor

import os
import time

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# Device class names from RTTI analysis
DEVICE_CLASSES = [
    "GenericDevice",
    "LogicAnalyzerDevice",
    "LogicDevice",
    "Logic16Device",
    "Logic2FpgaDevice",
    "LogicStudentDevice",
    "LogicGraduateDevice",
    "LogicProfessionalDevice",
    "LogicProfessional8Device",
    "LogicProfessional16Device",
]

FEATURE_CLASSES = [
    "Fx2DeviceFeatures",
    "Fx3DeviceFeatures",
    "FpgaDeviceFeatures",
    "ProFpgaDeviceFeatures",
]

USB_CLASSES = [
    "UsbDevice",
    "LinuxUsbDevice",
]

MANAGER_CLASSES = [
    "DevicesManager",
    "LinuxDevicesManager",
]

ALL_CLASSES = DEVICE_CLASSES + FEATURE_CLASSES + USB_CLASSES + MANAGER_CLASSES

# Known virtual method signatures (from string analysis of assert messages)
# These help annotate vtable entries
KNOWN_METHODS = {
    # FpgaDeviceFeatures methods
    "FpgaDeviceFeatures": [
        "~FpgaDeviceFeatures()",                              # slot 0 (dtor)
        "ReadRegisters(vector<U8>&, const S8*)",               # slot 1
        "WriteRegisters(vector<pair<U8,U8>>&, const S8*)",     # slot 2 or 3
        "WriteConfigurationData(const U8*, U32)",              # slot ?
        "WriteRegisterBit(U8, U8, U8, const S8*)",            # slot ?
    ],
    # Fx2DeviceFeatures methods
    "Fx2DeviceFeatures": [
        "~Fx2DeviceFeatures()",                               # slot 0
        "ReadEeprom(U16, U8*, U16)",                          # slot ?
        "WriteEeprom(U16, vector<U8>&)",                      # slot ?
        "ResetAndReEnumerate()",                               # slot ?
    ],
    # LogicDevice methods
    "LogicAnalyzerDevice": [
        "~LogicAnalyzerDevice()",                             # slot 0
        "StartSampling(DeviceSettings&)",                     # slot ?
        "GetDeviceSettings()",                                # slot ?
        "ComputeTransferSize(U64)",                           # slot ?
    ],
}

REPORT_FILENAME = "device-hierarchy.txt"


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


def find_rtti_name_string(program, class_name):
    """Find the RTTI name string for a class.

    In Itanium ABI, typeinfo name strings have the format:
    <length><ClassName>\\0
    e.g. "13GenericDevice\\0"
    """
    target = "{}{}".format(len(class_name), class_name)
    for sdata in DefinedDataIterator.definedStrings(program):
        val = str(sdata.getValue())
        if val == target:
            return sdata.getAddress()
    # Fallback: search in memory
    mem = program.getMemory()
    target_bytes = target.encode("ascii") + b"\x00"
    addr = mem.findBytes(program.getMinAddress(), target_bytes,
                         None, True, ConsoleTaskMonitor())
    return addr


def find_typeinfo_for_name(program, name_addr):
    """Find the typeinfo object that references this RTTI name string.

    A typeinfo struct in .data.rel.ro has:
      [+0] vtable pointer (to __class_type_info, __si_class_type_info, etc.)
      [+8] name pointer   -> points to the RTTI name string
      [+16] base pointer  (for __si_class_type_info only)

    We find all references to name_addr and check which one looks like
    a typeinfo (the reference is at the struct's +8 offset).
    """
    if name_addr is None:
        return None

    refs = getReferencesTo(name_addr)
    for ref in refs:
        from_addr = ref.getFromAddress()
        # The typeinfo starts 8 bytes before the name pointer
        ti_addr = from_addr.subtract(8)
        # Check if this is in .data.rel.ro
        block = program.getMemory().getBlock(ti_addr)
        if block is not None and ".data.rel.ro" in block.getName():
            return ti_addr

    return None


def read_pointer(program, addr):
    """Read a 64-bit pointer value from an address."""
    mem = program.getMemory()
    try:
        return mem.getLong(addr) & 0xFFFFFFFFFFFFFFFF
    except Exception:
        return None


def get_base_class_name(program, typeinfo_addr):
    """Follow the base class pointer in a __si_class_type_info to get
    the base class name."""
    if typeinfo_addr is None:
        return None

    # Read the name pointer at +8 to verify this is a typeinfo
    name_ptr_addr = typeinfo_addr.add(8)
    name_ptr = read_pointer(program, name_ptr_addr)
    if name_ptr is None:
        return None

    # Read the base class typeinfo pointer at +16
    base_ti_ptr_addr = typeinfo_addr.add(16)
    refs = getReferencesFrom(base_ti_ptr_addr)

    for ref in refs:
        base_ti_addr = ref.getToAddress()
        # Read the base's name pointer at base_ti + 8
        base_name_ptr_addr = base_ti_addr.add(8)
        base_name_refs = getReferencesFrom(base_name_ptr_addr)
        for bnref in base_name_refs:
            name_addr = bnref.getToAddress()
            listing = program.getListing()
            data = listing.getDataAt(name_addr)
            if data is not None and data.hasStringValue():
                raw_name = str(data.getValue())
                # Parse: "13GenericDevice" -> "GenericDevice"
                i = 0
                while i < len(raw_name) and raw_name[i].isdigit():
                    i += 1
                return raw_name[i:]

    return None


def find_vtable_entries(program, typeinfo_addr):
    """Find the vtable associated with a typeinfo and return the list
    of virtual function addresses.

    The vtable layout (Itanium ABI):
      [-16] offset_to_top (0 for primary vtable)
      [-8]  typeinfo pointer
      [0]   first virtual function
      [8]   second virtual function
      ...

    We search .data.rel.ro for references to this typeinfo, then walk
    forward collecting code pointers.
    """
    if typeinfo_addr is None:
        return []

    refs = getReferencesTo(typeinfo_addr)
    best_vtable = []

    for ref in refs:
        from_addr = ref.getFromAddress()
        block = program.getMemory().getBlock(from_addr)
        if block is None or ".data.rel.ro" not in block.getName():
            continue

        # Check if offset_to_top (at from_addr - 8) is zero
        ott_addr = from_addr.subtract(8)
        ott_val = read_pointer(program, ott_addr)
        if ott_val != 0:
            continue  # Not the primary vtable

        # The first virtual function is at from_addr + 8
        vptr_addr = from_addr.add(8)
        entries = []

        for i in range(60):
            func_addr_val = read_pointer(program, vptr_addr.add(i * 8))
            if func_addr_val is None or func_addr_val == 0:
                break

            # Check if this address is in executable memory
            target = program.getAddressFactory().getDefaultAddressSpace().getAddress(func_addr_val)
            target_block = program.getMemory().getBlock(target)
            if target_block is not None and target_block.isExecute():
                entries.append(target)
            else:
                break

        if len(entries) > len(best_vtable):
            best_vtable = entries

    return best_vtable


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


# ---------------------------------------------------------------------------
# Main script
# ---------------------------------------------------------------------------

def main():
    monitor = ConsoleTaskMonitor()
    program = currentProgram
    report_path = get_report_path()

    lines = []
    lines.append("=" * 78)
    lines.append("DEVICE CLASS HIERARCHY ANALYSIS")
    lines.append("Binary: {}".format(program.getName()))
    lines.append("Date: {}".format(time.strftime("%Y-%m-%d %H:%M:%S")))
    lines.append("=" * 78)
    lines.append("")

    # ------------------------------------------------------------------
    # Part 1: Locate all RTTI typeinfo objects
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 1: RTTI TYPEINFO OBJECTS")
    lines.append("=" * 78)
    lines.append("")

    typeinfo_map = {}  # class_name -> typeinfo_addr
    name_addr_map = {}  # class_name -> rtti_name_addr

    for class_name in ALL_CLASSES:
        name_addr = find_rtti_name_string(program, class_name)
        if name_addr is None:
            lines.append("[!] {}: RTTI name string NOT FOUND".format(class_name))
            continue

        name_addr_map[class_name] = name_addr
        ti_addr = find_typeinfo_for_name(program, name_addr)
        if ti_addr is None:
            lines.append("[!] {}: typeinfo NOT FOUND (name at {})".format(
                class_name, name_addr))
            continue

        typeinfo_map[class_name] = ti_addr
        lines.append("  {}: name={}, typeinfo={}".format(
            class_name, name_addr, ti_addr))

    lines.append("")
    lines.append("Found {}/{} typeinfo objects".format(
        len(typeinfo_map), len(ALL_CLASSES)))
    lines.append("")

    # ------------------------------------------------------------------
    # Part 2: Reconstruct inheritance tree
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 2: INHERITANCE TREE (from RTTI base class pointers)")
    lines.append("=" * 78)
    lines.append("")

    inheritance = {}  # child -> parent
    for class_name, ti_addr in typeinfo_map.items():
        base_name = get_base_class_name(program, ti_addr)
        if base_name:
            inheritance[class_name] = base_name
            lines.append("  {} -> {}".format(class_name, base_name))
        else:
            lines.append("  {} -> [ROOT]".format(class_name))

    lines.append("")

    # ------------------------------------------------------------------
    # Part 3: Vtable analysis and comparison
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 3: VTABLE ANALYSIS")
    lines.append("=" * 78)
    lines.append("")

    vtable_map = {}  # class_name -> [func_addrs]
    for class_name, ti_addr in typeinfo_map.items():
        entries = find_vtable_entries(program, ti_addr)
        vtable_map[class_name] = entries
        lines.append("  {}: {} virtual methods".format(class_name, len(entries)))

    lines.append("")

    # Compare parent-child vtables
    lines.append("--- Vtable Comparisons ---")
    lines.append("")

    for child, parent in sorted(inheritance.items()):
        child_vt = vtable_map.get(child, [])
        parent_vt = vtable_map.get(parent, [])
        if not child_vt or not parent_vt:
            continue

        overrides = 0
        new_methods = max(0, len(child_vt) - len(parent_vt))
        for i in range(min(len(child_vt), len(parent_vt))):
            if child_vt[i] != parent_vt[i]:
                overrides += 1

        lines.append("  {} (extends {}):".format(child, parent))
        lines.append("    {} methods, {} overrides, {} new".format(
            len(child_vt), overrides, new_methods))

        # Show individual override details
        for i in range(min(len(child_vt), len(parent_vt))):
            if child_vt[i] != parent_vt[i]:
                child_func = getFunctionAt(child_vt[i])
                parent_func = getFunctionAt(parent_vt[i])
                child_name = child_func.getName() if child_func else "?"
                parent_name = parent_func.getName() if parent_func else "?"
                lines.append("    [{:2d}] {} -> {} (was {})".format(
                    i, child_vt[i], child_name, parent_name))

        lines.append("")

    # ------------------------------------------------------------------
    # Part 4: Per-device string analysis
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 4: PER-DEVICE VIRTUAL METHOD STRING REFERENCES")
    lines.append("(Strings referenced from overridden virtual methods)")
    lines.append("=" * 78)
    lines.append("")

    for child, parent in sorted(inheritance.items()):
        child_vt = vtable_map.get(child, [])
        parent_vt = vtable_map.get(parent, [])
        if not child_vt:
            continue

        has_output = False
        for i in range(len(child_vt)):
            is_override = (i < len(parent_vt) and child_vt[i] != parent_vt[i])
            is_new = (i >= len(parent_vt))

            if is_override or is_new:
                func = getFunctionAt(child_vt[i])
                if func is None:
                    continue

                strings = get_string_refs_in_function(program, func)
                if strings:
                    if not has_output:
                        lines.append("--- {} ---".format(child))
                        has_output = True

                    tag = "override" if is_override else "new"
                    lines.append("  [{:2d}] {} ({}) @ {}:".format(
                        i, func.getName(), tag, func.getEntryPoint()))
                    for s in strings[:15]:
                        lines.append("    \"{}\"".format(s[:120]))
                    lines.append("")

        if has_output:
            lines.append("")

    # ------------------------------------------------------------------
    # Part 5: Feature class comparison
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 5: FEATURE CLASS VTABLE COMPARISON")
    lines.append("=" * 78)
    lines.append("")

    comparisons = [
        ("FpgaDeviceFeatures", "ProFpgaDeviceFeatures"),
        ("Fx2DeviceFeatures", "Fx3DeviceFeatures"),
    ]

    for parent_name, child_name in comparisons:
        parent_vt = vtable_map.get(parent_name, [])
        child_vt = vtable_map.get(child_name, [])
        if not parent_vt and not child_vt:
            continue

        lines.append("--- {} vs {} ---".format(parent_name, child_name))
        for i in range(max(len(parent_vt), len(child_vt))):
            p = parent_vt[i] if i < len(parent_vt) else None
            c = child_vt[i] if i < len(child_vt) else None
            if p == c:
                pfunc = getFunctionAt(p) if p else None
                pname = pfunc.getName() if pfunc else "?"
                lines.append("  [{:2d}] SAME   {} ({})".format(i, p, pname))
            else:
                pfunc = getFunctionAt(p) if p else None
                cfunc = getFunctionAt(c) if c else None
                pname = pfunc.getName() if pfunc else "---"
                cname = cfunc.getName() if cfunc else "---"
                lines.append("  [{:2d}] DIFF   parent={}({})  child={}({})".format(
                    i, p or "---", pname, c or "---", cname))
        lines.append("")

    # ------------------------------------------------------------------
    # Part 6: Device factory analysis
    # ------------------------------------------------------------------
    lines.append("=" * 78)
    lines.append("PART 6: DEVICE FACTORY (AddNewDevice)")
    lines.append("=" * 78)
    lines.append("")

    # Search for AddNewDevice function
    fm = program.getFunctionManager()
    funcs = fm.getFunctions(True)
    for func in funcs:
        if "AddNewDevice" in func.getName():
            lines.append("  {} @ {}".format(func.getName(), func.getEntryPoint()))
            strings = get_string_refs_in_function(program, func)
            if strings:
                lines.append("  Strings:")
                for s in strings[:30]:
                    lines.append("    \"{}\"".format(s[:120]))
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
    println("Classes found: {}/{}".format(len(typeinfo_map), len(ALL_CLASSES)))
    println("Inheritance links: {}".format(len(inheritance)))
    println("Report: {}".format(report_path))


# Entry point
if __name__ == "__main__":
    main()
else:
    main()
