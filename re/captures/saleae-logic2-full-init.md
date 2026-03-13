# saleae-logic2-full-init.pcap Analysis

Captured: ~March 2026
Duration: 67.4 seconds
Total packets: 802
Capture type: usbmon (Linux USB monitor)

## Overview

This capture records a **full Saleae Logic 2 application startup and device initialization**
for a Logic Pro 8 (VID:PID 21a9:1005). It covers the USB bus from cold start through
firmware upload, but does **not** include any EP1 command traffic or EP2 streaming data.

The capture ends after firmware upload completes -- the post-firmware device initialization
(FPGA programming, register configuration, capture start) is NOT present.

## Devices on the Bus

| USB Addr | VID:PID   | Device              | Packets | Notes                     |
|----------|-----------|---------------------|---------|---------------------------|
| 0        | 21a9:1005 | Saleae (hub enum)   | 2       | Hub-level descriptor read |
| 1        | 1d6b:0002 | Linux USB root hub  | 102     | Hub enumeration           |
| 2        | 1050:0407 | Yubico YubiKey      | 6       | Descriptors only          |
| 4        | 30c9:005f | Luxvisions webcam   | 8       | Descriptors only          |
| 6        | 8087:0036 | Intel Bluetooth     | 542     | HCI bulk traffic          |
| 7        | 06cb:0123 | Synaptics touchpad  | 56      | HID interrupt traffic     |
| 9        | 21a9:1005 | Saleae (bootloader) | 6       | First enumeration         |
| 10       | 21a9:1005 | Saleae (fw upload)  | 80      | Re-enum + firmware upload |

## Saleae-Specific Timeline

### Phase 1: Bootloader Enumeration (t=0.1s, device addr 9)

Device 9 appears at t=0.1s with:
- VID:PID = 21a9:1005, bcdDevice = 1.00
- iProduct string index = 2
- 3 GET_DESCRIPTOR requests (device, config, device again)

This is the **B2 bootloader** mode -- the FX3 has no firmware loaded, but the B2 bootloader
reads VID/PID from EEPROM bytes 0x0004-0x0007, so it already reports the Saleae VID/PID.

### Phase 2: Application Startup Gap (t=0.1s to t=17.0s)

**16.9 seconds of silence** on the USB bus for the Saleae device.
This is the Saleae Logic 2 Electron app starting up, loading JavaScript, initializing
the graph server, and preparing to interact with the device.

### Phase 3: Re-enumeration (t=17.0s, device addr 10)

Device re-enumerates as address 10 (same VID/PID). The host reads:
- Device descriptor (18 bytes)
- Configuration descriptor (9 bytes)
- Device descriptor again (18 bytes)
- 4 string descriptors (4, 24, 16, 26 bytes)
- SET_CONFIGURATION (bRequest=9, wValue=0, wIndex=0)

### Phase 4: Pre-Upload Gap (t=17.0s to t=32.0s)

**15 seconds of silence** after enumeration before firmware upload begins.
The application is likely:
1. Determining hardware revision from EEPROM
2. Selecting the correct firmware image
3. Possibly downloading calibration data from Saleae servers (correlates with
   the parallel network capture in saleae-all-usb.pcap)

### Phase 5: FX3 Firmware Upload (t=32.0s to t=32.1s)

Firmware uploaded via **vendor control transfers** (bRequest=0xA0) in 31 chunks:

| Memory Region     | Address Range               | Size    | Chunks |
|-------------------|-----------------------------|---------|--------|
| I-TCM (boot code) | 0x00000100 - 0x00002480     | 9,088 B | 3      |
| System RAM seg 1  | 0x40003000 - 0x40010000     | 53,248 B| 13     |
| System RAM seg 2  | 0x40010000 - 0x4001C26C     | 49,772 B| 13     |
| System RAM seg 3  | 0x40030000 - 0x40031DF4     | 7,668 B | 2      |

- **Total firmware size: 119,776 bytes (117.0 KB)**
- **Chunk size: 4096 bytes** (except last chunk in each region)
- **Entry point: 0x4000E020** (jump command with 0 data bytes)
- **Upload duration: ~80 ms** (frames 381-444, t=32.037 to t=32.088)

The firmware upload protocol uses the standard Cypress FX3 RAM download:
- bmRequestType = 0x40 (vendor, host-to-device)
- bRequest = 0xA0
- wValue = address low 16 bits
- wIndex = address high 16 bits
- wLength = data length (0 for jump command)

## What This Capture Does NOT Contain

- No EP1 bulk OUT/IN traffic (command/response protocol)
- No EP2 bulk IN traffic (streaming sample data)
- No FPGA bitstream programming
- No EEPROM reads via EP1
- No capture start/stop sequences

This capture only covers the initial USB enumeration and FX3 firmware RAM download.
For EP1 command traces, see `usbmon4-ep1-decoded.txt` and `ep1-commands-extracted.txt`.

## Cross-Reference: Firmware Image File

The extracted firmware at `firmware/fw-fx3-pro8.img` is 119,824 bytes.
The pcap shows 119,776 bytes of USB payload data. The 48-byte difference is the
FX3 .img file header (containing section descriptors, entry point, and checksum)
which is parsed by host software but not sent over USB. This confirms the pcap
captures the complete firmware upload for a Logic Pro 8.

## FX3 Address Space Notes

- 0x00000000 - 0x00003FFF: I-TCM (Instruction Tightly Coupled Memory, 16 KB)
- 0x40000000 - 0x4007FFFF: System RAM (512 KB on FX3)
- 0x40030000 region: likely DMA descriptors or interrupt vectors
- Entry point 0x4000E020 is in the middle of System RAM segment 1
