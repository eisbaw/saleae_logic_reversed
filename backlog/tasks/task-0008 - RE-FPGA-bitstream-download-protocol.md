---
id: TASK-0008
title: RE FPGA bitstream download protocol
status: To Do
assignee: []
created_date: '2026-03-08 22:43'
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
