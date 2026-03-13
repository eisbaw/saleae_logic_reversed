---
id: TASK-0176
title: Consolidate duplicate Disaggregator implementations (proto vs driver)
status: Done
assignee: []
created_date: '2026-03-13 05:35'
updated_date: '2026-03-13 06:55'
labels:
  - architecture
  - tech-debt
  - driver
  - proto
dependencies: []
references:
  - crates/saleae-proto/src/disaggregator.rs
  - crates/saleae-driver/src/disaggregator.rs
  - 'crates/saleae-driver/src/calibrate.rs:23'
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Two independent Disaggregator implementations exist: saleae-proto/src/disaggregator.rs (1248 lines) and saleae-driver/src/disaggregator.rs (1114 lines). Both implement the same DeviceCalculateMixRate algorithm, 12-bit ADC extraction, contract_read_data, and digital byte-swap logic. The CLI uses the proto version while calibrate.rs uses the driver version -- different types that cannot interoperate without conversion. Pick one canonical implementation (proto, since it's wire-format logic), delete the other, update all references.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Consolidated two duplicate Disaggregator implementations into the single canonical version in saleae-proto.

**What changed:**
- Proto Disaggregator gained: ContractReadData (low_speed_divider), from_params() and from_register_settings() constructors, num_digital/num_analog/low_speed_divider accessors
- CaptureConfig.disaggregator_settings() replaced by CaptureConfig.disaggregator() returning proto Disaggregator
- calibrate.rs now uses proto DisaggregatedData instead of driver DisaggregatorOutput
- CLI commands.rs now uses proto Disaggregator.process_chunk()
- Deleted saleae-driver/src/disaggregator.rs (1114 lines)

**Net result:** -921 lines, 870 tests pass, zero clippy warnings.
**Commit:** caa7e9a
<!-- SECTION:FINAL_SUMMARY:END -->
