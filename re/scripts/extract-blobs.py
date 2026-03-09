#!/usr/bin/env python3
"""Extract embedded firmware and FPGA bitstream blobs from libgraph_server_shared.so.

Extracts all firmware images (FX3, FX2 Intel HEX) and FPGA bitstreams
(Xilinx 7-series, Xilinx Spartan-6, Lattice ECP5) embedded in the Saleae
Logic 2 v2.4.40 native library.

Output goes to firmware/ (gitignored). A manifest with SHA-256 hashes
is printed to stdout.

Usage:
    python3 re/scripts/extract-blobs.py [path-to-libgraph_server_shared.so]

If no path is given, uses $SALEAE_PATH/resources/linux-x64/libgraph_server_shared.so.
"""

import hashlib
import os
import struct
import sys
from pathlib import Path


# ── Blob Definitions ────────────────────────────────────────────────────────

# Each blob is defined as (name, offset, size, category, description).
# Offsets and sizes are determined from signature scanning and format parsing.

BLOBS = []


def define_blobs():
    """Build the blob table from known offsets.

    Must be called after the binary is loaded so we can compute sizes
    for blobs whose end depends on parsing (Intel HEX, Lattice postamble).
    """
    global BLOBS

    # ── FX3 Firmware (Cypress .img format) ──────────────────────────────

    BLOBS.append({
        "name": "fw-fx3-b2-wrapper.bin",
        "offset": 0x0079E78F,
        "size": 17,
        "category": "fx3-firmware",
        "description": "B2 wrapper header (SPI/I2C boot config, contains VID 0x21A9)",
    })
    BLOBS.append({
        "name": "fw-fx3-pro8.img",
        "offset": 0x0079E7A0,
        "size": 119824,
        "category": "fx3-firmware",
        "description": "FX3 firmware for Logic Pro 8 (21a9:1005), compiled 2022-10-25",
    })
    BLOBS.append({
        "name": "fw-fx3-pro16.img",
        "offset": 0x007BBBB0,
        "size": 119824,
        "category": "fx3-firmware",
        "description": "FX3 firmware for Logic Pro 16 (21a9:1006), compiled 2022-10-25",
    })

    # ── Intel HEX Firmware (FX2/FX2LP, 8051) ───────────────────────────

    BLOBS.append({
        "name": "fw-ihex-1.hex",
        "offset": 0x0037E0AC,
        "size": 16397,
        "category": "ihex-firmware",
        "description": "Intel HEX firmware image 1 (FX2, 5565 data bytes, addr 0x0000-0x15BD)",
    })
    BLOBS.append({
        "name": "fw-ihex-2.hex",
        "offset": 0x0039743D,
        "size": 25992,
        "category": "ihex-firmware",
        "description": "Intel HEX firmware image 2 (FX2, 10794 data bytes, addr 0x0000-0x2EDB)",
    })
    BLOBS.append({
        "name": "fw-ihex-3.hex",
        "offset": 0x003AC67E,
        "size": 15549,
        "category": "ihex-firmware",
        "description": "Intel HEX firmware image 3 (FX2, 5217 data bytes, addr 0x0000-0x1461)",
    })
    BLOBS.append({
        "name": "fw-ihex-4.hex",
        "offset": 0x003CB8ED,
        "size": 23507,
        "category": "ihex-firmware",
        "description": "Intel HEX firmware image 4 (FX2, 8152 data bytes, addr 0x0000-0x1FD8)",
    })

    # ── Xilinx 7-Series Bitstreams ──────────────────────────────────────
    # Raw .bin format: starts with bus width auto-detect (00 00 00 BB 11 22 00 44)
    # followed by FF padding and AA995566 sync word.
    # IDCODE: 0x04AC2093 (both)

    BLOBS.append({
        "name": "bitstream-xilinx-7series-0.bin",
        "offset": 0x007D9000,
        "size": 0x00C48A10 - 0x007D9000,  # 4651024
        "category": "xilinx-7series",
        "description": "Xilinx 7-series bitstream 0, IDCODE 0x04AC2093, ~4.4 MB",
    })
    BLOBS.append({
        "name": "bitstream-xilinx-7series-1.bin",
        "offset": 0x00C48A10,
        "size": 0x010FB850 - 0x00C48A10,  # 4927552
        "category": "xilinx-7series",
        "description": "Xilinx 7-series bitstream 1, IDCODE 0x04AC2093, ~4.7 MB",
    })

    # ── Xilinx Spartan-6 Bitstreams ─────────────────────────────────────
    # Raw .bin format: starts with FFFFFFFF preamble then AA995566 sync.
    # Two FPGA parts: XC6SLX9 (IDCODE 0x04001093) and XC6SLX16 (IDCODE 0x04002093).

    s6_preambles = [
        0x010FB850, 0x0114EBF0, 0x011A1E70, 0x011F50F0,
        0x01248370, 0x012B98C0, 0x0132AE10, 0x0137E090,
        0x013D1310,
    ]
    # Ends: each extends to the start of the next blob
    s6_next = s6_preambles[1:] + [0x01442860]  # first Lattice FF 00 prefix
    s6_parts = [
        "slx9", "slx9", "slx9", "slx9",
        "slx16", "slx16",
        "slx9", "slx9",
        "slx16",
    ]
    s6_notes = [
        "XC6SLX9 variant A (COR1=3D08)",
        "XC6SLX9 variant A",
        "XC6SLX9 variant B (COR1=3D00)",
        "XC6SLX9 variant B",
        "XC6SLX16",
        "XC6SLX16",
        "XC6SLX9 variant A",
        "XC6SLX9 variant B",
        "XC6SLX16",
    ]

    for i, (start, end) in enumerate(zip(s6_preambles, s6_next)):
        BLOBS.append({
            "name": f"bitstream-spartan6-{s6_parts[i]}-{i:02d}.bin",
            "offset": start,
            "size": end - start,
            "category": "xilinx-spartan6",
            "description": f"Xilinx Spartan-6 bitstream {i}, {s6_notes[i]}",
        })

    # ── Lattice ECP5 Bitstreams ─────────────────────────────────────────
    # .bit format: starts with FF 00 preamble then ASCII header text
    # "Lattice Semiconductor Corporation Bitstream", followed by metadata
    # fields and then binary config data. Ends with 88 88 5E postamble
    # and FFFFFFFF.

    lattice_true_starts = [
        0x01442860, 0x01477B00, 0x014B5E40, 0x0150D540,
        0x01552D20, 0x015A2AF0, 0x01610750, 0x01656C90,
        0x016B5570, 0x016EA240,
    ]
    lattice_next = lattice_true_starts[1:] + [0x017298B3]  # L[9] ends at postamble
    lattice_designs = [
        "Logic8_Top_ADS528x",
        "Logic_Pro_Top_ADS528x",
        "Logic_Pro16_Top_ADS528x",
        "Logic8_Top_ADS528x",
        "Logic_Pro_Top_ADS528x",
        "Logic_Pro16_Top_ADS528x",
        "Logic8_Top_ADS528x",
        "Logic_Pro16_Top_ADS528x",
        "Logic8_Top_ADS528x",
        "Logic_Pro_Top_ADS528x",
    ]
    lattice_parts = [
        "lfe5um-25f", "lfe5um-25f", "lfe5um-25f",
        "lfe5um5g-45f", "lfe5um5g-45f", "lfe5um5g-45f",
        "lfe5u-45f", "lfe5u-45f",
        "lfe5u-12f", "lfe5u-12f",
    ]
    lattice_crcs = [
        "D8EC", "CE24", "36E3", "F147", "25C9",
        "74EF", "F4ED", "6391", "80F3", "AC6D",
    ]

    for i, (start, end) in enumerate(zip(lattice_true_starts, lattice_next)):
        BLOBS.append({
            "name": f"bitstream-ecp5-{lattice_parts[i]}-{i:02d}.bit",
            "offset": start,
            "size": end - start,
            "category": "lattice-ecp5",
            "description": (
                f"Lattice ECP5 bitstream {i}, {lattice_parts[i].upper()}, "
                f"design={lattice_designs[i]}, CRC=0x{lattice_crcs[i]}"
            ),
        })


