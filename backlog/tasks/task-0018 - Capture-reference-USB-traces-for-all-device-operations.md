---
id: TASK-0018
title: Capture reference USB traces for all device operations
status: To Do
assignee: []
created_date: '2026-03-08 22:45'
labels: []
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
With real hardware connected, capture USB traffic for every major device operation using the original Saleae Logic 2 app.

Captures needed (per available device):
1. Device plug-in / enumeration (before firmware)
2. Firmware upload sequence
3. FPGA bitstream programming
4. Simple digital capture (1 channel, 1MHz, 1 second)
5. Multi-channel digital capture
6. Analog capture (1 channel)
7. Mixed analog+digital capture
8. Capture stop / device idle
9. Device disconnect
10. Calibration data read (if observable)

Save to re/captures/ with companion .md files per naming convention.
These captures serve as ground truth for differential testing against the Rust implementation.
<!-- SECTION:DESCRIPTION:END -->
