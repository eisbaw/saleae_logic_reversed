---
id: TASK-0018
title: Capture reference USB traces for all device operations
status: In Progress
assignee: []
created_date: '2026-03-08 22:45'
updated_date: '2026-03-09 02:26'
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

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Capture script re/scripts/usb-capture.sh exists and works
- [ ] #2 Justfile USB recipes use tcpdump (not tshark, which is not installed)
- [ ] #3 At least one USB capture completed with companion .md file
- [ ] #4 Capture setup documented in re/notes/usb-capture-setup.md
- [ ] #5 .gitignore covers both .pcap and .pcapng files
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## Infrastructure (Done)

- Created `re/scripts/usb-capture.sh` -- main capture script using tcpdump over SSH
- Updated justfile with working tcpdump-based recipes (old tshark ones replaced)
- Added `just usb-check` recipe to verify capture prerequisites
- Documented setup in `re/notes/usb-capture-setup.md`
- Updated `.gitignore` to cover both `.pcap` and `.pcapng` files

## Remote Host Status (nvdk-msdlab-09)

- tcpdump 4.99.0: INSTALLED, works with usbmon
- tshark: NOT INSTALLED (sudo dnf install wireshark-cli to add)
- usbmon: available via sudo modprobe, /dev/usbmon0-2 present
- Device: Logic Pro 8 (21a9:1005) on Bus 002, USB 3.0 SuperSpeed
- passwordless sudo: YES

## Captures Completed

1. [x] Device idle (bootloader state) -- `logic-pro-8_idle_2026-03-09_032409`
   - 50 packets in ~5s, all USB housekeeping (descriptor queries, hub status, LPM)
   - No Saleae-specific vendor requests
   - EP2 IN has bMaxBurst=15 confirming high-throughput data endpoint

## Captures Remaining (require Saleae Logic 2 app running)

2. [ ] Device plug-in / enumeration
3. [ ] Firmware upload sequence
4. [ ] FPGA bitstream programming
5. [ ] Simple digital capture (1ch, 1MHz, 1s)
6. [ ] Multi-channel digital capture
7. [ ] Analog capture (1ch)
8. [ ] Mixed analog+digital capture
9. [ ] Capture stop / device idle
10. [ ] Device disconnect
11. [ ] Calibration data read
<!-- SECTION:NOTES:END -->
