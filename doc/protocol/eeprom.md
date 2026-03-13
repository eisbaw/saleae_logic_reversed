# EEPROM Layout and Access

Confidence: **Confirmed** -- hardware-validated against a real Logic Pro 8
(21a9:1005) on nvdk-msdlab-09, corroborated by Ghidra decompilation of
`libgraph_server_shared.so` v2.4.40 and pcap analysis.

Binary dump: `re/captures/saleae-eeprom-512.bin` (first 512 bytes).


## Overview

FX3-based Saleae Logic devices have a 4 KiB I2C EEPROM on the board. It
stores the Cypress bootloader header, USB identity, device serial,
calibration coefficients, and hardware revision. The host reads the EEPROM
during device initialization to determine FPGA variant, ADC type, and
per-channel calibration parameters.

Two distinct I2C peripherals are relevant:

1. **Main EEPROM** (FX3 internal I2C bus) -- 4096 bytes at I2C address
   implied by the CY bootloader. Accessed via the ReadVersion opcode
   (0x07) I2C tunnel. Contains identity, calibration coefficients, and
   hardware revision.

2. **Calibration EEPROM** (I2C address 0xC0, 7-bit 0x60) -- a separate
   I2C device accessed via WriteI2C (0x87) and ReadI2C (0x88) opcodes.
   Contains device-specific calibration blocks used during analog
   initialization. Documented separately below.


## Main EEPROM Access Mechanism

The main EEPROM is read and written through the EP1 ReadVersion command
(opcode 0x07) using the FX3 I2C tunnel.

### Read

```
Host -> Device (EP1 OUT):  [0x07] [0x33] [0x81] [addr_lo] [addr_hi] [count_lo] [count_hi]
Device -> Host (EP1 IN):   [data...]  (count bytes)
```

The `0x33 0x81` prefix selects the I2C EEPROM read path inside the FX3
firmware. Address and count are 16-bit little-endian.

Example: read 16 bytes from offset 0x0010:
```
[0x07] [0x33] [0x81] [0x10] [0x00] [0x10] [0x00]
```

### Write

Writing uses a two-step sequence through the same opcode:

```
Step 1:  [0x07] [0x33] [0x81] [addr_lo] [count]    (header)
Step 2:  [0x07] [data...]                           (payload)
```

Decompiled from `Fx3DeviceFeatures::WriteEeprom` (ELF VA 0x1c50d80).
Writes are chunked to 256 bytes (EEPROM page size). The firmware retries
up to 3 times on failure.

### Rust implementation

- `Ep1CommandChannel::read_eeprom()` in `crates/saleae-driver/src/ep1.rs`
- `Ep1CommandChannel::write_eeprom()` in `crates/saleae-driver/src/ep1.rs`
- `eeprom_read_all()` in `crates/saleae-driver/src/eeprom.rs`


## Main EEPROM Layout

Total size: 4096 bytes (0x1000). Read in 16 chunks of 256 bytes each.
Decompiled from `FUN_01d43660` (Ghidra 01d43660).

| Offset | Size | Content | Confidence |
|--------|------|---------|------------|
| 0x0000 | 2 | `"CY"` -- Cypress B2 bootloader magic | Confirmed |
| 0x0002 | 2 | B2 bootloader header (flags/config) | Confirmed |
| 0x0004 | 4 | PID + VID, each 16-bit LE: `05 10 A9 21` = 21a9:1005 | Confirmed |
| 0x0008 | 8 | Device ID / serial number, little-endian | Confirmed |
| 0x0010 | 16 | Calibration coefficients (written to FPGA r0x17) | Confirmed |
| 0x0020 | 6 | Hardware revision: `"REV"` + major + `"."` + minor | Confirmed |
| 0x0026 | 474 | Empty (0xFF) | Confirmed |
| 0x0200 | 3584 | Empty (0xFF) on tested device | Observed |

### Hex dump (first 38 bytes)

From the Logic Pro 8 on nvdk-msdlab-09:

