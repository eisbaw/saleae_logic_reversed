# IDCODE 0x04AC2093 Identification

## Result

**IDCODE 0x04AC2093 = Xilinx XCAU15P (Artix UltraScale+)**

This is NOT a 7-series part. The task title is misleading -- the IDCODE belongs to
the Artix UltraScale+ family, which uses 7-series-compatible configuration protocol
but is a distinct 16nm FinFET+ product line.

## IDCODE Decode

```
0x04AC2093 = 0000_0100_1010_1100_0010_0000_1001_0011

  version      = 0x0     (bits [31:28])
  part number  = 0x4AC2  (bits [27:12])
  manufacturer = 0x049   (bits [11:1]) = Xilinx/AMD
  LSB          = 1       (bit [0], always 1 per IEEE 1149.1)
```

Part code 0x4AC2 falls well outside the 7-series range (0x3620-0x37C8) and
matches the Artix UltraScale+ allocation.

## Source

Identified via the openFPGALoader device database (`src/part.hpp`):

```c
{0x04AC4033, {"xilinx", "artixusp", "xcau10p", 6}},
{0x04AC4093, {"xilinx", "artixusp", "xcau10p", 6}},
{0x04AC2093, {"xilinx", "artixusp", "xcau15p", 6}},  // <-- this one
{0x04A65093, {"xilinx", "artixusp", "xcau20p", 6}},
{0x04A64093, {"xilinx", "artixusp", "xcau25p", 6}},
```

Not documented in Xilinx UG470 (which covers 7-series only). The XCAU15P
IDCODE appears in UG570 (UltraScale Configuration User Guide).

## XCAU15P Key Specs

- Family: Artix UltraScale+
- Process: 16nm FinFET+
- Logic cells: ~170K
- CLB LUTs: ~78K
- CLB Flip-Flops: ~156K
- GTH transceivers: 4x at up to 12.5 Gb/s
- Low power, cost-optimized

## Context in Saleae Binary

Two XCAU15P bitstreams are embedded in `libgraph_server_shared.so`:

| Index | Offset     | Size     | IDCODE      | Part    |
|-------|------------|----------|-------------|---------|
| 0     | 0x007D9010 | 4,542 KB | 0x04AC2093 | XCAU15P |
| 1     | 0x00C48A20 | 4,812 KB | 0x04AC2093 | XCAU15P |

The remaining 9 embedded bitstreams are Spartan-6 (XC6SLX4 and XC6SLX9).

Two different XCAU15P bitstreams likely correspond to different hardware
revisions or capture configurations. The binary references
`FPGA HW Revision: {}.{}` and multiple revision-dependent code paths.

## Implications

1. The PRD states all FX3-based devices use "Xilinx Spartan" -- this is only
   true for older hardware revisions. Newer Logic Pro 8/16 revisions use the
   XCAU15P (Artix UltraScale+).

2. The `FPGA JESD lock=...` strings in the binary confirm JESD204B usage,
   which requires the GTH transceivers available on the XCAU15P but absent
   from Spartan-6. This strongly suggests the XCAU15P bitstreams are for
   Logic Pro devices (which have analog ADCs connected via JESD204B).

3. The Spartan-6 bitstreams (XC6SLX4 for Logic 4/8, XC6SLX9 for Logic Pro
   8/16 older revisions) use simpler parallel ADC interfaces, not JESD.

## Complete Embedded Bitstream Map

| Index | Offset     | Family     | IDCODE      | Part       | Size     |
|-------|------------|------------|-------------|------------|----------|
| 0     | 0x007D9010 | USP        | 0x04AC2093 | XCAU15P    | 4,543 KB |
| 1     | 0x00C48A20 | USP        | 0x04AC2093 | XCAU15P    | 4,812 KB |
| 2     | 0x010FB860 | Spartan-6  | 0x04001093 | XC6SLX4    | 333 KB   |
| 3     | 0x0114EC00 | Spartan-6  | 0x04001093 | XC6SLX4    | 333 KB   |
| 4     | 0x011A1E80 | Spartan-6  | 0x04001093 | XC6SLX4    | 333 KB   |
| 5     | 0x011F5100 | Spartan-6  | 0x04001093 | XC6SLX4    | 333 KB   |
| 6     | 0x01248380 | Spartan-6  | 0x04002093 | XC6SLX9    | 453 KB   |
| 7     | 0x012B98D0 | Spartan-6  | 0x04002093 | XC6SLX9    | 453 KB   |
| 8     | 0x0132AE20 | Spartan-6  | 0x04001093 | XC6SLX4    | 333 KB   |
| 9     | 0x0137E0A0 | Spartan-6  | 0x04001093 | XC6SLX4    | 333 KB   |
| 10    | 0x013D1320 | Spartan-6  | 0x04002093 | XC6SLX9    | ~27 MB*  |

*Bitstream [10] size is estimated as distance to EOF; actual bitstream is
likely ~453 KB with the remainder being other embedded data or padding.