# ── Validation ──────────────────────────────────────────────────────────────

def validate_fx3_firmware(blob_data: bytes, blob_info: dict) -> list[str]:
    """Validate a Cypress FX3 firmware image."""
    issues = []
    if len(blob_data) < 4:
        issues.append("Too small for CY header")
        return issues
    if blob_data[0:2] != b"CY":
        issues.append(f"Missing CY magic: got {blob_data[0:2].hex()}")
    else:
        image_type = blob_data[3]
        if image_type == 0xB0:
            pass  # normal firmware
        elif image_type == 0xB2:
            pass  # SPI/I2C boot config
        else:
            issues.append(f"Unexpected image_type: 0x{image_type:02X}")
    return issues


def validate_ihex(blob_data: bytes, blob_info: dict) -> list[str]:
    """Validate an Intel HEX firmware image."""
    issues = []
    try:
        text = blob_data.decode("ascii")
    except UnicodeDecodeError:
        issues.append("Not valid ASCII")
        return issues
    if not text.startswith(":"):
        issues.append("Does not start with ':'")
    if ":00000001FF" not in text:
        issues.append("Missing EOF record")
    # Count records
    records = [line for line in text.strip().split("\n") if line.startswith(":")]
    if len(records) < 10:
        issues.append(f"Only {len(records)} records (expected many more)")
    return issues


