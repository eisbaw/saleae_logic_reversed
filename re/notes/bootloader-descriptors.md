# FX3 Bootloader / SPI Flash Boot State Descriptors

Investigation date: 2026-03-09
Task: TASK-0033
Hardware: Logic Pro 8 (21a9:1005) on nvdk-msdlab-09


## Summary

The FX3 does NOT expose the standard Cypress bootloader VID:PID (04b4:00f3)
on power-up. Instead, it boots from SPI flash into a minimal "WestBridge"
firmware that immediately enumerates as 21a9:1005 (Saleae VID:PID). The
Saleae host software then uploads the full firmware via vendor control
transfers, causing the device to disconnect and re-enumerate at USB 3.0
SuperSpeed with complete Saleae descriptors.

This means:
1. There is NO window where the device appears as a generic Cypress FX3
   bootloader (04b4:00f3)
2. The SPI flash contains a B2+B0 boot image that sets VID:PID to 21a9:1005
3. The SPI flash firmware provides only USB 2.0 High Speed (no SuperSpeed)
4. The full Saleae firmware upload enables USB 3.0 SuperSpeed


## Two-Stage Boot Process (from dmesg)

Kernel log from nvdk-msdlab-09 shows the complete boot sequence:

### Stage 1: SPI Flash Boot (power-on / USB plug-in)

```
[Wed Mar  4 11:25:09 2026] usb 1-10: new high-speed USB device number 11 using xhci_hcd
[Wed Mar  4 11:25:09 2026] usb 1-10: New USB device found, idVendor=21a9, idProduct=1005, bcdDevice= 1.00
[Wed Mar  4 11:25:09 2026] usb 1-10: New USB device strings: Mfr=1, Product=2, SerialNumber=3
[Wed Mar  4 11:25:09 2026] usb 1-10: Product: WestBridge
[Wed Mar  4 11:25:09 2026] usb 1-10: Manufacturer: Cypress
[Wed Mar  4 11:25:09 2026] usb 1-10: SerialNumber: 0000000004BE
```

Key observations:
- VID:PID = 21a9:1005 (Saleae, NOT Cypress default 04b4:00f3)
- bcdDevice = 1.00 (SPI flash firmware version indicator)
- Product = "WestBridge" (Cypress FX3 SDK codename for the USB controller)
- Manufacturer = "Cypress" (default, not "Saleae")
- SerialNumber = 0000000004BE (real hardware serial, NOT "SERNUM")
- USB 2.0 High Speed (bus 1 = xHCI USB 2.0 companion controller)
- iSerialNumber = 3 (descriptor string index 3)

### Stage 2: Saleae Firmware Upload + Re-enumeration

```
[Wed Mar  4 11:35:16 2026] usb 1-10: USB disconnect, device number 11
[Wed Mar  4 11:35:17 2026] usb 2-3: new SuperSpeed USB device number 3 using xhci_hcd
[Wed Mar  4 11:35:17 2026] usb 2-3: New USB device found, idVendor=21a9, idProduct=1005, bcdDevice= 0.00
[Wed Mar  4 11:35:17 2026] usb 2-3: New USB device strings: Mfr=1, Product=2, SerialNumber=4
[Wed Mar  4 11:35:17 2026] usb 2-3: Product: Logic Pro
[Wed Mar  4 11:35:17 2026] usb 2-3: Manufacturer: Saleae
[Wed Mar  4 11:35:17 2026] usb 2-3: SerialNumber: SERNUM
```

Key observations:
- VID:PID = 21a9:1005 (same VID:PID as SPI boot state)
- bcdDevice = 0.00 (full firmware version indicator)
- Product = "Logic Pro" (Saleae product name)
- Manufacturer = "Saleae" (updated from "Cypress")
- SerialNumber = "SERNUM" (generic label, NOT the real serial)
- USB 3.0 SuperSpeed (bus 2 = xHCI USB 3.0 root hub)
- iSerialNumber = 4 (descriptor string index 4, different from SPI boot)
- Elapsed time: ~10 minutes between plug-in and firmware upload
  (Saleae Logic 2 was likely started manually in between)

### Firmware Upload Triggers Re-enumeration

The disconnect on bus 1 (USB 2.0) immediately followed by connect on bus 2
(USB 3.0) shows that:
1. The SPI flash firmware only supports USB 2.0 (no SuperSpeed capability)
2. The firmware upload causes a FX3 soft-reset
3. The new firmware enables USB 3.0 SuperSpeed
4. The kernel sees this as a new device on a different bus


