---
id: TASK-0067
title: RE software trigger engine (host-side trigger detection)
status: Done
assignee: []
created_date: '2026-03-10 12:23'
updated_date: '2026-03-10 15:25'
labels:
  - re
  - ghidra
  - trigger
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the host-side software trigger mechanism in libgraph_server_shared.so. Three trigger node types exist:

**Digital trigger** (`Saleae::Graph::DigitalTriggerNode`):
- Source: `/workspace/monorepo/graph-io/nodes/digital_trigger_node/src/digital_trigger_node.cpp`
- Types: EdgeTrigger, PulseTrigger
- Settings: DigitalTriggerSettingsData, DigitalTriggerState2
- Key method: `HandleDigitalData(string, uint64_t, shared_ptr&lt;DigitalData&gt;)`
- Also: `HandleSamplingConfiguration(string, uint64_t, shared_ptr&lt;SamplingConfigurationData&gt;)`
- Receives data via pipe handler from digital store

**Analog trigger** (`Saleae::Graph::AnalogTriggerNode`):
- Settings: AnalogTriggerSettingsData, AnalogTriggerNodeState2
- Hardware component: `MsoFpga::SetAnalogTriggerConfiguration` (FPGA-assisted for MSO)
- Constraints from assertions:
  - `triggerHysteresisFalling >= triggerThreshold`
  - `triggerHysteresisRising <= triggerThreshold`
  - `config.mPulseWidth >= MsoConstants::MinGlitchFilterLength` (glitch filter!)
- For Logic devices: pure software trigger on ADC samples

**Analyzer trigger** (`Saleae::Graph::AnalyzerTriggerNode`):
- Settings: AnalyzerTriggerSettingsData
- Triggers on protocol decoder results (e.g. "trigger on SPI byte 0xAA")
- Has WorkRequest and NewAnalyzerRunStarted handlers

**Common trigger infrastructure**:
- Pre/post trigger buffers: `(preTriggerLength + postTriggerLength) % SamplesPerPacketMultiple_WithStats == 0`
- Pre-trigger index bounds: `MinTriggerBufferIndex_Altera`, `MinTriggerBufferIndex_Xilinx`
- Post-trigger: `MaxTriggerBufferIndex`
- Holdoff: `MaxHoldoffSamples`, timeout: `MaxTimeoutSamples`
- Software trigger: `SetSoftwareTrigger`, `"Received a software trigger at timestamp {}"`
- Navigation: `NavigateTrigger`, `SetCurrentTrigger`, `hasNextTrigger`, `hasPreviousTrigger`
- Memory trigger: `MemoryManagerInternals::CreateMemoryTrigger`

Investigation steps:
1. Search for mangled names to find vtable addresses
2. Decompile DigitalTriggerNode::HandleDigitalData -- this is the core edge detection
3. Decompile AnalogTriggerNode pipe handler
4. Extract trigger configuration struct layouts from serialization converters
5. Map pre/post trigger buffer management
6. Document glitch filter parameters
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Decompiled trigger engine: EdgeTrigger_Detect (0x01be1960) and PulseTrigger_Detect (0x01be23c0) dispatched by type field at offset 0x58. Edge detection iterates DigitalData for transitions with multi-channel pattern support. Pulse detection is a state machine measuring edge-to-edge duration against [min,max] range. AnalogTriggerConfiguration has 12 fields (channel, threshold, hysteresis, mode/type, holdoff/timeout). MSO FPGA trigger registers at 0x40-0x9c (16 regs). Software trigger injects 60-bit timestamp via registers 0x98/0x9c. For Logic Pro, triggers are purely software. Created re/notes/trigger-engine.md (452 lines) and 3 Ghidra scripts.
<!-- SECTION:FINAL_SUMMARY:END -->
