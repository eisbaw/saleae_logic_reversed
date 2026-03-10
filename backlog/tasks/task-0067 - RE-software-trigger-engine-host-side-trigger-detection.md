---
id: TASK-0067
title: RE software trigger engine (host-side trigger detection)
status: Done
assignee: []
created_date: '2026-03-10 12:23'
updated_date: '2026-03-10 15:24'
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
Completed reverse engineering of the software trigger engine. Findings documented in `re/notes/trigger-engine.md`.

Key discoveries:
- Three trigger nodes: DigitalTriggerNode, AnalogTriggerNode, AnalyzerTriggerNode
- Trigger type enum: 0=EdgeTrigger, 1=PulseTrigger (dispatched in CheckTrigger at 0x01be1720)
- Edge detection: scans DigitalData for transitions matching configured direction (rising=0, falling=1) across multiple channels with pattern matching
- Pulse detection: state machine measuring pulse duration, triggers when within [min_samples, max_samples] range
- AnalogTriggerConfiguration struct fully reconstructed (12 fields, offsets 0x00-0x30)
- FPGA trigger register map for MSO: 16 registers (0x40-0x9c) covering channel config, buffer management, holdoff/timeout, hysteresis, glitch filter, software trigger injection
- Buffer constraints documented: Altera vs Xilinx limits, alignment requirements
- Holdoff/timeout mechanism: 5 types (initial, normal holdoff/timeout, auto holdoff/timeout), all must be multiples of 8 samples
- Software trigger injection path via FPGA regs 0x98/0x9c with 60-bit timestamp
- Glitch filter: pulse width must be 5 or multiple of 10, range 5-250001

Created Ghidra scripts: DecompileTrigger.java, DecompileTrigger2.java, DecompileTrigger3.java
Decompiled output saved: re/ghidra/decompiled-trigger.txt (5171 lines)
<!-- SECTION:FINAL_SUMMARY:END -->