## SPI Flash Boot Image

The B2 wrapper header found at offset 0x0079E78F in libgraph_server_shared.so
corresponds to the SPI flash boot configuration:

```
Offset  Value       Description
0       0x43 'C'    Signature byte 1
1       0x59 'Y'    Signature byte 2
2       0x1c        image_ctl: SPI boot mode, checksum enabled
3       0xB2        image_type: data/config (B2 = SPI/I2C config wrapper)
4-7     0x21a91007  Config: VID 0x21A9 encoded in data field
```

The B2 header confirms:
- SPI boot mode is configured (bit 3 of image_ctl)
- Saleae VID (0x21A9) is programmed into the SPI flash
- The device boots directly with Saleae VID:PID, bypassing Cypress defaults

The SPI flash likely contains a minimal FX3 firmware that:
1. Initializes the USB 2.0 controller
2. Exposes EP0 for vendor control transfers (firmware upload)
3. Sets VID:PID to 21a9:1005 with "WestBridge" / "Cypress" strings
4. Reports the real hardware serial number (0000000004BE)
5. Waits for the host to upload the full firmware via bRequest=0xA0


## Descriptor Comparison

| Field            | SPI Flash Boot        | After Saleae FW Upload    |
|------------------|-----------------------|---------------------------|
| VID              | 0x21a9                | 0x21a9                    |
| PID              | 0x1005                | 0x1005                    |
| bcdDevice        | 1.00                  | 0.00                      |
| bcdUSB           | 2.00 (High Speed)     | 3.00 (SuperSpeed)         |
| Manufacturer     | Cypress               | Saleae                    |
| Product          | WestBridge            | Logic Pro                 |
| SerialNumber     | 0000000004BE          | SERNUM                    |
| iSerialNumber    | 3                     | 4                         |
| USB Speed        | High Speed (480 Mbps) | SuperSpeed (5 Gbps)       |
| Endpoints        | Unknown (likely EP0)  | EP0 + 3 bulk (EP1O/I, EP2I) |
| bMaxPacketSize0  | Unknown               | 9 (512 for USB 3.0)       |
| MaxPower         | Unknown               | 496 mA                    |
| Bus              | Bus 1 (USB 2.0)       | Bus 2 (USB 3.0)           |


## Implications for Rust Driver

1. **Device detection must handle both states**: The Rust driver must
   recognize 21a9:1005 in BOTH the SPI flash state (bcdDevice=1.00,
   USB 2.0) and the full firmware state (bcdDevice=0.00, USB 3.0).

2. **bcdDevice distinguishes boot states**: bcdDevice=1.00 indicates
   SPI flash boot (needs firmware upload). bcdDevice=0.00 indicates
   full firmware is running. This is a reliable discriminator.

3. **No Cypress bootloader handling needed**: The standard Cypress
   bootloader VID:PID (04b4:00f3) is never exposed. The SPI flash
   ensures the device always appears as a Saleae device from the
   kernel's perspective.

4. **Real serial number is available early**: The SPI flash firmware
   exposes the real hardware serial (0000000004BE) at iSerialNumber=3.
   After firmware upload, this is replaced with the generic "SERNUM"
   label at iSerialNumber=4. The real serial may need to be captured
   before firmware upload for device identification.

5. **Firmware upload over USB 2.0**: Since the SPI flash firmware only
   supports High Speed (480 Mbps), the vendor control transfer firmware
   upload runs at USB 2.0 speed. This is fine since the FX3 firmware
   images are only ~120 KB.


## What We Could NOT Capture

We could not capture the full `lsusb -v` output for the SPI flash boot
state because:

1. The device is currently running full Saleae firmware (bcdDevice=0.00)
2. Returning to SPI flash boot state requires physical power cycling
   (USB unplug/replug)
3. The device is directly connected to the xHCI root hub (USB port 2-3)
   which does NOT support per-port power switching via uhubctl
4. USB bus reset (USBDEVFS_RESET ioctl) resets the USB protocol state
   but does NOT cut VBUS power, so the FX3 RAM firmware survives
5. sysfs deauthorize/reauthorize also does not cut VBUS power
6. No uhubctl, usbreset, or usb_modeswitch tools are installed on the
   system (uhubctl was tested via nix-shell but root hub is unsupported)

To capture full SPI flash boot descriptors, we would need:
- Physical access to unplug/replug the device
- OR a programmable USB hub between the root hub and the device
- OR a USB relay/switch controlled remotely


