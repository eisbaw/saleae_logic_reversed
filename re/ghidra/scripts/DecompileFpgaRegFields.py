# DecompileFpgaRegFields.py -- Decompile functions that write to specific FPGA registers
#
# Targets:
# 1. CAPTURE_CONTROL (0x0040) -- all writers, full bit layout
# 2. SCOPE_CONFIG_IDs (0x0044-0x0050) -- SetConfigurationIds
# 3. INIT_CONFIG (0x00E0) -- post-programming init value
# 4. TRIGGER_TICK_SIZE (0x0094) -- clock rate constant
# 5. DIGITAL_VOLTAGE_THRESHOLD (0x0104) -- secondary write mechanism
#
# @category Saleae
# @author RE team

from ghidra.app.decompiler import DecompInterface
from ghidra.util.task import ConsoleTaskMonitor
from ghidra.program.util import DefinedDataIterator
from ghidra.program.model.lang import OperandType

import codecs

OUTPUT_FILE = "/tmp/ghidra-fpga-reg-fields.txt"

# Target register addresses (MsoFpga 16-bit addresses)
TARGET_REGS = {
    0x0040: "CAPTURE_CONTROL",
    0x0044: "SCOPE_CONFIG_ID_0",
    0x0048: "SCOPE_CONFIG_ID_1",
    0x004C: "SCOPE_CONFIG_ID_2",
    0x0050: "SCOPE_CONFIG_ID_3",
    0x0094: "TRIGGER_TICK_SIZE",
    0x00E0: "INIT_CONFIG",
    0x0104: "DIGITAL_VOLTAGE_THRESHOLD",
}

# Strings that anchor us to the right functions
ANCHOR_STRINGS = [
    "Setting scope configuration IDs",
    "SetConfigurationIds",
    "SetAnalogChannelConfiguration",
    "SetAnalogTriggerConfiguration",
    "SetDigitalDerivedVoltageThresholds",
    "SetDigitalVoltageThreshold",
    "HandleSetDigitalVoltageThreshold",
    "Write 0x{:x} to 0x{:x}",
    "FPGA WriteRegister 0x{:04x}",
    "voltage threshold",
    "set digital voltage",
    # Init sequence related
    "GPIF enabled",
    "EnableGpif",
    "InitializeFpga",
    "ProgramFpga",
    "SetInitialFpgaState",
    "Waiting for DDR",
    "DDR ready",
    "JESD",
    "InitialConfig",
]


def find_strings(program, search_terms):
    """Find addresses of strings containing any of the search terms."""
    results = {}
    for data in DefinedDataIterator.definedStrings(program):
        val = data.getValue()
        if val is None:
            continue
        s = str(val)
        for term in search_terms:
            if term in s:
                results.setdefault(term, []).append(data.getAddress())
    return results


def get_functions_referencing(program, addr):
    """Get all functions that reference a given address."""
    ref_mgr = program.getReferenceManager()
    fm = program.getFunctionManager()
    funcs = set()
    for ref in ref_mgr.getReferencesTo(addr):
        from_addr = ref.getFromAddress()
        func = fm.getFunctionContaining(from_addr)
        if func is not None:
            funcs.add(func)
    return funcs


def scan_for_register_immediates(program, func, target_values):
    """Scan a function's instructions for MOV with target immediate values."""
    listing = program.getListing()
    body = func.getBody()
    hits = []

    inst_iter = listing.getInstructions(body, True)
    while inst_iter.hasNext():
        inst = inst_iter.next()
        mnemonic = inst.getMnemonicString().upper()
        if mnemonic in ("MOV", "MOVW", "MOVL", "MOVABS"):
            for i in range(inst.getNumOperands()):
                op_type = inst.getOperandType(i)
                if (op_type & OperandType.SCALAR) != 0:
                    scalar = inst.getScalar(i)
                    if scalar is not None:
                        uval = scalar.getUnsignedValue()
                        if uval in target_values:
                            hits.append({
                                'addr': inst.getAddress(),
                                'value': uval,
                                'instruction': str(inst),
                                'reg_name': target_values[uval],
                            })
    return hits


def decompile_function(decomp, func, monitor):
    """Decompile a single function, return C code string."""
    result = decomp.decompileFunction(func, 120, monitor)
    if result.decompileCompleted():
        df = result.getDecompiledFunction()
        if df is not None:
            return df.getC()
    return None


def get_callers(program, func):
    """Get functions that call this function."""
    fm = program.getFunctionManager()
    ref_mgr = program.getReferenceManager()
    callers = set()
    for ref in ref_mgr.getReferencesTo(func.getEntryPoint()):
        caller = fm.getFunctionContaining(ref.getFromAddress())
        if caller is not None and caller != func:
            callers.add(caller)
    return callers


