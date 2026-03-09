---
id: TASK-0008
title: RE FPGA bitstream download protocol
status: Done
assignee: []
created_date: '2026-03-08 22:43'
updated_date: '2026-03-09 00:21'
labels: []
dependencies:
  - TASK-0006
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer DownloadBitstream, LoadBitstream, ProgramFpgaBitstream, GetFpgaBitstreamVersion functions.

Known facts:
- 10 distinct bitstream CRC values embedded (0x25C9, 0x36E3, 0x6391, 0x74EF, 0x80F3, 0xAC6D, 0xCE24, 0xD8EC, 0xF147, 0xF4ED)
- Bitstream Status: "Final Version 10.27"
- References to Altera, Xilinx, and Lattice FPGA vendors
- "Logic16 loading {} voltage bitstream" -- voltage-specific bitstreams
- "Lattice PLL and SERDES reset complete" -- Lattice-specific init sequence
- "Lattice Semiconductor Corporation Bitstream" header string
- "Programming FPGA bitstream, {} bytes" -- size logging
- Separate constants: MaxSamplesPerPacket_Altera vs _Xilinx

Investigate:
1. How does the host select which bitstream to download?
2. Is the bitstream embedded in the .so or loaded from a file?
3. What USB transfer mechanism is used (bulk? control?)?
4. How does the host detect FPGA type/revision?
5. What is the programming sequence per FPGA vendor?
6. How is programming success verified?

Bitstream upload is to volatile FPGA config -- safe on power cycle.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Key Findings

### 21 Embedded FPGA Bitstreams Found

All bitstreams are embedded directly in libgraph_server_shared.so:
- 11 Xilinx (2 x 7-series ~4.5MB, 6 x Spartan-6 SLX9 ~333KB, 3 x SLX16 ~453KB)
- 10 Lattice ECP5 (headers fully parsed with part names, dates, CRCs)

### All 10 CRC Values Mapped

| CRC    | Part                    | Design              |
|--------|-------------------------|----------------------|
| 0xD8EC | LFE5UM-25F (ECP5UM)     | Logic8               |
| 0xCE24 | LFE5UM-25F (ECP5UM)     | Logic_Pro            |
| 0x36E3 | LFE5UM-25F (ECP5UM)     | Logic_Pro16          |
| 0xF147 | LFE5UM5G-45F (ECP5UM5G) | Logic8               |
| 0x25C9 | LFE5UM5G-45F (ECP5UM5G) | Logic_Pro            |
| 0x74EF | LFE5UM5G-45F (ECP5UM5G) | Logic_Pro16          |
| 0xF4ED | LFE5U-45F (ECP5U)       | Logic8               |
| 0x6391 | LFE5U-45F (ECP5U)       | Logic_Pro16          |
| 0x80F3 | LFE5U-12F (ECP5U)       | Logic8               |
| 0xAC6D | LFE5U-12F (ECP5U)       | Logic_Pro            |

### FPGA Families (corrected from PRD)

Current-gen devices use Lattice ECP5, NOT Xilinx Spartan-6:
- Lattice ECP5UM (LFE5UM-25F) - older HW revision
- Lattice ECP5UM5G (LFE5UM5G-45F) - mid HW revision  
- Lattice ECP5U (LFE5U-12F/45F) - newest HW revision
- Xilinx Spartan-6 (SLX9/SLX16) - legacy devices only
- Xilinx 7-series (unknown part) - 2 bitstreams, early/prototype?
- No Altera/Intel bitstreams found despite string references

### Programming Protocol

- Bitstream sent via WriteConfigurationData in chunks (62 or 509 bytes)
- FPGA verified by reading back 0xAA from a register
- Post-programming: PLL/SERDES/JESD204B link establishment
- CryptoInterface (ATECC-style) provides device authentication
- SALEAE_CHUNKED_FPGA_PROGRAMMING env var for chunked transfer mode

### ADC Integration

Design names all reference ADS528x (TI ADS5282/ADS5287), confirming
JESD204B high-speed serial ADC interface on ECP5 devices.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Completed RE analysis of FPGA bitstream download protocol. Found 21 embedded bitstreams (11 Xilinx, 10 Lattice ECP5) with all 10 CRC values mapped to specific FPGA parts and device types. Current-gen devices use Lattice ECP5 family (not Spartan-6 as originally assumed). Programming uses chunked USB bulk/control transfers through FX3. Created Ghidra script TraceBitstreamDownload.py and comprehensive documentation in re/notes/ and doc/protocol/.
<!-- SECTION:FINAL_SUMMARY:END -->
