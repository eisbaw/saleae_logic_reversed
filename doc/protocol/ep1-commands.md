# EP1 Command Protocol

Status: High confidence (static analysis of libgraph_server_shared.so v2.4.40)
Corroboration: RIP-relative LEA cross-reference to string constants, vtable
offset pattern matching at 3 independent call sites, immediate value extraction,
firmware source file path cross-reference (CommandProcessing.c).
Hardware-validated: Reseed, Ping (0x7D), ReadVersion (0x07), ReadRegisters
(0x81), WriteRegisters (0x80), WriteConfigurationData (0x7F), ResetFpga
(0x7E), StartRead (0x01), StopStream (0x02) all tested on Logic Pro 8.


## Overview

After FX3 firmware upload and re-enumeration, the host communicates with the
device using a command/response protocol on EP1 bulk endpoints. Each command
is a single-byte opcode followed by optional payload bytes, sent via EP1 OUT
(0x01). The device responds on EP1 IN (0x81).

This protocol controls all device operations: FPGA programming, register
access, capture start/stop, EEPROM access, I2C tunneling, and diagnostics.


## Wire Format

- **Transport**: USB bulk transfers on EP1 OUT (write) and EP1 IN (read)
- **Command format**: `[opcode] [payload...]` (1 + N bytes)
- **Response format**: varies by command (1-4 bytes typical)
- **Byte order**: Big-endian for multi-byte register values


## Command Map

| Opcode | Name | Direction | Payload | Response |
|--------|------|-----------|---------|----------|
| 0x01 | COMMAND_START_READ | OUT | None | 2 bytes: `[0x07, 0x00]` |
| 0x02 | COMMAND_STOP_STREAM | OUT | None | 2 bytes: `[0x07, 0x00]` |
| 0x07 | ReadVersion (I2C tunnel) | OUT | I2C params | Variable (see `doc/protocol/eeprom.md`) |
| 0x55 | WriteEeprom | OUT | Address + data | Read-back confirmation |
| 0x7D | Ping | OUT | 1 byte | 1 byte: complement of input |
| 0x7E | ResetFpga | OUT | None | 1 byte: `0x00` |
| 0x7F | WriteConfigurationData | OUT | Length + data | 1 byte: `0x00` |
| 0x80 | WriteRegisters | OUT | Count + addr/val pairs | None (base) or 1 byte ACK (Pro) |
| 0x81 | ReadRegisters | OUT | Count + addresses | Count bytes (values) |
| 0x82 | ResetAndReEnumerate | OUT | None | (device resets) |
| 0x86 | StopDataPath | OUT | None | Unknown |
| 0x87 | WriteI2C | OUT | I2C addr + data | Confirmation |
| 0x88 | ReadI2c | OUT | Variable | Variable |
| 0x89 | Device2_1 | OUT | Unknown | Variable |
| 0x8A | CMD_ASSERT_TEST | OUT | Unknown | Unknown |
| 0x8B | GetDeviceInfo | OUT | None | Variable |


## Detailed Command Formats

### COMMAND_START_READ (0x01)

Tells the FX3 to begin forwarding FPGA sample data to EP2 IN.

```
Host -> Device (EP1 OUT):  [0x01]
Device -> Host (EP1 IN):   [0x07] [0x00]
```

Success response is `[0x07, 0x00]`. On failure, the host logs:
```
Start command error. Error: {}; CY Error: {}
```

### COMMAND_STOP_STREAM (0x02)

Tells the FX3 to stop forwarding data on EP2 IN.

```
Host -> Device (EP1 OUT):  [0x02]
Device -> Host (EP1 IN):   [0x07] [0x00]
```

### ResetFpga (0x7E)

Resets the FPGA before bitstream programming.

```
Host -> Device (EP1 OUT):  [0x7E]
Device -> Host (EP1 IN):   [0x00]
```

### WriteConfigurationData (0x7F)

Writes FPGA bitstream or capture configuration data in chunks.

**USB 2.0** (max 64 bytes total):
```
Host -> Device (EP1 OUT):  [0x7F] [len_8bit] [data...]
Device -> Host (EP1 IN):   [0x00]
```
Maximum data payload: 62 bytes.