def main():
    program = currentProgram
    monitor = ConsoleTaskMonitor()

    decomp = DecompInterface()
    decomp.openProgram(program)

    output = []
    output.append("=" * 70)
    output.append("DecompileFpgaRegFields.py -- FPGA Register Field Analysis")
    output.append("=" * 70)
    output.append("")

    # Phase 1: Find anchor strings
    output.append("[Phase 1] Finding anchor strings...")
    string_addrs = find_strings(program, ANCHOR_STRINGS)

    for term in ANCHOR_STRINGS:
        if term in string_addrs:
            output.append("  FOUND: '{}' at {}".format(
                term[:60], ", ".join(str(a) for a in string_addrs[term])))
        else:
            output.append("  MISSING: '{}'".format(term[:60]))
    output.append("")

    # Phase 2: Find functions referencing anchor strings
    output.append("[Phase 2] Finding functions referencing anchor strings...")
    target_funcs = set()
    func_to_anchors = {}

    for term, addrs in string_addrs.items():
        for addr in addrs:
            funcs = get_functions_referencing(program, addr)
            for func in funcs:
                target_funcs.add(func)
                func_to_anchors.setdefault(func, []).append(term)
                output.append("  {} at {} referenced by '{}'".format(
                    func.getName(), func.getEntryPoint(), term[:50]))

    output.append("  Total functions from anchors: {}".format(len(target_funcs)))
    output.append("")

    # Phase 3: Scan anchor functions for target register immediates
    output.append("[Phase 3] Scanning anchor functions for target register immediates...")

    funcs_with_regs = set()
    for func in sorted(target_funcs, key=lambda f: f.getEntryPoint().getOffset()):
        hits = scan_for_register_immediates(program, func, TARGET_REGS)
        if hits:
            funcs_with_regs.add(func)
            output.append("  {} at {} writes:".format(func.getName(), func.getEntryPoint()))
            for h in hits:
                output.append("    0x{:04X} ({}) at {} -- {}".format(
                    h['value'], h['reg_name'], h['addr'], h['instruction']))
    output.append("")

    # Phase 4: For functions with register writes, also find their callers
    # (to trace init sequences)
    output.append("[Phase 4] Finding callers of register-writing functions...")
    caller_funcs = set()
    for func in funcs_with_regs:
        callers = get_callers(program, func)
        for caller in callers:
            if caller not in target_funcs:
                caller_funcs.add(caller)
                func_to_anchors.setdefault(caller, []).append(
                    "(caller of {})".format(func.getName()))
                output.append("  {} at {} calls {}".format(
                    caller.getName(), caller.getEntryPoint(), func.getName()))
    output.append("  Additional caller functions: {}".format(len(caller_funcs)))
    output.append("")

    # Phase 5: Decompile all relevant functions
    # Prioritize: functions with register writes first, then callers
    all_funcs = funcs_with_regs | caller_funcs
    # Also include functions anchored by specific strings even without register immediates
    for func in target_funcs:
        anchors = func_to_anchors.get(func, [])
        for a in anchors:
            if any(key in a for key in [
                "SetConfigurationIds", "SetAnalogChannel", "SetAnalogTrigger",
                "SetDigitalDerived", "SetDigitalVoltage", "HandleSetDigital",
                "EnableGpif", "InitializeFpga", "ProgramFpga", "SetInitialFpga",
                "DDR", "JESD", "InitialConfig", "scope configuration",
                "Write 0x{:x}", "voltage threshold"
            ]):
                all_funcs.add(func)
                break

    output.append("[Phase 5] Decompiling {} functions...".format(len(all_funcs)))
    output.append("")

    for func in sorted(all_funcs, key=lambda f: f.getEntryPoint().getOffset()):
        entry = func.getEntryPoint()
        elf_va = entry.getOffset() - 0x100000
        anchors = func_to_anchors.get(func, ["(no anchor)"])

        output.append("=" * 70)
        output.append("=== {} @ {} (ELF VA: 0x{:x}) ===".format(
            func.getName(), entry, elf_va))
        output.append("=== Anchors: {} ===".format(", ".join(str(a)[:50] for a in anchors)))
        output.append("=" * 70)

        hits = scan_for_register_immediates(program, func, TARGET_REGS)
        if hits:
            output.append("Target register writes in this function:")
            for h in hits:
                output.append("  0x{:04X} ({}) at {}".format(
                    h['value'], h['reg_name'], h['addr']))

        c_code = decompile_function(decomp, func, monitor)
        if c_code:
            output.append(c_code)
        else:
            output.append("  DECOMPILATION FAILED")
        output.append("")

    # Phase 6: Read voltage threshold address table
    output.append("=" * 70)
    output.append("[Phase 6] Reading voltage threshold address table...")
    output.append("=" * 70)

    addr_space = program.getAddressFactory().getDefaultAddressSpace()
    mem = program.getMemory()

    try:
        dat_addr = addr_space.getAddress(0x0089ad8a)
        output.append("")
        output.append("DAT_0089ad8a (voltage threshold I2C address table):")
        for i in range(4):
            a = dat_addr.add(i * 2)
            val = mem.getShort(a) & 0xFFFF
            output.append("  channel {}: address = 0x{:04X}".format(i, val))
    except Exception as e:
        output.append("  Error reading DAT_0089ad8a: {}".format(str(e)))

    output.append("")

    # Write output
    text = u"\n".join(output)
    with codecs.open(OUTPUT_FILE, "w", encoding="utf-8") as f:
        f.write(text)

    println("Output written to " + OUTPUT_FILE)
    println("Functions decompiled: {}".format(len(all_funcs)))


main()
