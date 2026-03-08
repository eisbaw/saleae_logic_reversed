---
id: TASK-0015
title: Explore TransferQueue class -- async USB bulk transfer management
status: To Do
assignee: []
created_date: '2026-03-08 22:44'
labels: []
dependencies:
  - TASK-0006
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Investigate the TransferQueue class found in string analysis.

Known:
- "(anonymous namespace)::TransferQueue::TransferQueue(libusb_device_handle &, UsbEndpoint, uint32_t, uint32_t, uint64_t, uint64_t)"
- "Failed to submit USB transfer, reducing queue capacity to {}"
- "Done with usb stream, {} successful transfers"

Hypothesis: TransferQueue manages a pool of pre-allocated USB bulk transfers submitted asynchronously. It handles dynamic queue sizing based on USB bandwidth availability. This is the core streaming mechanism.

Investigate:
1. How many transfers are queued simultaneously?
2. What is the transfer size?
3. How does dynamic queue sizing work?
4. How does it handle transfer failures/retries?
5. What are the uint64_t parameters? (timeout? byte count limits?)

This class is critical for understanding capture data throughput.
<!-- SECTION:DESCRIPTION:END -->
