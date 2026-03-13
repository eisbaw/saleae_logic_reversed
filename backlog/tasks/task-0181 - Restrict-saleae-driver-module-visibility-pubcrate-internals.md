---
id: TASK-0181
title: Restrict saleae-driver module visibility -- pub(crate) internals
status: Done
assignee: []
created_date: '2026-03-13 05:35'
updated_date: '2026-03-13 09:17'
labels:
  - architecture
  - driver
  - quality
dependencies: []
references:
  - crates/saleae-driver/src/lib.rs
  - 'crates/saleae-driver/src/facade.rs:115'
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Every module in saleae-driver is pub mod (adc, capture, ep1, fpga, etc.) but the facade exists as the intended entry point. The CLI freely reaches into saleae_driver::fpga::program_fpga, saleae_driver::ep1::Ep1CommandChannel, etc. Make internal modules pub(crate) and route through facade. Also decouple facade from concrete NusbTransport (facade.rs line 115 directly constructs NusbTransport::open).
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Changed 7 internal modules to pub(crate): autoscale, eeprom, framed, hmcad, init, jesd, transfer_queue. Added root-level re-exports for key types.
<!-- SECTION:FINAL_SUMMARY:END -->
