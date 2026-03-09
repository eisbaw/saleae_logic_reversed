# TraceI2CTunnel.py -- Ghidra script for tracing I2C tunnel protocol
#
# Analyzes the I2C/EEPROM tunnel protocol in libgraph_server_shared.so.
# Finds code that constructs I2C commands (opcodes 0x55, 0x87, 0x88),
# traces the data flow from EEPROM operations through EP1 bulk transfers,
# and identifies I2C bus devices and their addresses.
#
# Usage:
#   Run in Ghidra Script Manager on the libgraph_server_shared.so project.
#   Output goes to the Ghidra console.
#
# Related analysis: re/notes/i2c-eeprom-protocol.md

from ghidra.program.model.listing import CodeUnit
from ghidra.program.model.symbol import SourceType
from ghidra.program.model.address import AddressSet
from ghidra.app.decompiler import DecompInterface, DecompileOptions
from ghidra.util.task import ConsoleTaskMonitor
import re


def get_address(offset):
    """Convert a numeric offset to an Address object."""
    return currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(offset)


def find_string_at(addr):
    """Read a null-terminated ASCII string at the given address."""
    mem = currentProgram.getMemory()
    result = []
    try:
        for i in range(200):
            b = mem.getByte(addr.add(i))
            if b == 0:
                break
            if 0x20 <= (b & 0xFF) <= 0x7E:
                result.append(chr(b & 0xFF))
            else:
                break
    except:
        pass
    return ''.join(result)


def find_references_to_string(string_addr):
    """Find all code locations that reference a given string address via LEA."""
    refs = []
    ref_mgr = currentProgram.getReferenceManager()
    for ref in ref_mgr.getReferencesTo(string_addr):
        from_addr = ref.getFromAddress()
        refs.append(from_addr)
    return refs


def search_for_strings(patterns):
    """Search .rodata for strings matching the given regex patterns."""
    mem = currentProgram.getMemory()
    results = {}

    # Get .rodata block
    rodata = None
    for block in mem.getBlocks():
        if block.getName() == '.rodata':
            rodata = block
            break

    if rodata is None:
        print("WARNING: .rodata section not found")
        return results

    listing = currentProgram.getListing()
    data_iter = listing.getDefinedData(rodata.getStart(), True)

    while data_iter.hasNext():
        data = data_iter.next()
        if data.getAddress().compareTo(rodata.getEnd()) > 0:
            break
        if not data.hasStringValue():
            continue
        try:
            s = data.getValue()
            if s is None:
                continue
            for pattern in patterns:
                if re.search(pattern, s, re.IGNORECASE):
                    addr = data.getAddress()
                    results[str(addr)] = s
                    break
        except:
            continue

    return results


def get_function_at(addr):
    """Get the function containing the given address."""
    fm = currentProgram.getFunctionManager()
    return fm.getFunctionContaining(addr)


def decompile_function(func):
    """Decompile a function and return the C code."""
    if func is None:
        return None
    monitor = ConsoleTaskMonitor()
    decomp = DecompInterface()
    decomp.openProgram(currentProgram)
    try:
        result = decomp.decompileFunction(func, 30, monitor)
        if result and result.decompileCompleted():
            return result.getDecompiledFunction().getC()
    except:
        pass
    finally:
        decomp.dispose()
    return None


