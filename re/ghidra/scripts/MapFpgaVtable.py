# MapFpgaVtable.py -- Ghidra script to label FpgaDeviceFeatures and
# ProFpgaDeviceFeatures vtable entries and associated functions.
#
# This script applies labels discovered through binary analysis of
# libgraph_server_shared.so v2.4.40 (TASK-0045).
#
# Vtable layout (8 entries):
#   [0] ~FpgaDeviceFeatures() [D1 complete destructor]
#   [1] ~FpgaDeviceFeatures() [D0 deleting destructor]  -- Pro overrides
#   [2] WriteRegister(U8 addr, U8 val)
#   [3] WriteRegisters(vector<pair<U8,U8>>&, const S8*)  -- Pro overrides
#   [4] ReadRegister(U8 addr, const S8*) -> U8
#   [5] ReadRegisters(vector<U8>&, const S8*) -> vector<U8>
#   [6] DownloadBitstream(const U8*, U64, callback, double)
#   [7] ResetFpga()
#
# Non-virtual member functions:
#   WriteRegisterBit(U8 reg, U8 bit, U8 val, const S8*)
#   WriteConfigurationData(const U8*, U32)
#
# EP1 command opcodes used:
#   0x80 = WriteRegisters (batch write, previously undocumented)
#   0x81 = ReadRegister / ReadRegisters
#   0x7E = ResetFpga
#   0x7F = WriteConfigurationData
#
# Usage in Ghidra:
#   Script Manager -> Run -> MapFpgaVtable.py
#
# @author  saleae_reverse project
# @category DeviceFeatures
# @keybinding
# @menupath
# @toolbar

from ghidra.program.model.symbol import SourceType

def label(addr_int, name):
    """Apply a label at the given address."""
    addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(addr_int)
    sym = currentProgram.getSymbolTable().getPrimarySymbol(addr)
    if sym and sym.getName() == name:
        print("  Already labeled: {} at 0x{:08x}".format(name, addr_int))
        return
    currentProgram.getSymbolTable().createLabel(addr, name, SourceType.ANALYSIS)
    print("  Labeled: {} at 0x{:08x}".format(name, addr_int))

def comment(addr_int, text):
    """Set a plate comment at the given address."""
    addr = currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(addr_int)
    listing = currentProgram.getListing()
    cu = listing.getCodeUnitAt(addr)
    if cu:
        cu.setComment(cu.PLATE_COMMENT, text)

print("=== MapFpgaVtable: Labeling FpgaDeviceFeatures vtable entries ===")
print()

# --- FpgaDeviceFeatures vtable ---
FPGA_VTABLE = 0x2e48fb8
FPGA_TYPEINFO = 0x2e49088

print("FpgaDeviceFeatures vtable at 0x{:08x}".format(FPGA_VTABLE))
label(FPGA_VTABLE, "FpgaDeviceFeatures::vtable")
label(FPGA_TYPEINFO, "FpgaDeviceFeatures::typeinfo")
label(FPGA_VTABLE - 8, "FpgaDeviceFeatures::vtable::typeinfo_ptr")

# Virtual method implementations (base class)
fpga_methods = {
    0x1c54e30: ("FpgaDeviceFeatures::~D1", "Complete (non-deleting) destructor"),
    0x1c53520: ("FpgaDeviceFeatures::~D0", "Deleting destructor"),
    0x1c53570: ("FpgaDeviceFeatures::WriteRegister",
                "virtual void WriteRegister(U8 addr, U8 val)\n"
                "Wrapper: packs (addr,val) pair, calls WriteRegisters"),
    0x1c535f0: ("FpgaDeviceFeatures::WriteRegisters",
                "virtual void WriteRegisters(vector<pair<U8,U8>>&, const S8*)\n"
                "EP1 opcode 0x80. Max 31 registers per batch.\n"
                "Fire-and-forget (no response check)."),
    0x1c53990: ("FpgaDeviceFeatures::ReadRegister",
                "virtual U8 ReadRegister(U8 addr, const S8*)\n"
                "EP1 opcode 0x81, count=1. Returns 1 byte."),
    0x1c53af0: ("FpgaDeviceFeatures::ReadRegisters",
                "virtual vector<U8> ReadRegisters(vector<U8>&, const S8*)\n"
                "EP1 opcode 0x81, batch read. Max 63 registers."),
    0x1c53f20: ("FpgaDeviceFeatures::DownloadBitstream",
                "virtual void DownloadBitstream(const U8*, U64, callback, double)\n"
                "Calls ResetFpga, then loops WriteConfigurationData chunks.\n"
                "Chunk size: 62 (USB2.0) or 508+ (USB3.0)."),
    0x1c54740: ("FpgaDeviceFeatures::ResetFpga",
                "virtual void ResetFpga()\n"
                "EP1 opcode 0x7E. Expects 0x00 response."),
}

