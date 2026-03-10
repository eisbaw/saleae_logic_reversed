#!/usr/bin/env python3
"""
Extract vtable entries from a shared object by parsing ELF relocations.

The vtable entries in .data.rel.ro are zeroed in the file and filled by
R_X86_64_RELATIVE relocations at load time. Each relocation says:
    *(base + offset) = base + addend

Since the .so is not PIE, the addend IS the target VA.
"""
import struct
import sys

SO_PATH = "/nix/store/brcaw41j62pjki70ahf7ffd2f3bhaa3d-saleae-logic-2-extracted/resources/linux-x64/libgraph_server_shared.so"

# LogicGraduateDevice:
#   typeinfo VA = 0x2e48720 (from device-class-hierarchy.md)
#   vtable layout: [offset_to_top=0, typeinfo_ptr, slot0, slot1, ...]
#   The typeinfo pointer relocation is at 0x2e48690 (confirmed from readelf output)
#   So offset_to_top is at 0x2e48688, typeinfo_ptr at 0x2e48690
#   Virtual method slots start at 0x2e48698

VTABLE_START = 0x2e48698
NUM_SLOTS = 17  # from device-class-hierarchy.md: 17 vtable entries

# Also extract comparison vtables
CLASSES = {
    "LogicGraduateDevice": {"vtable_start": 0x2e48698, "num_slots": 17},
    # Logic2FpgaDevice (parent): typeinfo at 0x2e47d58
    # Need to find its vtable start
}


def parse_elf(path):
    with open(path, 'rb') as f:
        # ELF header
        f.seek(0)
        ident = f.read(16)
        assert ident[:4] == b'\x7fELF'

        f.seek(0x28)
        shoff = struct.unpack('<Q', f.read(8))[0]
        f.seek(0x3A)
        shentsize = struct.unpack('<H', f.read(2))[0]
        f.seek(0x3C)
        shnum = struct.unpack('<H', f.read(2))[0]
        f.seek(0x3E)
        shstrndx = struct.unpack('<H', f.read(2))[0]

        # Get shstrtab
        f.seek(shoff + shstrndx * shentsize)
        sh = f.read(shentsize)
        strtab_off = struct.unpack('<Q', sh[24:32])[0]

        # Find RELA sections and .data.rel.ro
        rela_sections = []
        data_rel_ro = None

        for i in range(shnum):
            f.seek(shoff + i * shentsize)
            sh = f.read(shentsize)
            name_off = struct.unpack('<I', sh[0:4])[0]
            sh_type = struct.unpack('<I', sh[4:8])[0]
            sh_addr = struct.unpack('<Q', sh[16:24])[0]
            sh_offset = struct.unpack('<Q', sh[24:32])[0]
            sh_size = struct.unpack('<Q', sh[32:40])[0]
            sh_entsize = struct.unpack('<Q', sh[56:64])[0]

            f.seek(strtab_off + name_off)
            name = b''
            while True:
                c = f.read(1)
                if c == b'\x00': break
                name += c
            name = name.decode()

            if sh_type == 4:  # SHT_RELA
                rela_sections.append((name, sh_offset, sh_size, sh_entsize))
            elif name == '.data.rel.ro':
                data_rel_ro = (sh_addr, sh_offset, sh_size)

        # Build relocation map: offset -> addend for R_X86_64_RELATIVE
        reloc_map = {}
        R_X86_64_RELATIVE = 8
        R_X86_64_64 = 1

        for sec_name, sec_off, sec_size, sec_entsize in rela_sections:
            if sec_entsize == 0:
                continue
            count = sec_size // sec_entsize
            f.seek(sec_off)
            for _ in range(count):
                entry = f.read(sec_entsize)
                if len(entry) < 24:
                    break
                r_offset = struct.unpack('<Q', entry[0:8])[0]
                r_info = struct.unpack('<Q', entry[8:16])[0]
                r_addend = struct.unpack('<q', entry[16:24])[0]  # signed
                r_type = r_info & 0xFFFFFFFF

                if r_type == R_X86_64_RELATIVE:
                    reloc_map[r_offset] = r_addend
                elif r_type == R_X86_64_64:
                    # External symbol + addend
                    reloc_map[r_offset] = f"extern+{r_addend}"

        return reloc_map, data_rel_ro


