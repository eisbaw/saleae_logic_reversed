---
id: TASK-0196
title: 'Clean up doc/ stale items: resolved open questions, missing cross-refs'
status: Done
assignee: []
created_date: '2026-03-13 05:37'
updated_date: '2026-03-13 10:56'
labels:
  - docs
  - quality
dependencies: []
references:
  - doc/protocol/analog-capture.md
  - doc/protocol/capture-constants.md
  - doc/protocol/fpga-registers.md
  - doc/protocol/digital-capture.md
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Multiple doc cleanup items: (1) analog-capture.md has "RESOLVED" items still in Open Questions section -- should be removed or moved. (2) capture-constants.md and capture-sequence.md list open questions with corresponding backlog tasks but no cross-references. (3) fpga-registers.md uses inconsistent naming (CAPTURE_CONTROL vs r0x00). (4) Digital-capture.md and capture-sequence.md have different confidence ratings for the same protocol claims. (5) Post-init sequence verification (fpga-registers.md:235-257) doesn't explain success criteria, timeouts, or error cases.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Cleaned up all 8 doc/protocol/*.md files (ipc.md, eeprom.md, overflow-handling.md were already clean).

Changes:
- analog-capture.md: Fixed FIR->IIR (calibration uses biquad IIR), resolved interleaving format question (TASK-0141), updated LMH6518 VGA description to match AfeChannelConfig layout (TASK-0197), added cross-refs
- capture-sequence.md: Resolved 2 of 3 open questions (start/stop bytes now hardware-validated, DisaggregatorSettingsData resolved by TASK-0141), added cross-refs
- capture-constants.md: Updated Altera/Xilinx mapping to reference DeviceFeatureClass/FpgaVariant/AdcType Rust types
- ep1-commands.md: Added ReadVersion (0x07) to command map table, updated status to reflect hardware validation, added cross-refs
- fpga-bitstream.md: Updated confidence to reflect pcap/hardware validation, added FpgaVariant/AdcType references, added cross-refs
- fpga-registers.md: Updated status to partially validated, added cross-refs
- digital-capture.md: Added cross-refs to overflow-handling and disaggregator
- fx3-firmware.md: Added cross-refs to downstream protocol docs and Rust code

Commit: dfd96b9
<!-- SECTION:FINAL_SUMMARY:END -->