for addr, (name, desc) in sorted(fpga_methods.items()):
    label(addr, name)
    comment(addr, desc)

print()

# --- ProFpgaDeviceFeatures vtable ---
PRO_VTABLE = 0x2e49008
PRO_TYPEINFO = 0x2e49098

print("ProFpgaDeviceFeatures vtable at 0x{:08x}".format(PRO_VTABLE))
label(PRO_VTABLE, "ProFpgaDeviceFeatures::vtable")
label(PRO_TYPEINFO, "ProFpgaDeviceFeatures::typeinfo")
label(PRO_VTABLE - 8, "ProFpgaDeviceFeatures::vtable::typeinfo_ptr")

# Pro overrides (only slots [1] and [3])
pro_methods = {
    0x1c54f80: ("ProFpgaDeviceFeatures::~D0",
                "Deleting destructor (semantically identical to base)"),
    0x1c549d0: ("ProFpgaDeviceFeatures::WriteRegisters",
                "virtual void WriteRegisters(vector<pair<U8,U8>>&, const S8*)\n"
                "EP1 opcode 0x80. Same format as base.\n"
                "Adds response verification: reads back confirmation byte\n"
                "after EP1 write ('write register response check')."),
}

for addr, (name, desc) in sorted(pro_methods.items()):
    label(addr, name)
    comment(addr, desc)

print()

# --- Non-virtual member functions ---
print("Non-virtual member functions:")
nonvirt = {
    0x1c53e30: ("FpgaDeviceFeatures::WriteRegisterBit",
                "void WriteRegisterBit(U8 reg, U8 bit, U8 val, const S8*)\n"
                "Read-modify-write helper. Not virtual."),
    0x1c544b0: ("FpgaDeviceFeatures::WriteConfigurationData",
                "void WriteConfigurationData(const U8*, U32)\n"
                "EP1 opcode 0x7F. USB2.0 max 62 bytes, USB3.0 max 509.\n"
                "Called by DownloadBitstream. Not virtual."),
    0x1c53bc0: ("FpgaDeviceFeatures::ReadRegisters_impl",
                "Internal helper for ReadRegisters.\n"
                "Performs assertion checking and buffer management."),
}

for addr, (name, desc) in sorted(nonvirt.items()):
    label(addr, name)
    comment(addr, desc)

print()

# --- RTTI name strings ---
print("RTTI name strings:")
label(0x47496e, "rtti_name_FpgaDeviceFeatures")
label(0x474983, "rtti_name_ProFpgaDeviceFeatures")

# --- Fx2/Fx3 DeviceFeatures vtables (for reference) ---
print()
print("Related vtables (Fx2/Fx3 DeviceFeatures):")
label(0x2e48ec8, "Fx2DeviceFeatures::vtable")
label(0x2e48ec0, "Fx2DeviceFeatures::vtable::typeinfo_ptr")
label(0x2e49060, "Fx2DeviceFeatures::typeinfo")

label(0x2e48f40, "Fx3DeviceFeatures::vtable")
label(0x2e48f38, "Fx3DeviceFeatures::vtable::typeinfo_ptr")
label(0x2e49070, "Fx3DeviceFeatures::typeinfo")

print()
print("=== MapFpgaVtable complete ===")
print()
print("Summary:")
print("  FpgaDeviceFeatures:    8 vtable entries labeled")
print("  ProFpgaDeviceFeatures: 2 override entries labeled")
print("  Non-virtual methods:   3 functions labeled")
print("  RTTI strings:          2 entries labeled")
print("  Related vtables:       2 (Fx2, Fx3) labeled")