def validate_xilinx_7series(blob_data: bytes, blob_info: dict) -> list[str]:
    """Validate a Xilinx 7-series bitstream."""
    issues = []
    # Should contain bus width auto-detect: BB 11 22 00 44
    if b"\xBB\x11\x22\x00\x44" not in blob_data[:64]:
        issues.append("Missing bus width auto-detect pattern")
    # Should contain sync word: AA 99 55 66
    sync_pos = blob_data.find(b"\xAA\x99\x55\x66")
    if sync_pos == -1:
        issues.append("Missing sync word AA995566")
    else:
        # Read IDCODE from config packets after sync
        # IDCODE write is typically at sync+20 to sync+40
        pass
    if len(blob_data) < 1_000_000:
        issues.append(f"Suspiciously small: {len(blob_data)} bytes")
    return issues


def validate_xilinx_spartan6(blob_data: bytes, blob_info: dict) -> list[str]:
    """Validate a Xilinx Spartan-6 bitstream."""
    issues = []
    # Should start with FFFFFFFF preamble
    if blob_data[:4] != b"\xFF\xFF\xFF\xFF":
        issues.append(f"Missing FFFFFFFF preamble: got {blob_data[:4].hex()}")
    # Should contain sync word: AA 99 55 66
    sync_pos = blob_data.find(b"\xAA\x99\x55\x66")
    if sync_pos == -1:
        issues.append("Missing sync word AA995566")
    if len(blob_data) < 100_000:
        issues.append(f"Suspiciously small: {len(blob_data)} bytes")
    return issues


def validate_lattice_ecp5(blob_data: bytes, blob_info: dict) -> list[str]:
    """Validate a Lattice ECP5 bitstream."""
    issues = []
    # Should start with FF 00 then "Lattice Semiconductor"
    if blob_data[0:2] != b"\xFF\x00":
        issues.append(f"Missing FF 00 header prefix: got {blob_data[0:2].hex()}")
    if b"Lattice Semiconductor" not in blob_data[:100]:
        issues.append("Missing 'Lattice Semiconductor' header text")
    if b"Diamond" not in blob_data[:200]:
        issues.append("Missing 'Diamond' tool reference")
    if b"Bitstream CRC:" not in blob_data[:500]:
        issues.append("Missing Bitstream CRC field")
    if len(blob_data) < 50_000:
        issues.append(f"Suspiciously small: {len(blob_data)} bytes")
    return issues


VALIDATORS = {
    "fx3-firmware": validate_fx3_firmware,
    "ihex-firmware": validate_ihex,
    "xilinx-7series": validate_xilinx_7series,
    "xilinx-spartan6": validate_xilinx_spartan6,
    "lattice-ecp5": validate_lattice_ecp5,
}


# ── Lattice Header Parser ──────────────────────────────────────────────────

def parse_lattice_header(blob_data: bytes) -> dict:
    """Parse Lattice .bit file ASCII header and return metadata dict."""
    meta = {}
    # Skip FF 00 prefix
    pos = 2 if blob_data[:2] == b"\xFF\x00" else 0

    # Read null-terminated strings
    while pos < min(len(blob_data), 500):
        end = blob_data.find(b"\x00", pos, 500)
        if end == -1:
            break
        try:
            field = blob_data[pos:end].decode("ascii")
        except UnicodeDecodeError:
            break
        if not field or not any(c.isalpha() for c in field):
            pos = end + 1
            continue

        if field.startswith("Lattice Semiconductor"):
            meta["vendor"] = field
        elif field.startswith("Version:"):
            meta["tool_version"] = field.split(":", 1)[1].strip()
        elif field.startswith("Bitstream Status:"):
            meta["status"] = field.split(":", 1)[1].strip()
        elif field.startswith("Design name:"):
            meta["design"] = field.split(":", 1)[1].strip()
        elif field.startswith("Architecture:"):
            meta["architecture"] = field.split(":", 1)[1].strip()
        elif field.startswith("Part:"):
            meta["part"] = field.split(":", 1)[1].strip()
        elif field.startswith("Date:"):
            meta["date"] = field.split(":", 1)[1].strip()
        elif field.startswith("Rows:"):
            meta["rows"] = int(field.split(":", 1)[1].strip())
        elif field.startswith("Cols:"):
            meta["cols"] = int(field.split(":", 1)[1].strip())
        elif field.startswith("Bits:"):
            meta["bits"] = int(field.split(":", 1)[1].strip())
        elif field.startswith("Bitstream CRC:"):
            meta["crc"] = field.split(":", 1)[1].strip()
        elif field.startswith("Readback:"):
            meta["readback"] = field.split(":", 1)[1].strip()
        elif field.startswith("Security:"):
            meta["security"] = field.split(":", 1)[1].strip()

        pos = end + 1

    return meta


