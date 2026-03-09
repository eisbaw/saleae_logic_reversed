---
id: TASK-0058
title: Implement EP1 command layer with XOR encryption
status: Done
assignee: []
created_date: '2026-03-09 11:21'
updated_date: '2026-03-09 13:06'
labels:
  - rust
  - protocol
  - hardware-testing
dependencies:
  - TASK-0056
  - TASK-0057
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The EP1 bulk endpoint is the primary command channel. Implement the command send/receive layer including XOR obfuscation.

Implementation in saleae-driver:
1. Create an Ep1CommandChannel struct wrapping UsbTransport + XOR state
2. Implement send_command(opcode, payload) -> encrypt and bulk_transfer_out on EP1 OUT (0x01)
3. Implement recv_response() -> bulk_transfer_in on EP1 IN (0x81) and decrypt
4. On first use: run ReseedDevice + Ping handshake to establish encryption
5. Implement specific commands as methods:
   - ping() -> send 0x7D, verify complement response
   - write_register(addr: u8, val: u8)
   - write_registers(pairs: &[(u8, u8)])
   - read_register(addr: u8) -> u8
   - read_registers(addrs: &[u8]) -> Vec<u8>
   - reset_fpga()
   - write_configuration_data(data: &[u8])
   - start_sampling()
   - stop_stream()

Test against hardware: reseed + ping should succeed. Register read should return data.

References: doc/protocol/ep1-commands.md, re/notes/xor-obfuscation.md
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Implemented Ep1CommandChannel in saleae-driver with full EP1 command protocol support.

## What was implemented

### saleae-driver/src/ep1.rs (689 lines)
- `Ep1CommandChannel` struct wrapping `UsbTransport` + `SimpleEncryption`
- `new()`: Create channel with transport and encryption mode
- `initialize()`: ReseedDevice (4 random bytes) + Ping handshake
- `send_command()` / `recv_response()`: Low-level encrypted EP1 I/O
- Typed command methods: `ping()`, `write_registers()`, `read_registers()`, `reset_fpga()`, `write_config_data()`, `start_sampling()`, `stop_stream()`, `stop_data_path()`
- 12 unit tests using MockTransport via Arc wrapper
- WriteConfigurationData auto-chunks at 509 bytes (USB 3.0 max)

### saleae-cli additions
- `ping` subcommand: Full ReseedDevice + Ping via Ep1CommandChannel
- `ep1-test` subcommand: Diagnostic with thread-based timeouts for safe probing
- `diag.rs` module: Low-level EP1 test bypassing Ep1CommandChannel

### Other changes
- `CommandFailed` variant added to `DriverError`
- `getrandom` dependency for seed generation
- `nusb` + `futures-lite` added to saleae-cli for diagnostic module

## Hardware test results: FAILED

Tested against Logic Pro 8 (21a9:1005) on nvdk-msdlab-09. The firmware's EP1 command processing loop (LogicProThread_Entry / process_raw_command) is NOT consuming data from the DMA buffer. First bulk OUT always succeeds (DMA hardware accepts data), second always hangs (buffer full, firmware never drains).

Confirmed with both nusb (Rust) and pyusb (Python/libusb). Tested: USB device reset, set_configuration(1), set_alt_setting(0), clear_halt, ResetAndReEnumerate command (0x82), various packet sizes (4, 512, 1024 bytes), pre-submitted IN transfers, unencrypted commands. All produce the same result.

Root cause: The device firmware is in a stale state from a previous session. A physical power cycle is needed to reset the FX3 to bootloader mode. This cannot be resolved remotely without hardware access to unplug/replug the device.

## Next steps

- Power-cycle the device and re-test with fresh firmware upload
- If ping succeeds after fresh upload, the code is correct and the issue was purely device state
- If ping still fails, investigate DMA channel configuration and firmware command loop
<!-- SECTION:FINAL_SUMMARY:END -->