def analyze_i2c_function(func, label):
    """Analyze a function that performs I2C operations."""
    if func is None:
        return

    print("\n" + "=" * 70)
    print("Function: {} at {}".format(label, func.getEntryPoint()))
    print("  Name: {}".format(func.getName()))
    print("  Size: {} bytes".format(func.getBody().getNumAddresses()))
    print("=" * 70)

    # Decompile
    c_code = decompile_function(func)
    if c_code:
        # Look for interesting patterns in decompiled code
        lines = c_code.split('\n')
        for i, line in enumerate(lines):
            # Command opcode stores
            if '0x55' in line or '0x87' in line or '0x88' in line or '0x89' in line:
                print("  [OPCODE] Line {}: {}".format(i, line.strip()))
            # I2C address references
            if '0x50' in line or '0xa0' in line or '0xA0' in line:
                print("  [I2C_ADDR] Line {}: {}".format(i, line.strip()))
            # Size constants
            if '0x100' in line or '256' in line or '0x2000' in line or '8192' in line:
                print("  [SIZE] Line {}: {}".format(i, line.strip()))
            # Page size
            if '0x20' in line and 'page' in line.lower():
                print("  [PAGE] Line {}: {}".format(i, line.strip()))
            if '0x40' in line and 'page' in line.lower():
                print("  [PAGE] Line {}: {}".format(i, line.strip()))
            # vtable calls (EP1 write/read)
            if 'vtable' in line.lower() or '0x18' in line or '0x20' in line or '0x28' in line:
                if 'call' in line.lower() or '->' in line:
                    print("  [VTABLE] Line {}: {}".format(i, line.strip()))
            # Error handling
            if 'error' in line.lower() or 'fail' in line.lower():
                print("  [ERROR] Line {}: {}".format(i, line.strip()))

        # Print full decompiled code (truncated)
        print("\n  --- Decompiled C (first 100 lines) ---")
        for line in lines[:100]:
            print("  " + line)
        if len(lines) > 100:
            print("  ... ({} more lines)".format(len(lines) - 100))
    else:
        print("  (decompilation failed)")