```
0x0000: 43 59 05 B2  -- "CY" + B2 bootloader header
0x0004: 05 10 A9 21  -- PID=0x1005, VID=0x21A9 -> 21a9:1005 (Logic Pro 8)
0x0008: F1 D9 D4 1B B5 51 40 76  -- Device ID (LE: 0x764051B51BD4D9F1)
0x0010: 46 FF C6 A6 60 B7 DE 07 67 8B 03 66 11 35 7C 7F  -- 16 cal coefficients
0x0020: 52 45 56 01 2E 02  -- "REV" + major=1 + "." + minor=2
0x0026: FF FF FF ...  -- empty
```


## Field Details

### Bootloader Header (0x0000-0x0003)

The first two bytes are the ASCII string `"CY"` (0x43 0x59), which is the
Cypress B2 bootloader signature. The FX3 boot ROM looks for this magic to
determine the boot image format. The next two bytes contain boot
configuration flags.

The EEPROM self-test (`LogicProfessionalDevice::TestEeprom`, ELF VA
0x1c44010) preserves bytes 0x0000-0x0007 (bootloader header + VID/PID)
and overwrites everything from offset 0x0008 onward with an incrementing
test pattern. This is a destructive manufacturing diagnostic.

### PID/VID (0x0004-0x0007)

USB Product ID (bytes 4-5) and Vendor ID (bytes 6-7), each stored as
16-bit little-endian. This is the Cypress B2 bootloader convention
(PID first, then VID). The FX3 boot ROM uses these to enumerate the
device on the USB bus before firmware upload.

| Bytes | VID:PID | Device |
|-------|---------|--------|
| `03 10 A9 21` | 21a9:1003 | Logic 4 |
| `04 10 A9 21` | 21a9:1004 | Logic 8 |
| `05 10 A9 21` | 21a9:1005 | Logic Pro 8 |
| `06 10 A9 21` | 21a9:1006 | Logic Pro 16 |

### Device ID / Serial (0x0008-0x000F)

8-byte factory-assigned device identifier, stored little-endian. This is
the device's unique identity used to download per-device calibration data
from Saleae's servers.

Example: bytes `F1 D9 D4 1B B5 51 40 76` = integer
`0x764051B51BD4D9F1` = decimal `8520900333282843121`.

The calibration file URL is derived from this ID:
```
https://downloads.saleae.com/calibration/764051b51bd4d9f1.cal
```

See `doc/calibration/calibration-format.md` for the .cal file format.

### Calibration Coefficients (0x0010-0x001F)

16 bytes of calibration coefficients read during Phase 3 of device
initialization. These are written one-by-one to FPGA register 0x17
(`CAL_COEFF_REG`) using WriteRegisters (opcode 0x80).

Read command:
```
[0x07] [0x33] [0x81] [0x10] [0x00] [0x10] [0x00]
```

The coefficients are device-specific and presumably written during factory
calibration. The exact meaning of each byte is not yet fully decoded, but
they are applied to the FPGA's analog front-end calibration logic before
ADC initialization completes.

Values from the test device:
```
46 FF C6 A6 60 B7 DE 07 67 8B 03 66 11 35 7C 7F
```

These match the pcap Phase 3e FPGA r0x17 writes exactly.

Rust implementation: `jesd::write_calibration_coefficients()` in
`crates/saleae-driver/src/jesd.rs`.

### Hardware Revision (0x0020-0x0025)

6 bytes encoding the board hardware revision. Format:

```
[0x52] [0x45] [0x56] [major] [0x2E] [minor]
  'R'    'E'    'V'           '.'
```

The major revision determines the FPGA variant and ADC type via
`ParseBoardRevision` (ELF VA 0x1c23bc0):

| Major | FPGA | ADC | Bitstream |
|-------|------|-----|-----------|
| 0 | Xilinx Spartan-6 | AD9637 (parallel LVDS) | `bitstream-spartan6-slx9-02.bin` |
| 1 | Xilinx Spartan-6 (alt) | AD9637 | `bitstream-spartan6-slx9-02.bin` |
| 3 | Lattice ECP5 | AD9637 | Lattice-specific |
| 4, 5, 6, 9, 10, 11 | Lattice ECP5 | ADS528x (JESD204B) | Lattice-specific |
| 7 | Xilinx Spartan-6 (alt) | ADS528x (JESD204B) | Xilinx-specific |
| 8 | Lattice ECP5 | ADS528x (JESD204B) | Lattice-specific |