# ── Main ────────────────────────────────────────────────────────────────────

def main():
    # Resolve binary path
    if len(sys.argv) > 1:
        binary_path = Path(sys.argv[1])
    else:
        saleae_path = os.environ.get("SALEAE_PATH", "")
        if not saleae_path:
            print("ERROR: No binary path given and SALEAE_PATH not set.", file=sys.stderr)
            print("Usage: python3 re/scripts/extract-blobs.py [path-to-.so]", file=sys.stderr)
            sys.exit(1)
        binary_path = Path(saleae_path) / "resources" / "linux-x64" / "libgraph_server_shared.so"

    if not binary_path.exists():
        print(f"ERROR: Binary not found: {binary_path}", file=sys.stderr)
        sys.exit(1)

    # Find project root (directory containing re/ and firmware/)
    script_dir = Path(__file__).resolve().parent
    project_root = script_dir.parent.parent
    output_dir = project_root / "firmware"
    output_dir.mkdir(exist_ok=True)

    print(f"Binary: {binary_path}")
    print(f"Output: {output_dir}")
    print(f"Binary size: {binary_path.stat().st_size:,} bytes")
    print()

    # Load binary
    with open(binary_path, "rb") as f:
        data = f.read()

    # Build blob table
    define_blobs()

    # Extract and validate
    results = []
    errors = 0

    for blob in BLOBS:
        name = blob["name"]
        offset = blob["offset"]
        size = blob["size"]
        category = blob["category"]
        description = blob["description"]

        # Bounds check
        if offset + size > len(data):
            print(f"  ERROR: {name} extends past end of binary "
                  f"(offset=0x{offset:X}, size={size}, binary={len(data)})")
            errors += 1
            continue

        # Extract
        blob_data = data[offset:offset + size]

        # Compute SHA-256
        sha256 = hashlib.sha256(blob_data).hexdigest()

        # Validate
        validator = VALIDATORS.get(category)
        issues = validator(blob_data, blob) if validator else []

        # Write to disk
        out_path = output_dir / name
        with open(out_path, "wb") as f:
            f.write(blob_data)

        # Parse Lattice metadata if applicable
        lattice_meta = None
        if category == "lattice-ecp5":
            lattice_meta = parse_lattice_header(blob_data)

        status = "OK" if not issues else "WARN"
        if issues:
            errors += 1

        results.append({
            "name": name,
            "offset": offset,
            "size": size,
            "sha256": sha256,
            "category": category,
            "description": description,
            "issues": issues,
            "status": status,
            "lattice_meta": lattice_meta,
        })

        issue_str = f" [{'; '.join(issues)}]" if issues else ""
        print(f"  {status:4s} {name:45s} {size:>10,} bytes  "
              f"offset=0x{offset:08X}  sha256={sha256[:16]}...{issue_str}")

    # Summary
    print()
    print(f"{'='*80}")
    print(f"Extracted {len(results)} blobs to {output_dir}/")
    if errors:
        print(f"  {errors} blob(s) had validation warnings")
    else:
        print(f"  All blobs passed validation")

    # Print manifest in a table format suitable for documentation
    print()
    print("MANIFEST")
    print("=" * 120)
    print(f"{'Name':<50s} {'Offset':>12s} {'Size':>12s} {'SHA-256':<64s}")
    print("-" * 120)
    for r in results:
        print(f"{r['name']:<50s} 0x{r['offset']:08X}   {r['size']:>10,}  {r['sha256']}")
    print("-" * 120)

    # Print Lattice metadata summary
    print()
    print("LATTICE ECP5 METADATA")
    print("=" * 120)
    for r in results:
        if r.get("lattice_meta"):
            m = r["lattice_meta"]
            print(f"  {r['name']}")
            print(f"    Part: {m.get('part', '?')}")
            print(f"    Design: {m.get('design', '?')}")
            print(f"    Architecture: {m.get('architecture', '?')}")
            print(f"    Date: {m.get('date', '?')}")
            print(f"    Bits: {m.get('bits', '?')}")
            print(f"    CRC: {m.get('crc', '?')}")
            print(f"    Tool: {m.get('tool_version', '?')}")
            print(f"    Status: {m.get('status', '?')}")
            print()

    return 0 if errors == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
