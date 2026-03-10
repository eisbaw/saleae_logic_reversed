---
id: TASK-0090
title: RE SampleRateOption and performance option tiers
status: Done
assignee: []
created_date: '2026-03-10 14:15'
updated_date: '2026-03-10 15:06'
labels:
  - re
  - ghidra
  - sample-rate
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Three distinct sample rate configuration APIs discovered in the binary:

```
void DeviceSettings::GetDigitalOnlyPerformanceOptions(std::vector&lt;SampleRateOption&gt; &)
void DeviceSettings::GetAnalogBasicPerformanceOptions(std::vector&lt;SampleRateOption&gt; &)
void DeviceSettings::GetAnalogAdvancedPerformanceOptions(std::vector&lt;std::vector&lt;SampleRateOption&gt;&gt; &)
```

Plus setters:
```
void DeviceSettings::SetDigitalOnlyPerformanceOption(const SampleRateOption &)
void DeviceSettings::SetAnalogBasicPerformanceOption(const SampleRateOption &)
void DeviceSettings::SetAnalogAdvancedPerformanceOption(const SampleRateOption &)
```

And SampleRateOption accessors:
```
U32 SampleRateOption::AnalogPhysicalSampleRateHz() const
U32 SampleRateOption::DigitalSampleRateHz() const
```

Also: `HandleSetSampleRate`, `"set sample rate: {}, {}"`, `"sample rate not found"` error messages.

Decompiling these reveals:
- All valid sample rate combinations per device
- How analog and digital rates relate (digital can be higher than analog)
- The SampleRateOption struct layout
- Per-device rate tables (connected to TASK-0044 GetDeviceSettings)

Source: `/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceSettings.cpp`
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled and analyzed the complete sample rate configuration system from DeviceSettings.cpp.

Key findings:
- SampleRateOption struct: 24 bytes with fields for digital rate, analog physical rate, analog output rate, validity flags, downsample flag, and group index
- Three performance tiers: Digital-Only (Logic/Logic16), Analog Basic (Logic4/Logic8), Analog Advanced (Pro 8/16)
- Pro device digital rates: 500M, 250M, 125M, 100M, 50M, 25M, 20M, 12.5M, 10M, 6.25M, 5M, 4M, 2.5M, 2M, 1M MHz
- Pro device analog rates: 100M (USB3 only), 50M, 40M, 25M, 20M, 10M, 8M, 5M, 4M, 2M, 1M MHz
- Logic 4: fixed 12 MHz digital / 6 MHz analog, with decimation to 125kHz, 5kHz, 1kHz, 100Hz, 10Hz
- Analog Advanced tier uses 5 bandwidth groups (100%, 80%, 60%, 40%, 20% of max bitrate)
- USB speed determines max bitrate, gating which rates are available

Files: re/notes/sample-rate-options.md, re/ghidra/scripts/DecompileSampleRate.java, re/ghidra/scripts/ExtractSampleRateData.java
<!-- SECTION:FINAL_SUMMARY:END -->
