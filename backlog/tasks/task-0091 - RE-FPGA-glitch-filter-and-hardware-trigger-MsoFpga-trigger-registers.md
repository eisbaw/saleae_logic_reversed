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
Decompiled SetAnalogTriggerConfiguration, DisableAnalogTrigger, InternalSetDigitalConfiguration, and 18 other MsoFpga functions. Key findings:

**TRIGGER_CONTROL register (0x0070) corrected bit layout:**
- bits [31:16] = TriggerThreshold (int16, ADC code)
- bits [15:8] = ChannelSource (uint8) -- previously undocumented
- bits [7:4] = TriggerType (4 bits: 0=Edge, 1=Pulse)
- bits [3:2] = TriggerMode (2 bits: 0=Normal, 1=Auto, 2=Single, 3=Disabled)
- bits [1:0] = GlitchFilterEnabled (2 bits) -- previously identified as simple enable

**TriggerMode = 3 is Disabled** (confirmed: DisableAnalogTrigger writes 0x0C to reg 0x70).

**Hysteresis register (0x7C):** rising[15:0] | falling[31:16] (corrected from prior docs).

**Glitch filter:** per-channel digital, pulse width 5 or multiples of 10, range [5, 250005]. Register format: enabled<<20 | pulseWidth[19:0].

**Hardware triggers are MSO-only.** Logic Pro 8/16, Logic 8, and Logic 4 have no FPGA trigger registers.

**New register 0x0194** discovered (scope channel configuration, purpose TBD).

Documented in re/notes/fpga-glitch-filter.md. Updated doc/protocol/fpga-registers.md with corrected bit layouts and new register.
<!-- SECTION:FINAL_SUMMARY:END -->
