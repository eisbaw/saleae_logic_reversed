---
id: TASK-0170
title: Consolidate dual FPGA register naming and clean up fpga_regs.rs
status: Done
assignee: []
created_date: '2026-03-12 22:28'
updated_date: '2026-03-12 22:49'
labels:
  - proto
  - quality
  - fpga
dependencies: []
references:
  - 'crates/saleae-proto/src/fpga_regs.rs:582'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
fpga_regs.rs has dual names for identical registers: ADC_BANK_SELECT and FEAT_ADC_SPI_ADDR (both 0x03), ADC_REG_ADDR and FEAT_ADC_SPI_DATA_LO (both 0x16), etc. A test asserts equality but having two names for the same register invites conflicting writes. Pick one canonical name per register, make the other a doc alias. Also FEAT_PLL_STATUS and FEAT_JESD_LINK_STATUS both map to 0x40 with different semantics per device path -- needs a discriminator or explicit doc.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Consolidated dual FPGA register naming in fpga_regs.rs (commit cdac60b):

- Removed duplicate ADC register definitions: kept canonical ADC_BANK_SELECT (0x03), ADC_REG_ADDR (0x04), ADC_REG_DATA (0x05), ADC_REG_TRIGGER (0x16) in the FEAT block; removed the separate "ADC indirect register access" section and the old FEAT_ADC_SPI_* names (FEAT_ADC_SPI_ADDR, FEAT_ADC_SPI_DATA_LO, FEAT_ADC_SPI_DATA_HI, FEAT_ADC_SPI_ADDR_EXT)
- Made FEAT_JESD_LINK_STATUS an explicit alias of FEAT_PLL_STATUS (both 0x40) with doc comments explaining dual semantics: Spartan-6 reads it as PLL/DCM status, Lattice ECP5 reads it as JESD link status
- Made FEAT_JESD_STATUS_DETAIL an explicit alias of FEAT_PLL_STATUS_2 (both 0x4A) with same dual-semantics documentation
- Removed 20 unused backward-compat aliases (REG_ANALOG_CH_CONFIG_0..5, REG_TRIGGER_AUTO_CONFIG, REG_TRIGGER_HOLDOFF_LO/HI, REG_TRIGGER_TIMEOUT_LO/HI, REG_TRIGGER_INITIAL, REG_TEST_DATA_0/1) that had no consumers outside test assertions
- All 851 workspace tests pass
<!-- SECTION:FINAL_SUMMARY:END -->
