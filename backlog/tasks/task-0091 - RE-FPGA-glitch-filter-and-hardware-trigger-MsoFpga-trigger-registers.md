---
id: TASK-0091
title: RE FPGA glitch filter and hardware trigger (MsoFpga trigger registers)
status: Done
assignee: []
created_date: '2026-03-10 14:15'
updated_date: '2026-03-10 18:09'
labels:
  - re
  - ghidra
  - fpga
  - trigger
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The binary reveals FPGA-level trigger hardware:

```
void Saleae::MsoFpga::SetAnalogTriggerConfiguration(const MsoFpga::AnalogTriggerConfiguration &)
```

With register-level fields:
- `MsoFpga::TriggerMode_impl::TriggerMode` (SetField into register)
- `MsoFpga::TriggerType_impl::TriggerType` (SetField into register)

Glitch filter constraints:
```
config.mPulseWidth >= MsoConstants::MinGlitchFilterLength
config.mPulseWidth <= MsoConstants::MaxGlitchFilterLength
```

This is separate from the software triggers -- it's FPGA hardware that can reject glitches and detect analog threshold crossings.

Investigation:
1. Decompile SetAnalogTriggerConfiguration to extract register addresses
2. Extract TriggerMode and TriggerType enum values
3. Map glitch filter register fields (pulse width min/max)
4. Determine which devices support hardware trigger vs software-only
5. Check if Logic Pro 8 (Lattice ECP5) has this feature or if it's MSO-only
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
FPGA glitch filter and hardware trigger are MSO-only. Logic Pro 8/16, Logic 8, Logic 4 have no FPGA trigger registers -- all trigger detection is software-only.\n\nTRIGGER_CONTROL register (0x0070) corrected bit layout:\n- [31:16] TriggerThreshold (int16, ADC code)\n- [15:8] ChannelSource (uint8) -- previously undocumented\n- [7:4] TriggerType (4 bits: 0=Edge, 1=Pulse)\n- [3:2] TriggerMode (2 bits: 0=Normal, 1=Auto, 2=Single, 3=Disabled)\n- [1:0] GlitchFilterEnabled (2 bits)\n\nGlitch filter: per-channel digital, pulse width range [5, 250005], valid values: exactly 5 or any multiple of 10. Register format: enabled<<20 | pulseWidth[19:0].\n\nNew register 0x0194 discovered (scope channel config, purpose TBD).\n\nAnalysis: re/notes/fpga-glitch-filter.md, doc/protocol/fpga-registers.md updated."
<!-- SECTION:FINAL_SUMMARY:END -->