def main():
    print("=" * 70)
    print("TraceI2CTunnel.py -- I2C/EEPROM Tunnel Protocol Tracer")
    print("Binary: libgraph_server_shared.so (Saleae Logic 2 v2.4.40)")
    print("=" * 70)

    # Key I2C/EEPROM-related strings to search for
    i2c_patterns = [
        r'ReadI2c',
        r'WriteI2C',
        r'WriteEeprom',
        r'ReadEeprom',
        r'TestEeprom',
        r'WipeEeprom',
        r'I2c tunnel',
        r'TransferI2c',
        r'Tunnel.*size.*large',
        r'eeprom.*only',
        r'EepromSize',
        r'EepromPage',
        r'CypressI2c',
        r'i2c.*wake',
        r'SerializeI2c',
        r'EEPROM.*Reading',
        r'EEPROM.*Writing',
        r'eeprom_at24c64d',
        r'CryptoInterface',
        r'I2C_RAIL',
        r'DeviceID',
        r'DEVICEID',
    ]

    print("\n[1] Searching for I2C/EEPROM-related strings...")
    strings_found = search_for_strings(i2c_patterns)
    print("  Found {} matching strings".format(len(strings_found)))

    for addr_str, s in sorted(strings_found.items()):
        print("  {} : {}".format(addr_str, s[:80]))

    # Key known string addresses (from static analysis)
    # These may need adjustment if the binary is rebased
    key_strings = {
        'WriteI2C request': 0x35c862,
        'ReadI2c request': 0x3d8aa7,
        'ReadI2c response': 0x34ee68,
        'WriteI2C response': 0x4123f4,
        'Fx3 WriteEeprom': 0x382a9c,
        'Fx3 ReadEeprom': 0x3be2e5,
        'Fx2 WriteEeprom': 0x3ee931,
        'Fx2 ReadEeprom': 0x3d1dd6,
        'TestEeprom': 0x389977,
        'I2c tunnel error': 0x38ff08,
        'TransferI2c': 0x3edcfc,
        'eeprom_at24c64d.cpp': 0x3691f4,
        'Eeprom_AT24C64D Write': 0x409f57,
    }

    print("\n[2] Finding code references to key I2C strings...")
    functions_to_analyze = {}

    for label, offset in key_strings.items():
        addr = get_address(offset)
        refs = find_references_to_string(addr)
        if refs:
            print("  {} ({}) -> {} references".format(label, addr, len(refs)))
            for ref in refs[:3]:
                func = get_function_at(ref)
                if func:
                    func_name = func.getName()
                    print("    {} in {}()".format(ref, func_name))
                    key = str(func.getEntryPoint())
                    if key not in functions_to_analyze:
                        functions_to_analyze[key] = (func, label)
                else:
                    print("    {} (no function)".format(ref))
        else:
            print("  {} ({}) -> no references found".format(label, addr))

    print("\n[3] Decompiling I2C-related functions...")
    for key in sorted(functions_to_analyze.keys()):
        func, label = functions_to_analyze[key]
        analyze_i2c_function(func, label)

    # Additional: search for command opcode construction sites
    print("\n" + "=" * 70)
    print("[4] Searching for EP1 command opcode construction (0x55, 0x87, 0x88)...")
    print("=" * 70)

    listing = currentProgram.getListing()
    mem = currentProgram.getMemory()
    text_block = None
    for block in mem.getBlocks():
        if block.getName() == '.text':
            text_block = block
            break

    if text_block:
        # Search for MOV byte [reg], 0x55/0x87/0x88 patterns in .text
        opcodes_of_interest = {0x55: 'WriteEeprom', 0x87: 'WriteI2C', 0x88: 'ReadI2c'}

        inst_iter = listing.getInstructions(text_block.getStart(), True)
        opcode_sites = []
        count = 0

        while inst_iter.hasNext() and count < 5000000:
            inst = inst_iter.next()
            count += 1
            if inst.getAddress().compareTo(text_block.getEnd()) > 0:
                break

            mnemonic = inst.getMnemonicString()
            if mnemonic != 'MOV':
                continue

            # Check if this is a byte move with an opcode value
            num_ops = inst.getNumOperands()
            if num_ops < 2:
                continue

            try:
                # Check second operand for our opcode values
                scalar = inst.getScalar(1)
                if scalar is not None:
                    val = scalar.getUnsignedValue()
                    if val in opcodes_of_interest:
                        # Check if first operand is a byte store
                        op0_str = inst.getDefaultOperandRepresentation(0)
                        if 'BYTE' in str(inst) or 'byte' in str(inst).lower():
                            func = get_function_at(inst.getAddress())
                            func_name = func.getName() if func else "(unknown)"
                            opcode_sites.append((inst.getAddress(), val, func_name))
                            print("  {} : MOV byte ..., 0x{:02x} ({}) in {}()".format(
                                inst.getAddress(), val, opcodes_of_interest[val], func_name))
            except:
                continue

        print("  Found {} opcode construction sites".format(len(opcode_sites)))

    # Summary
    print("\n" + "=" * 70)
    print("[5] SUMMARY")
    print("=" * 70)
    print("""
I2C/EEPROM Protocol Architecture:

1. FX2 devices (Logic, Logic 16):
   - I2C via vendor control transfers on EP0
   - Fx2DeviceFeatures::ReadEeprom / WriteEeprom
   - EEPROM: 256 bytes, 64-byte pages
   - i2c_err_t error type

2. FX3 devices (Logic 4/8, Logic Pro 8/16):
   - I2C tunneled via EP1 command protocol
   - Opcode 0x55 = WriteEeprom
   - Opcode 0x87 = WriteI2C (raw)
   - Opcode 0x88 = ReadI2c
   - Fx3DeviceFeatures::ReadEeprom / WriteEeprom
   - EEPROM size varies by device (EepromSizeMax virtual)

3. MSO devices:
   - I2C tunneled via MSO-specific interface
   - AT24C64D EEPROM: 8192 bytes, 32-byte pages
   - Eeprom_AT24C64D class
   - Used for AFE calibration metadata

I2C Bus Devices:
   - EEPROM (device ID, metadata)
   - Crypto IC (ATECC508A/608A, device authentication)
   - I2C controlled via GPIO (I2C_RAIL_EN1, I2C_RAIL_EN2)

Calibration Path:
   - Logic 4/8/Pro: Device ID read from EEPROM -> calibration
     downloaded from Saleae server by JS frontend
   - MSO: Calibration stored on-device in AT24C64D EEPROM
""")


if __name__ == '__main__':
    main()