def main():
    print("Parsing ELF relocations (this may take a moment for 48MB binary)...")
    reloc_map, data_rel_ro = parse_elf(SO_PATH)
    print(f"Found {len(reloc_map)} relocations")

    if data_rel_ro:
        print(f".data.rel.ro: VA=0x{data_rel_ro[0]:x}, offset=0x{data_rel_ro[1]:x}, size=0x{data_rel_ro[2]:x}")

    # First, find all vtables by scanning for typeinfo references
    # The typeinfo VAs we know:
    typeinfos = {
        0x2e47d58: "Logic2FpgaDevice",
        0x2e48720: "LogicGraduateDevice",
        0x2e48b78: "LogicProfessionalDevice",
        0x2e48b90: "LogicProfessional16Device",
        0x2e48ba8: "LogicProfessional8Device",
        0x2e492a0: "LogicStudentDevice",
        0x2e49048: "LogicAnalyzerDevice",
    }

    # Find vtable locations by looking for offsets that point to typeinfo VAs
    print("\n=== Finding vtables by typeinfo reference ===")
    vtable_locations = {}
    for offset, addend in sorted(reloc_map.items()):
        if isinstance(addend, int) and addend in typeinfos:
            # This offset holds a pointer to the typeinfo
            # The vtable methods start at offset + 8
            cls = typeinfos[addend]
            vtable_methods_start = offset + 8
            vtable_offset_to_top = offset - 8
            print(f"\n{cls}:")
            print(f"  offset_to_top at: 0x{vtable_offset_to_top:x}")
            print(f"  typeinfo_ptr at:  0x{offset:x} -> 0x{addend:x}")
            print(f"  methods start at: 0x{vtable_methods_start:x}")
            vtable_locations[cls] = vtable_methods_start

    # Now extract LogicGraduateDevice vtable entries
    print("\n\n=== LogicGraduateDevice Vtable ===")
    if "LogicGraduateDevice" in vtable_locations:
        start = vtable_locations["LogicGraduateDevice"]
        for i in range(20):
            va = start + i * 8
            if va in reloc_map:
                target = reloc_map[va]
                if isinstance(target, int):
                    print(f"  slot[{i:2d}] @ 0x{va:x} -> 0x{target:x}")
                else:
                    print(f"  slot[{i:2d}] @ 0x{va:x} -> {target}")
            else:
                # Could be a zero entry (pure virtual) or end of vtable
                print(f"  slot[{i:2d}] @ 0x{va:x} -> (no relocation, likely end of vtable or 0)")
                # Check if next entry has a reloc to see if this is padding
                if (va + 8) not in reloc_map:
                    # Two consecutive missing entries likely means end of vtable
                    if i > 0:
                        break

    # Also extract Logic2FpgaDevice vtable for comparison
    print("\n\n=== Logic2FpgaDevice Vtable (parent class) ===")
    if "Logic2FpgaDevice" in vtable_locations:
        start = vtable_locations["Logic2FpgaDevice"]
        for i in range(20):
            va = start + i * 8
            if va in reloc_map:
                target = reloc_map[va]
                if isinstance(target, int):
                    print(f"  slot[{i:2d}] @ 0x{va:x} -> 0x{target:x}")
                else:
                    print(f"  slot[{i:2d}] @ 0x{va:x} -> {target}")
            else:
                if (va + 8) not in reloc_map:
                    if i > 0:
                        print(f"  (end of vtable at slot {i})")
                        break
                else:
                    print(f"  slot[{i:2d}] @ 0x{va:x} -> (no relocation)")

    # LogicProfessionalDevice for comparison
    print("\n\n=== LogicProfessionalDevice Vtable (sibling class) ===")
    if "LogicProfessionalDevice" in vtable_locations:
        start = vtable_locations["LogicProfessionalDevice"]
        for i in range(20):
            va = start + i * 8
            if va in reloc_map:
                target = reloc_map[va]
                if isinstance(target, int):
                    print(f"  slot[{i:2d}] @ 0x{va:x} -> 0x{target:x}")
                else:
                    print(f"  slot[{i:2d}] @ 0x{va:x} -> {target}")
            else:
                if (va + 8) not in reloc_map:
                    if i > 0:
                        print(f"  (end of vtable at slot {i})")
                        break
                else:
                    print(f"  slot[{i:2d}] @ 0x{va:x} -> (no relocation)")

    # LogicStudentDevice for comparison
    print("\n\n=== LogicStudentDevice Vtable (Logic 4, for comparison) ===")
    if "LogicStudentDevice" in vtable_locations:
        start = vtable_locations["LogicStudentDevice"]
        for i in range(20):
            va = start + i * 8
            if va in reloc_map:
                target = reloc_map[va]
                if isinstance(target, int):
                    print(f"  slot[{i:2d}] @ 0x{va:x} -> 0x{target:x}")
                else:
                    print(f"  slot[{i:2d}] @ 0x{va:x} -> {target}")
            else:
                if (va + 8) not in reloc_map:
                    if i > 0:
                        print(f"  (end of vtable at slot {i})")
                        break
                else:
                    print(f"  slot[{i:2d}] @ 0x{va:x} -> (no relocation)")

    # Summary: compare Graduate slots with parent and Professional
    print("\n\n=== Slot Comparison (Graduate vs Parent vs Professional) ===")
    graduate_start = vtable_locations.get("LogicGraduateDevice")
    parent_start = vtable_locations.get("Logic2FpgaDevice")
    professional_start = vtable_locations.get("LogicProfessionalDevice")

    if graduate_start and parent_start and professional_start:
        for i in range(17):
            g_va = graduate_start + i * 8
            p_va = parent_start + i * 8
            prof_va = professional_start + i * 8

            g_target = reloc_map.get(g_va, "N/A")
            p_target = reloc_map.get(p_va, "N/A")
            prof_target = reloc_map.get(prof_va, "N/A")

            shared = ""
            if g_target == prof_target and isinstance(g_target, int):
                shared = " [SHARED with Professional]"
            elif g_target == p_target and isinstance(g_target, int):
                shared = " [INHERITED from parent]"
            elif g_target != p_target and isinstance(g_target, int):
                shared = " [OVERRIDDEN]"

            if isinstance(g_target, int):
                print(f"  slot[{i:2d}]: Graduate=0x{g_target:x}{shared}")
            else:
                print(f"  slot[{i:2d}]: Graduate={g_target}{shared}")


if __name__ == "__main__":
    main()
