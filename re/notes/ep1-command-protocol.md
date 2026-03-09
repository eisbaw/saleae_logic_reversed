# EP1 Command Protocol Analysis

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (49,762,176 bytes, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40


## Overview

The Cypress FX3 microcontroller uses a command/response protocol on EP1
OUT (0x01) and EP1 IN (0x81) for device control. Commands are single-byte
opcodes followed by optional payload data. Responses are read back from
EP1 IN after each command write.

This protocol is separate from the high-speed sample data stream on EP2 IN
(0x82) and from the firmware upload protocol on EP0 (vendor control transfer
with bRequest=0xA0).


## Discovery Method

1. String search in .rodata for command-related constants
   (COMMAND_START_READ, COMMAND_STOP_STREAM, etc.)
2. RIP-relative LEA scan to find x86-64 code referencing those strings
3. Surrounding instruction analysis to extract immediate values
   (MOV byte [reg], imm8) that represent command opcodes
4. Virtual function table (vtable) offset analysis to identify the
   I/O dispatch pattern

### Key Code Regions

| Region | VA Range | Purpose |
|--------|----------|---------|
| Command dispatch | 0x1c3f000-0x1c56000 | Host-side command construction and send |
| FPGA register access | 0x2d62000-0x2d68000 | MsoFpga direct register path |
| String constants | 0x3e7570, 0x355a29 etc | .rodata section |


## Transport Layer: vtable Dispatch

Commands are sent and received through a virtual function table pattern
on the USB device object:

| vtable offset | Function | Direction | Purpose |
|---------------|----------|-----------|---------|
| `[this+0x18]` | Raw register I/O | EP1 OUT/IN | 2-byte read / 6-byte write, no opcode |
| `[this+0x20]` | Command write | EP1 OUT | Write opcode + payload |
| `[this+0x28]` | Command read | EP1 IN | Read response bytes |

### Two Register Access Paths

There are two distinct paths for FPGA register access:

**Path 1: Direct (MsoFpga, vtable[0x18])**
- Used by `MsoFpga::WriteRegister` and `MsoFpga::ReadRegister`
- No command opcode prefix
- Write: 6 bytes `[addr_hi] [addr_lo] [val_3] [val_2] [val_1] [val_0]`
- Read: send 2 bytes `[addr_hi] [addr_lo]`, receive 4 bytes
- Values are big-endian on wire (x86 ROL byte-swap)
- Located at VA ~0x2d62690

**Path 2: Command-wrapped (FpgaDeviceFeatures, vtable[0x20])**
- Used by `FpgaDeviceFeatures::ReadRegisters`
- Opcode 0x81 prefix
- Read: send 3 bytes `[0x81] [addr_hi] [addr_lo]`, receive 4 bytes
- Located in the 0x1c3f000-0x1c56000 region


## Command Opcode Map

12 distinct command opcodes identified through binary analysis:

| Opcode | Name | Payload | Response | Evidence |
|--------|------|---------|----------|----------|
| 0x01 | COMMAND_START_READ | 1 byte (opcode only) | 2 bytes: `[0x07, 0x00]` = success | String "COMMAND_START_READ reply" at 0x3be25b |
| 0x02 | COMMAND_STOP_STREAM | 1 byte (opcode only) | 2 bytes: `[0x07, 0x00]` = success | String "COMMAND_STOP_STREAM reply" at 0x3c4e7d |
| 0x22 | ReseedDevice | Direct bulk write, not vtable | Read back for verification | Used by TestFirmwareResponsivnessAndReseed |
| 0x55 | WriteEeprom | Variable (addr + data) | Confirmed via "Write eeprom response read" | String at multiple offsets |
| 0x7E | ResetFpga | 1 byte (opcode only) | 1 byte: `0x00` = success | Near FPGA bitstream code |
| 0x7F | WriteConfigurationData | Variable (see below) | 1 byte: `0x00` = success | Assertion strings for 62/509 limits |
| 0x81 | ReadRegisters | 3 bytes: `[0x81] [addr_hi] [addr_lo]` | 4 bytes (32-bit value) | FPGA register read with opcode prefix |
| 0x82 | ResetAndReEnumerate | 1 byte (opcode only) | Unknown | Near Fx2DeviceFeatures code |
| 0x86 | StopDataPath | 1 byte (opcode only) | Unknown | Appears in stop sequence |
| 0x88 | ReadI2c | Variable | Variable | I2C tunnel through FX3 |
| 0x89 | Device2_1 | Unknown | Variable | "Device2_1 reply" string |
| 0x8A | CMD_ASSERT_TEST | Unknown | Unknown | Firmware assertion test |
| 0x8B | GetDeviceInfo | 1 byte (opcode only) | Variable | Firmware build date, device ID |


## Packet Formats

### COMMAND_START_READ (0x01)

```
Host -> Device (EP1 OUT):  [0x01]                    (1 byte)
Device -> Host (EP1 IN):   [0x07] [0x00]             (2 bytes, success)
```

Start command tells the FX3 to begin forwarding FPGA sample data to EP2 IN.
Error response logged as: "Start command error. Error: {}; CY Error: {}"

### COMMAND_STOP_STREAM (0x02)

```
Host -> Device (EP1 OUT):  [0x02]                    (1 byte)
Device -> Host (EP1 IN):   [0x07] [0x00]             (2 bytes, success)
```

Stop command tells the FX3 to stop data flow on EP2 IN.
Response logged as: "COMMAND_STOP_STREAM reply"
Error: "Stop command error. Error: {}; CY Error: {}"

### ResetFpga (0x7E)

```
Host -> Device (EP1 OUT):  [0x7E]                    (1 byte)
Device -> Host (EP1 IN):   [0x00]                    (1 byte, success)
```

Sent before FPGA bitstream programming. Puts FPGA into reset state.

### WriteConfigurationData (0x7F)

USB 2.0 format (max 64 bytes total on EP1 OUT):
```
Host -> Device (EP1 OUT):  [0x7F] [len_8bit] [data...]    (2 + len bytes)
Device -> Host (EP1 IN):   [0x00]                          (1 byte, success)
```

USB 3.0 format (max 512 bytes total on EP1 OUT):
```
Host -> Device (EP1 OUT):  [0x7F] [len_lo] [len_hi] [data...]   (3 + len bytes)
Device -> Host (EP1 IN):   [0x00]                                 (1 byte, success)
```

- USB 2.0: max 62 bytes data payload (64 - 2 header bytes)
- USB 3.0: max 509 bytes data payload (512 - 3 header bytes)
- Used for FPGA bitstream download and capture parameter configuration
- Data is sent in chunks; caller loops until all data written
- Assertion: "config.size() <= 62" (USB 2.0) / "config.size() <= 509" (USB 3.0)

### ReadRegisters (0x81)

```
Host -> Device (EP1 OUT):  [0x81] [addr_hi] [addr_lo]      (3 bytes)
Device -> Host (EP1 IN):   [val_3] [val_2] [val_1] [val_0] (4 bytes, big-endian)
```

This is the command-wrapped register read path used by FpgaDeviceFeatures.

### ReseedDevice (0x22)

```
Host -> Device (EP1 OUT):  [0x22]    (1 byte, via direct bulk write)
Device -> Host (EP1 IN):   (read back for verification)
```

Sent via `libusb_bulk_transfer` directly (not through the vtable command
protocol). Used by `TestFirmwareResponsivnessAndReseed` to verify the FX3
is responsive before starting capture.


## Stop Sequence Command Order

The full stop sequence involves multiple commands:

```
1. StopDataPath (0x86)        -- stop FPGA data path
2. COMMAND_STOP_STREAM (0x02) -- stop FX3 data forwarding
3. Cancel USB transfers       -- libusb_cancel_transfer on EP2 IN
4. Stop read thread           -- terminate async read loop
```


## Firmware-Side Command Dispatch

The FX3 firmware in `CommandProcessing.c` has a `process_raw_command` loop
that reads from EP1 OUT via `GetCommandRxBuffer()`, dispatches based on the
first byte (opcode), and sends responses via `CommitCommandTxBuffer()` on
EP1 IN.

Unrecognized opcodes log: "unrecognized command"

The firmware source files involved:
- `CommandProcessing.c`: command parser and dispatch
- `ProUsb.c`: USB event handling, endpoint configuration
- `LogicPro.c`: main application, GPIF, GPIO setup


## Relationship to Other Protocols

| Protocol | Endpoint | When Used |
|----------|----------|-----------|
| Firmware upload | EP0 (control, bRequest=0xA0) | Before FX3 re-enumeration |
| Command protocol | EP1 OUT/IN (bulk) | After firmware load, device init + capture |
| Sample data | EP2 IN (bulk, async) | During active capture only |
| Register access | EP1 OUT/IN (bulk, no opcode) | MsoFpga direct path |

The command protocol is the primary control channel after firmware upload.
All device configuration, FPGA programming, capture start/stop, and
diagnostic commands flow through EP1 using these opcodes.


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| Opcodes 0x01, 0x02 (start/stop) | Confirmed | Literal strings + immediate values + response format |
| Opcode 0x7F (WriteConfigurationData) | Confirmed | 62/509 assertion strings + chunk loop code |
| Opcode 0x7E (ResetFpga) | Confirmed | Near bitstream code, 1-byte response pattern |
| Opcode 0x81 (ReadRegisters) | Confirmed | 3-byte format visible in instruction sequence |
| Vtable offsets 0x18/0x20/0x28 | Confirmed | Multiple independent call sites use same pattern |
| Response format [0x07, 0x00] | High | Extracted from immediate values near response check |
| Opcodes 0x55, 0x86-0x8B | Medium-High | Immediate values + nearby string context |
| Opcode 0x22 (ReseedDevice) | High | Direct bulk transfer path, not vtable |
| Two register access paths | Confirmed | Different VA regions, different vtable offsets |


## Open Questions

1. **Exact response format for 0x55/0x86-0x8B**: The response parsing
   for WriteEeprom, StopDataPath, ReadI2c, Device2_1, CMD_ASSERT_TEST,
   and GetDeviceInfo needs Ghidra decompilation to determine exact byte
   layouts.

2. **Error response codes**: The "CY Error" field in start/stop error
   messages suggests the firmware returns structured error information.
   What are the possible error codes?

3. **Command timeout values**: What timeout values does the host use
   for EP1 bulk transfers during command send/receive?

4. **WriteEeprom payload format**: What is the address/data format for
   EEPROM writes? Is it I2C addressing (7-bit) to the AT24C64D?

5. **ReadI2c tunnel format**: How are I2C read requests encoded in the
   0x88 command? Is this used for calibration data readback?

6. **GetDeviceInfo response fields**: What fields are returned by 0x8B?
   Firmware build date, device ID, hardware revision -- but exact offsets
   are unknown.


## References

- `re/notes/fpga-register-map.md` -- direct register access path details
- `re/notes/fx3-usb-events.md` -- firmware architecture and command loop
- `re/notes/capture-start-stop.md` -- capture sequence using these commands
- `re/notes/fpga-bitstream-protocol.md` -- bitstream download using 0x7E/0x7F
- `doc/protocol/fpga-registers.md` -- confirmed register wire protocol
- `doc/protocol/capture-sequence.md` -- confirmed capture sequence
- `re/ghidra/scripts/TraceEP1Commands.py` -- Ghidra script for automated analysis
