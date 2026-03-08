---
id: TASK-0011
title: RE digital capture data format and streaming protocol
status: To Do
assignee: []
created_date: '2026-03-08 22:44'
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