## Verification Methods Used

1. `ssh nvdk-msdlab-09 'lsusb -d 21a9:'` -- confirmed device at 21a9:1005
2. `ssh nvdk-msdlab-09 'lsusb -d 04b4:'` -- confirmed NO Cypress bootloader
3. `ssh nvdk-msdlab-09 'lsusb'` -- full device listing
4. `ssh nvdk-msdlab-09 'pgrep -la Logic'` -- confirmed Saleae not running
5. `ssh nvdk-msdlab-09 'sudo lsusb -v -d 21a9:1005'` -- full descriptors
6. `ssh nvdk-msdlab-09 'sudo dmesg -T | grep ...'` -- boot sequence history
7. sysfs authorized/unbind/bind -- attempted USB disconnect
8. USBDEVFS_RESET ioctl -- attempted USB bus reset
9. uhubctl via nix-shell -- attempted per-port power control (unsupported)

## Post-Firmware USB Descriptors (Current State)

Full `lsusb -v` output for the device in its current (firmware loaded) state:

```
Bus 002 Device 003: ID 21a9:1005 Saleae, Inc. Logic Pro 8
Device Descriptor:
  bLength                18
  bDescriptorType         1
  bcdUSB               3.00
  bDeviceClass            0
  bDeviceSubClass         0
  bDeviceProtocol         0
  bMaxPacketSize0         9
  idVendor           0x21a9 Saleae, Inc.
  idProduct          0x1005 Logic Pro 8
  bcdDevice            0.00
  iManufacturer           1 Saleae
  iProduct                2 Logic Pro
  iSerial                 4 SERNUM
  bNumConfigurations      1
  Configuration Descriptor:
    bLength                 9
    bDescriptorType         2
    wTotalLength       0x0039
    bNumInterfaces          1
    bConfigurationValue     1
    iConfiguration          0
    bmAttributes         0x80
      (Bus Powered)
    MaxPower              496mA
    Interface Descriptor:
      bLength                 9
      bDescriptorType         4
      bInterfaceNumber        0
      bAlternateSetting       0
      bNumEndpoints           3
      bInterfaceClass       255 Vendor Specific Class
      bInterfaceSubClass      0
      bInterfaceProtocol      0
      iInterface              0
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x01  EP 1 OUT
        bmAttributes            2
          Transfer Type            Bulk
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0400  1x 1024 bytes
        bInterval               0
        bMaxBurst               0
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x81  EP 1 IN
        bmAttributes            2
          Transfer Type            Bulk
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0400  1x 1024 bytes
        bInterval               0
        bMaxBurst               0
      Endpoint Descriptor:
        bLength                 7
        bDescriptorType         5
        bEndpointAddress     0x82  EP 2 IN
        bmAttributes            2
          Transfer Type            Bulk
          Synch Type               None
          Usage Type               Data
        wMaxPacketSize     0x0400  1x 1024 bytes
        bInterval               0
        bMaxBurst              15
  Binary Object Store Descriptor:
    bLength                 5
    bDescriptorType        15
    wTotalLength       0x0016
    bNumDeviceCaps          2
    USB 2.0 Extension Device Capability:
      bLength                 7
      bDescriptorType        16
      bDevCapabilityType      2
      bmAttributes   0x00000002
        HIRD Link Power Management (LPM) Supported
    SuperSpeed USB Device Capability:
      bLength                10
      bDescriptorType        16
      bDevCapabilityType      3
      bmAttributes         0x00
      wSpeedsSupported   0x000e
        Device can operate at Full Speed (12Mbps)
        Device can operate at High Speed (480Mbps)
        Device can operate at SuperSpeed (5Gbps)
      bFunctionalitySupport   3
        Lowest fully-functional device speed is SuperSpeed (5Gbps)
      bU1DevExitLat          10 micro seconds
      bU2DevExitLat        2047 micro seconds
  Device Status:     0x000c
    (Bus Powered)
    U1 Enabled
    U2 Enabled
```


## Cross-References

- `re/notes/fx3-firmware-protocol.md` -- firmware image format and upload protocol
- `re/notes/firmware-extraction.md` -- B2 wrapper and firmware blob locations
- `re/notes/fx3-usb-events.md` -- firmware-side USB event handling
- `doc/state-machine/device-states.md` -- host-side device state machine
- `doc/protocol/fx3-firmware.md` -- confirmed firmware upload protocol