Major revision 2 is explicitly unsupported in the binary (throws
exception).

The test device reports Rev 1.02 = Xilinx Spartan-6 (alt) + AD9637.

Read command:
```
[0x07] [0x33] [0x81] [0x20] [0x00] [0x06] [0x00]
```

Rust implementation: `HardwareRevision::parse()` in
`crates/saleae-proto/src/device.rs`.


## Calibration EEPROM (I2C Address 0xC0)

A separate I2C device at address 0xC0 (7-bit address 0x60) is accessed
during Phase 6 of device initialization. This is not the main EEPROM but
a dedicated calibration controller with its own register-based protocol.

It is accessed via the WriteI2C (0x87) and ReadI2C (0x88) EP1 commands.

### Registers

| Register | Direction | Description |
|----------|-----------|-------------|
| 0x01 | Write | Acknowledge / status reset |
| 0x04 | Read | Device identification (5 bytes) |
| 0x08 | Write | Issue calibration block read command |
| 0x1C | Write | Write calibration parameters (26 bytes observed) |
| 0x23 | Read | Poll for 35-byte calibration block response |
| 0x43 | Read | Poll for 67-byte calibration block response |

### Read Protocol (Polled)

Calibration block reads use a three-step polled protocol:

1. **Issue command**: `WriteI2C(0xC0, 0x08, [cmd_data])`
2. **Poll for data**: `ReadI2C(0xC0, response_reg, 0)` -- repeat until
   status byte != 0x01
   - Status 0x01 = not ready (retry)
   - Status 0x02 = data ready (remaining bytes are the response)
3. **Acknowledge**: `WriteI2C(0xC0, 0x01, [])`

Reads typically complete within 2-6 polls. Maximum 20 attempts before
timeout.

### Phase 6 Initialization Sequence

From pcap frames 1651-1752:

1. Read device ID (register 0x04) -- returns 5 bytes
2. Read 35-byte calibration block (cmd to 0x08, poll 0x23)
3. Read device ID again (integrity check)
4. Write 26-byte calibration parameters (register 0x1C)
5. Read updated response from 35-byte register (poll 0x23)
6. Acknowledge (register 0x01)
7. Read device ID again
8. Read 67-byte calibration block (cmd to 0x08, poll 0x43)

Observed device ID: `[0x02, 0x04, 0x11, 0x33, 0x43]`.

Rust implementation: `eeprom_read_full_calibration()` in
`crates/saleae-driver/src/eeprom.rs`.


## EEPROM Self-Test

Decompiled from `LogicProfessionalDevice::TestEeprom` (ELF VA 0x1c44010,
source: `LogicProfessionalDevice.cpp` line 84).

This is a destructive manufacturing diagnostic that:

1. Writes an incrementing byte pattern (0x00, 0x01, ..., wrapping) to
   addresses 0x0008 through 0x0FFF
2. Reads back the entire 4096 bytes
3. Verifies each byte from offset 8 onward matches the expected pattern

Bytes 0x0000-0x0007 (bootloader header + VID/PID) are preserved. All
other data (serial, calibration coefficients, hardware revision) is
destroyed. The device requires re-calibration after this test.

Rust implementation: `eeprom_self_test()` in
`crates/saleae-driver/src/eeprom.rs`.


## Cross-References

- `doc/calibration/calibration-format.md` -- .cal file format and server download
- `doc/protocol/fpga-registers.md` -- FPGA register 0x17 (CAL_COEFF_REG)
- `doc/protocol/ep1-commands.md` -- ReadVersion (0x07), WriteI2C (0x87), ReadI2C (0x88)
- `doc/protocol/fpga-bitstream.md` -- bitstream selection by hardware revision
- `crates/saleae-proto/src/device.rs` -- `HardwareRevision`, `FpgaVariant`, `AdcType`
- `crates/saleae-proto/src/fpga_regs.rs` -- `CAL_COEFF_REG`, `CAL_COEFF_MAX_COUNT`
- `crates/saleae-driver/src/eeprom.rs` -- EEPROM read/write operations
- `crates/saleae-driver/src/ep1.rs` -- `read_eeprom()`, `write_eeprom()`
- `crates/saleae-driver/src/jesd.rs` -- `write_calibration_coefficients()`
