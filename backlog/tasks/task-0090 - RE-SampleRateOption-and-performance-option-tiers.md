---
id: TASK-0090
title: RE SampleRateOption and performance option tiers
status: Done
assignee: []
created_date: '2026-03-10 14:15'
updated_date: '2026-03-10 15:07'
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
Decompiled SampleRateOption struct (24 bytes with digitalSampleRateHz, analogPhysicalSampleRateHz, downsampleGroupIndex, etc.) and the three performance tier APIs. Pro devices have 15 digital rates (500MHz-1MHz) and 11 analog rates (100MHz-1MHz). Logic 4 fixed at 12MHz digital/6MHz analog with decimation. Three tiers: digital-only, analog basic, analog advanced (5 bandwidth groups). Created re/notes/sample-rate-options.md and multiple Ghidra/analysis scripts.
<!-- SECTION:FINAL_SUMMARY:END -->
