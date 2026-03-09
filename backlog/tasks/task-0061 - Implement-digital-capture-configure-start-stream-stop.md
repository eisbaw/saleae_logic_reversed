---
id: TASK-0061
title: 'Implement digital capture: configure, start, stream, stop'
status: To Do
assignee: []
created_date: '2026-03-09 11:22'
labels:
  - rust
  - hardware-testing
  - capture
dependencies:
  - TASK-0060
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
First end-to-end capture: stream digital samples from the Logic Pro 8.

Implementation:
1. Build SamplingConfigurationData: sample rate, enabled digital channels, samples per chunk
2. Send capture config to device via EP1 register writes
3. Submit USB bulk transfer queue on EP2 IN (0x82) with bMaxBurst=15
4. Send start sampling command (opcode 0x01) on EP1
5. Receive data chunks:
   - Parse 4-byte chunk header (channel, is_start, config_id)
   - Handle start packets (16 bytes with timestamp)
   - Extract digital sample bytes (8 samples per byte)
   - Strip WithStats trailer (20+ bytes) if present
6. Run disaggregator to split interleaved channels
7. Send stop command (opcode 0x02) on EP1
8. Drain remaining transfers

Output: raw digital samples to stdout or file. Even without trigger logic, we should see pin states.

Test: Connect a known signal (tie a channel to GND or VCC) and verify the captured data shows the expected constant level. Or capture with no connections and verify noise pattern.

References: doc/protocol/digital-capture.md, doc/protocol/capture-sequence.md, re/notes/start-packet-format.md, re/notes/withstats-format.md, re/notes/data-interleaving.md
<!-- SECTION:DESCRIPTION:END -->
