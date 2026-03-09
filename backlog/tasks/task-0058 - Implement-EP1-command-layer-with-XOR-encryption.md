---
id: TASK-0058
title: Implement EP1 command layer with XOR encryption
status: To Do
assignee: []
created_date: '2026-03-09 11:21'
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
