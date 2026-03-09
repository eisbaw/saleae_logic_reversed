---
id: TASK-0011
title: RE digital capture data format and streaming protocol
status: In Progress
assignee: []
created_date: '2026-03-08 22:44'
updated_date: '2026-03-09 00:38'
labels: []
dependencies:
  - TASK-0006
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reverse engineer the digital sample data pipeline.

Known from string/symbol analysis:
- DigitalConfiguration_DigitalDerived_END
- DigitalConfiguration_SmartCable_END  
- Digital deserialization
- SamplesPerPacketMultiple_WithStats
- Separate Altera vs Xilinx min/max packet sizes

Investigate:
1. What USB endpoint(s) carry digital sample data?
2. What is the bulk transfer packet format? (header, payload, stats)
3. How are digital channels encoded in the data stream?
4. Is there on-device compression? If so, what algorithm?
5. How does sample rate affect packet structure?
6. How does channel count affect packet structure?
7. What is the "WithStats" metadata in each packet?

Capture USB traffic during a simple digital capture (1 channel, 1MHz) and correlate with Ghidra analysis of the deserialization code.
<!-- SECTION:DESCRIPTION:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Analysis Results (2026-03-09)

### Data Pipeline Traced
Full data flow from USB to storage:
EP2 IN -> TransferQueue -> OnDataBuffer -> DisaggregatorNode -> DigitalCreatorNode -> DigitalStore (RLE)

### Key Findings
1. Digital and analog data share EP2 IN -- DisaggregatorNode splits them
2. Digital data stored as RLE via AppendRle function
3. Packets have stats metadata (SamplesPerPacketMultiple_WithStats)
4. Start packets precede data (ch, timestamp, config_id)
5. Per-packet overflow detection (cur_overflow, prev_overflow)
6. Digital samples packed 8 per byte (sample_count % 8 == 0)
7. Channel count constrained to 2 or 4
8. FPGA-specific packet size limits (Altera vs Xilinx constants)
9. Transfer size computed dynamically per device type
10. Three FIFO overflow types: Digital, Analog, Master

### Still Needed
- Exact numeric values of FpgaConstants (min/max packet sizes)
- Exact WithStats alignment value
- Binary layout of start packet
- Interleaving pattern (sample vs chunk interleaved)
- USB capture validation against real hardware

### Artifacts Created
- re/notes/digital-capture-format.md (working notes)
- doc/protocol/digital-capture.md (verified protocol doc)
- re/ghidra/scripts/TraceDigitalCapture.py (Ghidra trace script)
<!-- SECTION:NOTES:END -->
