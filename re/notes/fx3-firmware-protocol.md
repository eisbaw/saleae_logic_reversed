# FX3 Firmware Upload Protocol Analysis

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40


## Summary

The Saleae Logic 2 application embeds Cypress FX3 firmware images directly
inside `libgraph_server_shared.so` and uploads them to the FX3 controller
via the standard Cypress USB bootloader protocol (vendor control transfers
with bRequest=0xA0). All firmware targets volatile RAM only -- no flash
writes occur. Power cycling a device restores it to its bootloader state.


## Embedded Firmware Images

### FX3 Firmware (Cypress .img format) -- 2 images

Both images share identical structure and differ in only 7 bytes out of
119,824 bytes. The differences are:

| Firmware | Offset in .so  | PID    | Compile Date                              | Checksum   |
|----------|----------------|--------|-------------------------------------------|------------|
| FW1      | 0x0079e7a0     | 0x1005 | Logic Pro Firmware Compiled Oct 25 2022  16:42:00 | 0x7da4aede |
| FW2      | 0x007bbbb0     | 0x1006 | Logic Pro Firmware Compiled Oct 25 2022  16:42:01 | 0x7da6af05 |

FW1 = Logic Pro 8 (21a9:1005)
FW2 = Logic Pro 16 (21a9:1006)

The images are contiguous (FW2 immediately follows FW1 with zero gap).
After FW2, a region of 0xFF bytes (flash padding) follows.

A B2 wrapper header precedes FW1 at offset 0x0079e78f (17 bytes before
the B0 image). The B2 wrapper contains config for SPI/I2C boot path and
appears to encode Saleae's USB VID (0x21a9) in its data field.


### Intel HEX Firmware (FX2/FX2LP) -- 4 images

These are firmware images for the legacy FX2-based devices (Original Logic,
Logic 16). They target the 8051 instruction set and are much smaller.

| Group | Offset in .so  | Size in .so | Data bytes | Address range  |
|-------|----------------|-------------|------------|----------------|
| 1     | 0x0037e0ac     | 16,397      | 5,565      | 0x0000-0x15bd  |
| 2     | 0x0039743d     | 25,991      | 10,794     | 0x0000-0x2edb  |
| 3     | 0x003ac65e     | 15,581      | 5,217      | 0x0000-0x1461  |
| 4     | 0x003cb8cd     | 23,539      | 8,152      | 0x0000-0x1fd8  |

All four end with the standard Intel HEX EOF record (`:00000001FF`).

The exact mapping of IHex groups to devices (Logic original, Logic 16, etc.)
has not yet been confirmed. The `IHexFirmwareImageDescriptor` variant in
the code handles these images through a different upload path than FX3.


## FX3 Firmware Image Format (Confirmed)

The firmware uses standard Cypress FX3 boot image format (AN76405).

### B2 Wrapper Header (SPI/I2C boot config)

```
Offset  Size  Value       Description
0       1     0x43 ('C')  Signature byte 1
1       1     0x59 ('Y')  Signature byte 2
2       1     0x1c        image_ctl:
                            bit0=0: I2C page size = 256 bytes
                            bit2=1: I2C/SPI config
                            bit3=1: SPI boot mode
                            bit4=1: Checksum enabled
3       1     0xb2        image_type = data/config
4-7     4     0x21a91007  Config data (contains VID 0x21a9)
8-16    9     0x00...     Zero padding
```

### B0 Firmware Image Header

```
Offset  Size  Value       Description
0       1     0x43 ('C')  Signature byte 1
1       1     0x59 ('Y')  Signature byte 2
2       1     0x1c        image_ctl (same as B2)
3       1     0xb0        image_type = normal firmware boot
```

### B0 Section Format

After the 4-byte header, repeating sections follow:

```
Offset  Size  Description
0       4     dLength -- data length in 32-bit WORDS (multiply by 4 for bytes)
4       4     dAddress -- load address in FX3 memory space
8       N*4   dData -- the actual code/data bytes
```

A section with dLength=0 terminates the image:

```
0       4     0x00000000  -- zero length signals end of sections
4       4     entry_addr  -- program entry point address
8       4     checksum    -- 32-bit image checksum (if enabled in image_ctl)
```

### Section Layout (both FW1 and FW2)

| Section | Length (words) | Bytes  | Load Address | Memory Region |
|---------|---------------|--------|--------------|---------------|
| 1       | 2,272         | 9,088  | 0x00000100   | ITCM (Instruction Tightly Coupled Memory) |
| 2       | 16,384        | 65,536 | 0x40003000   | SYSMEM (System RAM) |
| 3       | 9,371         | 37,484 | 0x40013000   | SYSMEM (System RAM) |
| 4       | 1,917         | 7,668  | 0x40030000   | SYSMEM (System RAM) |
| (end)   | 0             | --     | 0x4000e020   | Entry point in SYSMEM |

Total code/data: 119,776 bytes
Total image size: 119,824 bytes (with headers and checksums)


## FX3 Memory Map

