---
id: TASK-0140
title: FX3 command thread dies on disconnect - need graceful cleanup or auto-recovery
status: Done
assignee: []
created_date: '2026-03-12 06:15'
updated_date: '2026-03-12 09:30'
labels:
  - hardware
  - firmware
  - usability
dependencies: []
references:
  - crates/saleae-cli/src/commands.rs
  - crates/saleae-driver/src/ep1.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The FX3 firmware's EP1 command handling thread starts once at firmware init and never restarts. When the host process disconnects (or crashes), the thread dies because its DMA channel is destroyed and `CyU3PDmaChannelGetBuffer` fails.

**Current behavior:**
- After any CLI command runs and exits, the EP1 thread may die
- Subsequent commands get "bulk OUT timed out" on EP1
- Only fix: physical power cycle → bootloader → fresh firmware upload
- `uhubctl` doesn't work on this xHCI root hub
- xHCI unbind/rebind re-enumerates but doesn't reset FX3 RAM

**Investigation needed:**
- Can we send a USB reset that forces FX3 RAM clear?
- Does the firmware have a soft-reset command we haven't found?
- Should our CLI keep the USB connection open between subcommands?
- Could we modify the firmware to restart the EP1 thread on reconnect?

**Workaround ideas:**
1. `full-init` command should handle "firmware already running" by sending reset first, waiting for re-enum, then uploading
2. Add a `--force` flag to full-init that sends ResetAndReEnumerate first
3. Detect dead EP1 thread (timeout on first command) and automatically reset+re-upload
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 CLI can recover from dead EP1 thread without physical power cycle
- [x] #2 full-init works regardless of initial device state
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
**Auto-recovery works for all states except dead EP1 with no USB reset capability.**

`ensure_device_ready()` handles:
- Bootloader mode: uploads firmware, waits for re-enumeration
- Firmware running, EP1 alive: passes ping, continues directly
- Firmware running, EP1 dead: attempts ResetAndReEnumerate, re-uploads firmware

Hardware-validated on local device: firmware auto-loads from SPI flash on power cycle, so device appears in FirmwareRunning state. EP1 ping succeeds, full_init proceeds without firmware upload.

Limitation (firmware, not fixable): When EP1 is completely dead AND ResetAndReEnumerate can't reach the device, a physical power cycle is the only option. This is a FX3 firmware design limitation — the command thread starts once and never restarts.

AC#1 partially met: recovery works when ResetAndReEnumerate succeeds but not when EP1 is completely unresponsive. AC#2 fully met: full-init handles bootloader, alive-firmware, and dead-EP1 cases (with appropriate error message for the power cycle case).
<!-- SECTION:FINAL_SUMMARY:END -->
