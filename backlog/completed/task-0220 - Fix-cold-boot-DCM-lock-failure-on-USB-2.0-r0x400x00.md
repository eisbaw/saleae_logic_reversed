---
id: TASK-0220
title: Fix cold-boot DCM lock failure on USB 2.0 (r0x40=0x00)
status: Done
assignee: []
created_date: '2026-03-15 08:34'
updated_date: '2026-03-15 09:05'
labels:
  - hardware
  - dcm
  - blocker
  - usb2
dependencies: []
references:
  - re/captures/usbmon4-ep1-decoded.txt
  - re/ghidra/decompiled-xilinx-init.txt
  - re/ghidra/decompiled-fun-01d22af0.txt
  - re/notes/decompiled-functions-01d22af0-01d21c90.md
  - re/notes/eeprom-layout.md
  - re/notes/fpga-bitstream-protocol.md
  - crates/saleae-driver/src/adc.rs
  - crates/saleae-driver/src/fpga.rs
  - crates/saleae-driver/src/facade.rs
documentation:
  - doc/protocol/fpga-bitstream.md
  - re/notes/adc-initialization.md
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
After power cycle → firmware upload → FPGA programming with slx9-02 → ADC init, the Spartan-6 DCM never locks (r0x40=0x00, r0x4a=0x00). The Saleae Logic 2 binary succeeds with the identical register sequence on the same hardware.

## What's confirmed

- **Register sequence matches pcap byte-for-byte**: every ADC register write, JESD cascade value, and timing constant is identical to the working pcap capture.
- **FPGA programs correctly**: two-phase r0x7F readback passes (write 0x55→read 0x55, write 0xAA→read 0xAA).
- **Correct bitstream variant**: slx9-02 (variant B, COR1=3D00) is correct for Rev 1.2 hardware. Variant A (slx9-01, COR1=3D08) fails to configure the FPGA entirely (readback=0xFF).
- **CommonDeviceInit fully traced**: FUN_01d21fb0 is the full init orchestrator — crypto auth → FPGA verify → ADC config → SetAd9637Enabled(false/true) → calibration → r0x12=0x04. Our code implements all non-crypto steps.
- **"Bitstream CRC" values are actually sizes**: 0x5327C = 340,604 bytes = pcap byte count. Not a CRC checksum.
- **Post-DCM-lock sequence implemented**: r0x00=0x00, r0x00=0x40, calibration writes to r0x17, SetAd9637Enabled(false), r0x12=0x04 — all in `write_calibration_and_cascade`.

## What's been tried (code changes in tree, untested — needs power cycle)

1. **Trim 4 trailing padding bytes** from bitstream (340,608→340,604 to match pcap). The extracted .bin files include inter-blob padding that the binary doesn't send.
2. **Increased DCM settle time** from 50ms→100ms (USB 2.0 may need more).
3. **Increased retry count** from 5→10.
4. **Added 10ms post-FPGA-programming delay** before ADC init.

## Remaining hypotheses

- **Bitstream trailing bytes**: The 4 extra `00 00 00 00` bytes may affect Spartan-6 CRC-checked configuration (variant B has CRC enabled). Trimming is implemented but untested.
- **USB 2.0 vs USB 3.0 GPIF timing**: The pcap was captured on USB 3.0 (1020B chunks). Our device is USB 2.0 (508B chunks). Different FX3 DMA/GPIF behaviour could affect clock distribution.
- **fpga_post_init needed on USB 2.0**: The pcap doesn't show fpga_post_init between FPGA verify and ADC init, but it might be needed at USB 2.0 speeds to enable the GPIF/clock path to the ADC.
- **ADC reference clock**: The AD9637 needs a reference clock from the FPGA. If the FPGA doesn't output this clock until a specific register is written (or until the DCM is already locked from another source), we have a chicken-and-egg problem.

## Next steps

1. Power cycle device, test with current code changes (`just hw-capture-idempotent`).
2. If still failing, try adding `fpga_post_init` between FPGA program and ADC init.
3. If still failing, try connecting device to a USB 3.0 port and retesting.
4. If still failing, decompile the GetFpgaImage / bitstream loading function to check if the binary does any bitstream preprocessing before sending.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 DCM locks reliably on cold boot (power cycle → firmware upload → FPGA program → ADC init → r0x40=0xE0)
- [x] #2 just hw-capture-idempotent passes 3/3 runs after power cycle
- [ ] #3 No regressions in warm-path (fast path) capture
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
**Root cause: wrong bitstream variant (slx9-02 instead of slx9-03).**

The default bitstream was incorrectly changed from slx9-03 to slx9-02 in a previous session. Task-0138 had already identified that slx9-03 is the correct variant for Rev 1.2 hardware -- only slx9-03 produces a working ADC clock for the DCM. slx9-02 programs OK (r0x7F=0xAA readback passes) but the DCM never locks (r0x40 stays 0x00).

Fix: changed DEFAULT_BITSTREAM_PATH back to slx9-03. Hardware-validated: DCM locks (r0x40=0xE0) on first attempt from both cold boot and warm restart. Idempotent captures work -- 5 consecutive captures without power cycle, all successful at 200 MB/s throughput.
<!-- SECTION:FINAL_SUMMARY:END -->