```
0x00000000 - 0x00003FFF  ITCM (16KB, volatile, instruction cache)
0x40000000 - 0x4007FFFF  SYSMEM (up to 512KB, volatile system RAM)
```

All sections target ITCM or SYSMEM. This is volatile RAM -- data is lost
on power cycle. NO flash or EEPROM addresses are targeted.

**SAFETY CONFIRMED: Firmware upload is RAM-only. Power cycle recovers.**


## USB Firmware Upload Protocol

### Standard Cypress FX3 Bootloader Protocol

The FX3 bootloader uses a well-documented USB vendor control transfer
protocol for firmware upload:

```
bmRequestType = 0x40 (Host-to-Device, Vendor, Device)
bRequest      = 0xA0 (Cypress FW_UPLOAD / RW_INTERNAL)
wValue        = address[15:0]  (lower 16 bits of target address)
wIndex        = address[31:16] (upper 16 bits of target address)
wLength       = data_length    (max 4096 bytes per transfer)
```

### Upload Sequence

1. Host reads the .img file (embedded in the .so as raw bytes)
2. Host validates the "CY" signature (bytes 0-1)
3. Host parses image_ctl and image_type from bytes 2-3
4. For each section:
   a. Read dLength (32-bit words) and dAddress
   b. Send data in chunks of up to 4096 bytes via vendor control transfers
   c. Each chunk: bmRequestType=0x40, bRequest=0xA0,
      wValue=addr_low, wIndex=addr_high, data=chunk
5. When dLength=0 is reached, the entry address follows
6. Host sends a 0-byte control transfer to the entry address
   (this triggers the FX3 to jump to the firmware entry point)
7. The FX3 re-enumerates with the VID:PID from the firmware's USB descriptors

### Saleae Implementation Specifics

From string analysis, the Saleae implementation:

- Uses `UsbDevice::DownloadFirmware(const Saleae::DeviceFirmwareImage &)`
  as the entry point
- Calls `Saleae::ParseFirmwareImage(gsl::span<const uint8_t>)` to validate
  the image
- Validates signature: `signature[0] == 'C' && signature[1] == 'Y'`
- Checks `image_type == IMAGE_TYPE_NORMAL` (0xB0)
- Uses `LinuxUsbDevice::SendControlTransfer(const UsbControlSetupPacket &, ...)`
  as the USB transport wrapper
- Selects firmware via `UsbDevice::GetFirmwareForDeviceType(::DeviceType)`
- Supports custom firmware: `"Loading custom firmware image to FX3: {}"`
- Measures upload time: `"Firmware download took "`
- Verifies 4 endpoints after firmware runs: `"Endpoint count is not 4.
  The device is likely programmed with firmware from a different version
  and must be reconnected."`


## Firmware Differences Between FW1 and FW2

Only 7 bytes differ between the Logic Pro 8 and Logic Pro 16 firmware:

| Offset | FW1 (Pro 8) | FW2 (Pro 16) | Meaning |
|--------|-------------|--------------|---------|
| 31640  | 0x30 ('0')  | 0x31 ('1')   | Compile timestamp: 16:42:0**0** vs 16:42:0**1** |
| 33660  | 0x3e (62)   | 0x64 (100)   | USB config descriptor bMaxPower: 124mA vs 200mA |
| 33790  | 0x05        | 0x06         | USB device descriptor PID low byte: 10**05** vs 10**06** |
| 33822  | 0x05        | 0x06         | USB device descriptor PID low byte (second descriptor) |
| 119820 | 0xde        | 0x05         | Image checksum byte 0 |
| 119821 | 0xae        | 0xaf         | Image checksum byte 1 |
| 119822 | 0xa4        | 0xa6         | Image checksum byte 2 |

The firmware is functionally identical. The only runtime differences are the
USB PID (which endpoint the device enumerates with) and the power budget
(Logic Pro 16 draws more current at 200mA vs 124mA for Pro 8).

Each firmware contains two USB device descriptors:
- One for USB 2.0 (bcdUSB=0x0210) with PID 0x1005 or 0x1006
- One for USB 3.0 (bcdUSB=0x0300) with PID 0x1005 or 0x1006


## Device Type Enumeration

A VID:PID lookup table was found at offset 0x004718e4.
**Note:** The "DeviceType" column below originally listed the *internal*
(compact) enum values.  The correct *external* enum values are documented
in `re/notes/device-type-enum.md` and are what `GetVidPidForDevice` and
the calibration server use.

| VID    | PID    | External DeviceType | Internal DeviceType | Device Name |
|--------|--------|---------------------|---------------------|-------------|
| 0x0925 | 0x3881 | 0                   | 1                   | Logic (original, FX2) |
| 0x21a9 | 0x1001 | 1                   | 2                   | Logic 16 (legacy, FX2LP) |
| 0x21a9 | 0x1003 | 4                   | 3                   | Logic 4 (FX3) |
| 0x21a9 | 0x1004 | 5                   | 4                   | Logic 8 (FX3) |
| 0x21a9 | 0x1005 | 6                   | 5                   | Logic Pro 8 (FX3) |
| 0x21a9 | 0x1006 | 7                   | 6                   | Logic Pro 16 (FX3) |
| 0x21a9 | 0x1007 | 12                  | 7                   | Mso |