**USB 3.0** (max 512 bytes total):
```
Host -> Device (EP1 OUT):  [0x7F] [len_lo] [len_hi] [data...]
Device -> Host (EP1 IN):   [0x00]
```
Maximum data payload: 509 bytes.

The caller loops, sending chunks until all data is written. Each chunk
gets an individual `0x00` success response.

### WriteRegisters (0x80)

Batch FPGA register write via the FpgaDeviceFeatures command path.

```
Host -> Device (EP1 OUT):  [0x80] [count] [addr0] [val0] [addr1] [val1] ... [addrN] [valN]
Device -> Host (EP1 IN):   (none for base devices, 1 byte ACK for Pro devices)
```

Register addresses are 8-bit (U8), values are 8-bit (U8). Maximum 31
registers per batch (assertion: `num_registers < 32`).

On Pro devices (ProFpgaDeviceFeatures), the host reads back a confirmation
byte after the write ("write register response check"). On non-Pro devices
(FpgaDeviceFeatures), the write is fire-and-forget with no response.

Note: This 8-bit register access path is distinct from the 16-bit MsoFpga
direct register path documented in `doc/protocol/fpga-registers.md`.

### ReadRegisters (0x81)

Reads FPGA registers via the FpgaDeviceFeatures command path.

**Single register read**:
```
Host -> Device (EP1 OUT):  [0x81] [0x01] [addr]     (3 bytes)
Device -> Host (EP1 IN):   [value]                   (1 byte)
```

**Batch register read**:
```
Host -> Device (EP1 OUT):  [0x81] [count] [addr0] [addr1] ... [addrN]
Device -> Host (EP1 IN):   [val0] [val1] ... [valN]  (count bytes)
```

Register addresses are 8-bit (U8), values are 8-bit (U8). Maximum 63
registers per batch read.

Note: There is a separate direct register access path (without the 0x81
opcode) used by MsoFpga code that uses 16-bit addresses and 32-bit values.
See `doc/protocol/fpga-registers.md`.

### WriteEeprom (0x55)

Writes data to the on-device EEPROM via the FX3 I2C bus.

```
Host -> Device (EP1 OUT):  [0x55] [addr_hi] [addr_lo] [data...]
Device -> Host (EP1 IN):   (read-back confirmation)
```

