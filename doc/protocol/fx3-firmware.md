# FX3 Firmware Upload Protocol

**Confidence: High** (binary analysis of embedded firmware images, confirmed
against documented Cypress FX3 bootloader protocol, USB descriptor
structures decoded)


## Overview

Saleae Logic Pro 8 and Logic Pro 16 use a Cypress EZ-USB FX3 controller.
On connection, the FX3 is in bootloader mode. The host uploads firmware via
USB vendor control transfers. The firmware runs from volatile RAM only --
no flash or EEPROM writes occur during normal firmware upload.


## Firmware Images

Two firmware images are embedded in `libgraph_server_shared.so`, back to
back with zero gap:

| Device         | PID    | Offset in .so | Size (bytes) | Compile Date                      |
|----------------|--------|---------------|--------------|-----------------------------------|
| Logic Pro 8    | 0x1005 | 0x0079e7a0    | 119,824      | Oct 25 2022 16:42:00              |
| Logic Pro 16   | 0x1006 | 0x007bbbb0    | 119,824      | Oct 25 2022 16:42:01              |

The images are functionally identical. Only 7 bytes differ: the USB PID in
two device descriptors, a compile timestamp character, the USB configuration
power budget (124mA vs 200mA), and the image checksum.

Each firmware contains dual USB device descriptors for USB 2.0 (bcdUSB=0x0210)
and USB 3.0 (bcdUSB=0x0300).


## Image Format

Standard Cypress FX3 boot image format (AN76405):

### Header (4 bytes)

| Offset | Size | Field      | Value |
|--------|------|------------|-------|
| 0      | 1    | Signature  | 'C' (0x43) |
| 1      | 1    | Signature  | 'Y' (0x59) |
| 2      | 1    | image_ctl  | 0x1c (SPI boot, checksum enabled) |
| 3      | 1    | image_type | 0xB0 (normal firmware) |

### Sections (repeating)

| Offset | Size | Field    | Description |
|--------|------|----------|-------------|
| 0      | 4    | dLength  | Data length in 32-bit words (0 = end marker) |
| 4      | 4    | dAddress | Load address in FX3 memory map |
| 8      | N*4  | dData    | Code/data payload |

### Terminator

| Offset | Size | Field      | Description |
|--------|------|------------|-------------|
| 0      | 4    | 0x00000000 | Zero length = end of sections |
| 4      | 4    | entry_addr | Program entry point (0x4000e020) |
| 8      | 4    | checksum   | 32-bit image checksum |

### Memory Layout

| Section | Bytes  | Address    | Region |
|---------|--------|------------|--------|
| 1       | 9,088  | 0x00000100 | ITCM (volatile instruction memory) |
| 2       | 65,536 | 0x40003000 | SYSMEM (volatile system RAM) |
| 3       | 37,484 | 0x40013000 | SYSMEM |
| 4       | 7,668  | 0x40030000 | SYSMEM |

Entry point: 0x4000e020 (SYSMEM)

All addresses are volatile RAM. Power cycle restores bootloader.


## USB Control Transfer Protocol

| Field         | Value | Description |
|---------------|-------|-------------|
| bmRequestType | 0x40  | Host-to-Device, Vendor, Device |
| bRequest      | 0xA0  | Cypress FW_UPLOAD / RW_INTERNAL |
| wValue        | addr & 0xFFFF | Lower 16 bits of target address |
| wIndex        | addr >> 16    | Upper 16 bits of target address |
| wLength       | 1-4096 | Chunk size (max 4096 bytes) |

### Upload Sequence

1. Parse CY header, validate signature == "CY" and image_type == 0xB0
2. For each section: read dLength, dAddress, then send dData in 4KB chunks
3. On terminal section (dLength=0): send 0-byte transfer to entry_addr
4. FX3 executes firmware, re-enumerates with Saleae VID:PID
5. Host verifies 4 USB endpoints present


## Device Type Mapping

| VID    | PID    | Type ID | Device |
|--------|--------|---------|--------|
| 0x0925 | 0x3881 | 1       | Logic (original, FX2) |
| 0x21a9 | 0x1001 | 4       | Logic 16 (legacy, FX2LP) |
| 0x21a9 | 0x1003 | 5       | Logic 4 (FX3) |
| 0x21a9 | 0x1004 | 6       | Logic 8 (FX3) |
| 0x21a9 | 0x1005 | 7       | Logic Pro 8 (FX3) |
| 0x21a9 | 0x1006 | 12      | Logic Pro 16 (FX3) |

FX2-based devices use Intel HEX firmware images (4 images embedded, 5-11KB
each). FX3-based devices use CY boot images. The exact firmware selection
for Logic 4 and Logic 8 (non-Pro FX3 devices) needs Ghidra confirmation.


## Safety Assessment

**RAM-only upload -- SAFE for hardware testing.**

All firmware sections target ITCM (0x00000100) and SYSMEM (0x40003000+).
These are volatile memories on the FX3. A USB disconnect or power cycle
returns the device to its bootloader state.

The MSO device (`MsoSpiFlash::WriteFirmware`) is a different and more
dangerous path -- it writes to non-volatile SPI flash. That device type
is out of scope for this analysis.


## Cross-References

- `doc/protocol/ep1-commands.md` -- EP1 command protocol (available after firmware upload)
- `doc/protocol/fpga-bitstream.md` -- FPGA programming (next step after firmware upload)
- `doc/protocol/eeprom.md` -- EEPROM layout (read after firmware upload)
- `crates/saleae-proto/src/firmware/fx3.rs` -- FX3 firmware image parser
- `crates/saleae-driver/src/firmware.rs` -- firmware upload implementation