Note: Logic 4 (0x1003) and Logic 8 (0x1004) are FX3-based but we found
only 2 CY firmware images (for Pro 8 and Pro 16). The non-Pro FX3 devices
may share firmware with the Pro devices (with PID patched at runtime via
the reseed mechanism), or `GetFirmwareForDeviceType` may map them to one
of the two embedded images. This needs Ghidra confirmation.


## Firmware Image Type System

The code supports two firmware image variants via a C++ std::variant:

```cpp
using DeviceFirmwareImage = std::variant<
    Saleae::FirmwareImageDescriptor,       // FX3 CY format
    Saleae::IHexFirmwareImageDescriptor    // FX2 Intel HEX format
>;
```

Selection logic (from assertion strings):
- `holds_alternative<FirmwareImageDescriptor>(image)` -- FX3 path
- `holds_alternative<IHexFirmwareImageDescriptor>(image)` -- FX2 path

The `ParseFirmwareImage(gsl::span<const uint8_t>)` function determines
image type and returns the appropriate descriptor.


## Device Reseed Mechanism

String evidence of a "reseed" operation:

- `virtual void LinuxUsbDevice::ReseedDevice(UsbEndpoint &)`
- `libusb_bulk_transfer failure (reseed)`
- `TestFirmwareResponsivnessAndReseed`
- `Can't reset the device seed and download the firmware in one command`

The reseed appears to be a mechanism to change the device's identity or
configuration after firmware upload, possibly via a bulk endpoint. The
constraint "Can't reset the device seed and download the firmware in one
command" suggests these are mutually exclusive operations in a single
device session. This needs further investigation.


## FX3 Firmware Strings (embedded in FW images)

The firmware images contain Cypress FX3 USB event handler strings:

```
CY_U3P_USB_EVENT_SETCONF. data:%d
CY_U3P_USB_EVENT_RESET, data:%d
CY_U3P_USB_EVENT_RESET / CY_U3P_USB_EVENT_DISCONNECT
CY_U3P_USB_EVENT_SETINTF. data:%d
CY_U3P_USB_EVENT_SUSPEND
CY_U3P_USB_EVENT_RESUME
CY_U3P_USB_EVENT_CONNECT
CY_U3P_USB_EVENT_SET_SEL
CY_U3P_USB_EVENT_EP0_STAT_CPLT
CY_U3P_USB_EVENT_SPEED
```

And application-level strings:

```
@CYWBFX3B              (Cypress Wireless Boot FX3B marker)
LogicProEventLoop
LogicPro.c
LogicProThread_Entry
LogicProApplicationInit
```

The source filename `LogicPro.c` confirms the firmware is custom Saleae
code running on the FX3 ARM926EJ-S processor, implementing the USB
endpoints and FPGA interface logic.


## Boot Sequence (Reconstructed)

1. Device powers on with FX3 in bootloader mode (VID:PID not yet Saleae)
2. Host detects device via libusb hotplug callback
3. Host calls `GetFirmwareForDeviceType(device_type)` to select firmware
4. Host calls `DownloadFirmware(firmware_image)`:
   a. ParseFirmwareImage validates CY header
   b. Sections uploaded via vendor control transfers (0x40/0xA0)
   c. Entry point transfer triggers firmware execution
   d. Timing logged: "Firmware download took ..."
5. FX3 re-enumerates with Saleae VID:PID (21a9:1005 or 21a9:1006)
6. Host verifies 4 endpoints present
7. Host proceeds to FPGA bitstream programming
8. Device ready for capture


## Open Questions

1. **Logic 4 / Logic 8 firmware**: Do these share one of the Pro firmware
   images? Or are there additional firmware images stored elsewhere that
   we haven't found? The `GetFirmwareForDeviceType` function needs Ghidra
   analysis to confirm the mapping.

2. **Reseed mechanism**: What exactly does `ReseedDevice` do? Is it
   related to serial number assignment, or device identity after firmware
   upload? The bulk endpoint usage and the "seed" terminology are unclear.

3. **FX2 firmware mapping**: Which of the 4 Intel HEX images goes to
   which legacy device? The `IHexFirmwareImageDescriptor` path needs
   tracing.

4. **Custom firmware path**: The "Loading custom firmware image to FX3: {}"
   string suggests a development/debug feature for loading arbitrary
   firmware. What triggers it?

5. **Firmware version checking**: The `GetFirmwareVersion` and
   `IsFirmwareDownloaded` functions suggest the host can query what
   firmware is already running. What vendor request is used for this?


## References

- Cypress AN76405: EZ-USB FX3/FX3S Boot Options
  (documents the CY image format and USB bootloader protocol)
- libusb ezusb.c: Reference implementation of FX3 firmware upload
  https://github.com/libusb/libusb/blob/master/examples/ezusb.c
- Cypress FX3 SDK: firmware/u3p_firmware/inc/cyfxapidesc.h
  (USB event handler API that matches the embedded strings)