Address is 16-bit (U16). Data length is bounded by EepromPageSize()
(the EEPROM chip's page write limit). The host logs "Write eeprom
response read" after receiving the confirmation.

### WriteI2C (0x87)

Raw I2C write tunnel through the FX3. Used for crypto IC communication
and other I2C peripherals.

```
Host -> Device (EP1 OUT):  [0x87] [i2c_addr] [data...]
Device -> Host (EP1 IN):   (confirmation)
```

The host logs "WriteI2C request" before sending and "WriteI2C response"
after receiving the reply.

### ReadI2c (0x88)

I2C read tunnel through the FX3. Used for EEPROM reads and crypto IC
communication.

```
Host -> Device (EP1 OUT):  [0x88] [params...]
Device -> Host (EP1 IN):   [data...]
```

The host logs "ReadI2c request" before sending and "ReadI2c response"
after receiving. On error: "I2c tunnel error code {:#X}" or
"I2c tunnel op timed out {:#X}".

### ReseedDevice (no fixed opcode)

Cryptographic seed exchange for XOR obfuscation of subsequent commands.
Sends 4 random bytes to the FX3 via direct `libusb_bulk_transfer` (not
through the vtable command dispatch). Both host and device use this
shared seed for SimpleEncryption (XOR-based obfuscation).

```
Host -> Device (EP1 OUT):  [seed_0] [seed_1] [seed_2] [seed_3]  (4 bytes)
```

The host generates the seed using OpenSSL `RAND_bytes()`. The seed is
stored at `this->offset_0x64` and used by `SetControlByte()` in
`SimpleEncryption.cpp`. Timeout: 2000ms.

Note: The prior entry listing this as opcode 0x22 was incorrect. The
0x22 byte was an ASCII double-quote in the error string "Random not
seeded", not a command opcode.

### Ping (0x7D)

Firmware liveness check. Sends a test byte; firmware returns its
bitwise complement.

```
Host -> Device (EP1 OUT):  [0x7D] [0x55]
Device -> Host (EP1 IN):   [0xAA]
```

Used by `TestFirmwareResponsivnessAndReseed` before each capture start.
The host sends `0x55` and expects `0xAA` (`0x55 XOR 0xFF`). Retried
up to 5 times. On failure: "ping command produced incorrect result".


## Register Access: Two Paths

The FPGA registers can be accessed through two distinct code paths:

### Path 1: Direct (no opcode)

Used by `MsoFpga` class. Raw 2-byte or 6-byte packets on EP1, dispatched
through vtable offset `[this+0x18]`:

```
Read:  Send [addr_hi] [addr_lo]            -> Receive 4 bytes
Write: Send [addr_hi] [addr_lo] [v3] [v2] [v1] [v0]  -> (no response)
```

### Path 2: Command-wrapped (opcode 0x81)

Used by `FpgaDeviceFeatures` class. Prefixed with 0x81 opcode, dispatched
through vtable offset `[this+0x20]`/`[this+0x28]`:

```
Read:  Send [0x81] [addr_hi] [addr_lo]     -> Receive 4 bytes
```

Both paths access the same underlying FPGA register space.


## vtable I/O Dispatch

Host-side command I/O is dispatched through a C++ virtual function table:

| Offset | Purpose |
|--------|---------|
| `[this+0x18]` | Raw register I/O (no opcode, 2/6 byte packets) |
| `[this+0x20]` | Command write (EP1 OUT, opcode + payload) |
| `[this+0x28]` | Command read (EP1 IN, response bytes) |


## Firmware-Side Dispatch

The FX3 firmware (`CommandProcessing.c`) runs a `process_raw_command` loop:

1. `GetCommandRxBuffer()` -- receive command from EP1 OUT DMA
2. Parse first byte as opcode
3. Execute command handler
4. `CommitCommandTxBuffer()` -- send response via EP1 IN DMA
5. `DiscardRxBuffer()` -- release input buffer

Unrecognized opcodes produce: "unrecognized command"


## Command Sequence: Capture Start

```
1. SetBankPower()                    (register writes, direct path)
2. SetCaptureParameters()            (0x7F WriteConfigurationData, chunked)
3. TestFirmwareResponsivnessAndReseed():
   a. ReseedDevice()                 (send 4 random bytes, establish XOR seed)
   b. Ping [0x7D, 0x55]             (expect 0xAA back, up to 5 retries)
4. ComputeTransferSize()             (host-side calculation)
5. SendStartCommand()                (0x01 COMMAND_START_READ)
6. [Async bulk reads begin on EP2 IN]
```

## Command Sequence: Capture Stop

```
1. StopDataPath (0x86)
2. COMMAND_STOP_STREAM (0x02)
3. Cancel EP2 IN transfers (libusb_cancel_transfer)
4. Stop read thread
```

## Command Sequence: FPGA Programming

```
1. ResetFpga (0x7E)
2. WriteConfigurationData (0x7F) x N chunks
3. [Post-programming register init via direct path]
```


## Endpoint Usage Summary

| Endpoint | Protocol | Used For |
|----------|----------|----------|
| EP0 (Control) | Vendor bRequest=0xA0 | Firmware upload only |
| EP1 OUT (0x01) | Command protocol | All device control |
| EP1 IN (0x81) | Command protocol | All device responses |
| EP2 IN (0x82) | Raw bulk stream | Sample data during capture |


## Cross-References

- `doc/protocol/eeprom.md` -- EEPROM access via ReadVersion (0x07) I2C tunnel
- `doc/protocol/fpga-registers.md` -- FPGA registers accessed via opcodes 0x80/0x81
- `doc/protocol/fpga-bitstream.md` -- FPGA programming via opcodes 0x7E/0x7F
- `doc/protocol/capture-sequence.md` -- capture start/stop using opcodes 0x01/0x02
- `crates/saleae-proto/src/ep1_opcodes.rs` -- Rust opcode constants
- `crates/saleae-proto/src/obfuscation.rs` -- XOR obfuscation (SimpleEncryption)
- `crates/saleae-driver/src/ep1.rs` -- EP1 command channel implementation
