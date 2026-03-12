---
id: TASK-0123
title: RE AD9637 register writes (WriteAd9637Register) for Logic Pro
status: Done
assignee: []
created_date: '2026-03-11 11:20'
updated_date: '2026-03-12 02:15'
labels:
  - re
  - ghidra
  - adc
  - analog
dependencies: []
references:
  - re/notes/adc-initialization.md
  - re/notes/init-sequence-pcap.md
  - crates/saleae-driver/src/jesd.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Logic2FpgaDevice::WriteAd9637Register(U16, U16) and SetAd9637Enabled(bool) configure the AD9637 ADC in Logic Pro 8/16. The pcap shows ADC register writes via indirect access (regs 0x03/0x04/0x05/0x16) but we haven't decompiled the function that generates those values.

Investigation:
1. Find strings referencing Ad9637 or WriteAd9637Register
2. Decompile WriteAd9637Register and SetAd9637Enabled
3. Map the indirect register write pattern to AD9637 SPI register addresses
4. Document: which AD9637 registers are written, what values, in what order
5. Compare against AD9637 datasheet (Analog Devices)
6. Cross-reference against pcap Phase 3 ADC init register writes

This is our primary target device (Logic Pro 8) so getting ADC init right is critical.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
## Completed: RE AD9637 register writes and fix adc.rs to match binary

### Decompiled functions
All three key functions were already decompiled in `re/ghidra/decompiled-xilinx-init.txt` and `re/ghidra/decompiled-fun-01d22af0.txt`:

1. **WriteAd9637Register** (FUN_01d208f0, ELF 0x1c208f0): Builds a 4-register batch write [r0x03=addr, r0x16=addr_hi, r0x04=value, r0x05=value_hi]. Our `jesd::write_adc_register` matches correctly.

2. **FUN_01d20d90** (ELF 0x1c20d90): ADC register configuration called from CommonDeviceInit with param_2=0x200000001, param_3=0. Sequence: reset(0x08=0x03), normal(0x08=0x00), sleep 10ms, then 7 config registers (0x0D=0x00, 0x15=0x21, 0x21=0xC2, 0x05=0x0F, 0x04=0x0F, 0x10=0x00, 0x18=0x04). Our `configure_ad9637` register values all match.

3. **SetAd9637Enabled** (FUN_01d23330, ELF 0x1c23330): Two paths:
   - false: SPI transfer (0x08=0x01) + ResetMainDcm
   - true: power normal, sleep 1ms, reset+standby, normal, sleep 50ms, ResetMainDcm, check DCM lock bits 5+6, check bitslip r0x4a bits 1+2

4. **ResetMainDcm** (FUN_01d22af0, ELF 0x1c22af0): Three AdcType paths:
   - AdcType==0 (Lattice): 5-step JESD cascade (0x1E->0x1C->0x18->0x08->0x00)
   - AdcType==1 (AD9637): r0x00=0x04, sleep 1ms, r0x00=0x00, poll r0x40 for (val&0xA0)==0xA0 with 20s timeout, then SERDES bitslip retry loop (max 9 retries)
   - AdcType==2: different path (not our device)

### Discrepancies found and fixed

1. **Wrong DCM reset sequence**: Was using 5-step Lattice JESD cascade for AD9637. Fixed to use correct 2-step sequence (r0x00=0x04, sleep, r0x00=0x00) with PLL poll timeout.

2. **Missing 1ms sleep in enable**: SetAd9637Enabled(true) has a 1ms sleep between power-normal and reset+standby. Our code did them back-to-back. Fixed.

3. **Wrong bitslip retry mechanism**: Was using ADC reset cycle + full JESD cascade for retries. Binary uses SERDES reset (r0x00=0x08, sleep, r0x00=0x00) with status reads. Fixed.

4. **Wrong retry count**: Was 10 (for margin). Binary uses exactly 9 (checks `8 < local_c8`). Fixed.

5. **Missing SetAd9637Enabled(false/true) decomposition**: The binary's CommonDeviceInit calls configure -> power_off -> SetEnabled(false) -> SetEnabled(true). Our code combined these differently. Restructured to match.

6. **DCM lock check**: Binary checks main DCM (bit 5) and slow DCM (bit 6) separately in SetAd9637Enabled(true), throwing distinct errors. Our code checked all 3 bits together. Fixed.

### Files changed
- `crates/saleae-driver/src/adc.rs` -- Major rewrite: added `reset_main_dcm()`, `set_ad9637_enabled_false()`, `set_ad9637_enabled_true()`. Removed old `dcm_lock_with_retry()` and `adc_reset_cycle()`. Updated `init_ad9637()` to match CommonDeviceInit flow.
- `crates/saleae-driver/src/fpga.rs` -- Updated mock response queue in `queue_xilinx_device_init_responses()` to match new init sequence.
- `crates/saleae-driver/tests/integration.rs` -- Updated `queue_xilinx_common_device_init()` mock responses.

### Test results
All 287 tests pass (266 unit + 21 integration).
<!-- SECTION:FINAL_SUMMARY:END -->
