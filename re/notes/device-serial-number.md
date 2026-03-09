# Device Serial Number Lifecycle

Investigation date: 2026-03-09
Task: TASK-0051
Hardware: Logic Pro 8 (21a9:1005) on nvdk-msdlab-09


## Summary

There are THREE distinct "serial numbers" in the Saleae device ecosystem, and
they serve completely different purposes:

| Identifier | Value (our device) | Source | When Available | Purpose |
|---|---|---|---|---|
| SPI flash serial | 0000000004BE | FX3 SPI boot firmware (iSerialNumber=3) | Before firmware upload (USB 2.0 state) | Unknown -- possibly Cypress or Saleae factory |
| USB string "SERNUM" | SERNUM (literal) | Uploaded FX3 firmware (iSerialNumber=4) | After firmware upload (USB 3.0 state) | Placeholder -- not a real serial |
| EEPROM device ID | 764051B51BD4D9F1 | I2C EEPROM via EP1 opcode 0x88 | After firmware + FPGA init | Calibration lookup, user-visible "Serial #" |


## EEPROM Device ID is the "Real" Serial Number

The Saleae Logic 2 host application uses the 64-bit EEPROM device ID as the
user-visible serial number. The JS frontend converts it like this:

```javascript
// bundle.js
deviceIdToSerial = function(A) {
    return BigInt(A).toString(16).toUpperCase();
}
```

The UI displays this as `Serial #: 764051B51BD4D9F1` in the device info dialog.
This same ID is used to download calibration data from Saleae's servers:

```
https://downloads.saleae.com/calibration/764051b51bd4d9f1.cal
```

The EEPROM device ID is read during `Logic2FpgaDevice::CommonDeviceInit()` after
firmware upload and FPGA programming, via the FX3 I2C tunnel (EP1 opcode 0x88).
The host logs it as:

```
Logic Pro {} Device ID: {:#x}
```

There are per-device-type format strings:
- `Logic Pro {} Device ID: {:#x}` (at 0x3be23d)
- `Logic 8 Device ID: {:#x}` (at 0x3c4e3e)
- `Logic Student Device ID: {:#x}` (at 0x3a5b56)
- `Logic Device ID: {:#x}` (at 0x403667)
- `Logic16 Device ID: {:#x}` (at 0x369c5e)

The EEPROM read includes retry logic with triple-read verification:
- "LogicAnalyzerDevice::ReadEeprom failed after 3 tries." (at 0x3c4f2a)
- "Logic 8 Device ID read failure detected. Correct ID {:#X}, First, failed
  read: {:#X}" (at 0x39e27a)
- "Logic 8 Device ID read complete failure. 3 IDs: {:#X}, {:#X}, {:#X}"
  (at 0x418b2b)

The FX2 path stores two redundant copies: DEVICEID1 (at 0x3e6b10) and
DEVICEID2 (at 0x388fb2).


## SPI Flash Serial (0000000004BE) -- Likely Cypress Default

The SPI flash boot firmware exposes 0000000004BE as iSerialNumber. This value:

- Is 12 hex digits (48 bits), typical of Cypress FX3 serial number format
- Decimal value is 1214, which is small and looks like a sequential number
- When queried against Saleae's calibration API (decimal 1214), returns null
- Is NOT used by the host application for any purpose we can identify
- Is replaced by "SERNUM" after firmware upload

This serial is almost certainly a Cypress FX3 default or factory-programmed
value in the SPI flash B2 configuration header, not a Saleae-assigned ID.
It may represent the FX3 chip or SPI flash production serial.


## "SERNUM" -- Hardcoded Placeholder in Uploaded Firmware

The string "SERNUM" is a literal constant in the USB string descriptor table
of the uploaded FX3 firmware blobs. Found at two offsets in the .so
(corresponding to two firmware variants -- Logic Pro 8 and Logic Pro 16):

```
007a69f4: 0e 03 53 00 45 00 52 00 4e 00 55 00 4d 00   ..S.E.R.N.U.M.
007c3e06: 0e 03 53 00 45 00 52 00 4e 00 55 00 4d 00   ..S.E.R.N.U.M.
```

Format: USB string descriptor (type 0x03, length 14 bytes = 6 UTF-16LE chars
+ 2 header bytes).

The full USB string descriptor table in each firmware blob contains:
1. "SERNUM" (iSerialNumber=4)
2. "iAP Interface"
3. "OS Des" (truncated -- likely OS string descriptor)
4. "Logic Pro" (iProduct=2)
5. "Saleae" (iManufacturer=1)

"SERNUM" is NOT a real serial -- it is a fixed placeholder. The host
application never reads this USB serial string. It reads the EEPROM device
ID via I2C instead.


## Host Application Does NOT Read USB Serial

The host binary references `libusb_get_string_descriptor_ascii` but there is
no evidence it reads the iSerialNumber string from the USB descriptor. The
`/serial` JSON path exists but appears to relate to the MSO device serial
(which comes from device metadata in the EEPROM, not the USB descriptor):

```
	Device Serial:    {}        -- MSO context only (at 0x37dbdd)
	Board Revision:   {} ({})
	Board ID:         {}
```

For Logic Pro devices, the device identification flow is:
1. Detect device by VID:PID (21a9:1005)
2. Check `IsFirmwareDownloaded` (via bcdDevice: 1.00 = needs FW, 0.00 = ready)
3. Upload firmware if needed (via vendor control transfers)
4. Wait for re-enumeration at USB 3.0
5. Program FPGA bitstream
6. `CommonDeviceInit` reads device ID from EEPROM via I2C tunnel
7. Device ID reported to JS frontend
8. JS frontend uses device ID for calibration download and as "Serial #"

The SPI flash serial (0000000004BE) is never read by the host application
because `libusb_get_string_descriptor_ascii` is called for the post-firmware
descriptors, not the pre-firmware ones. By the time the host connects to
the device, it has already re-enumerated with "SERNUM".


## Implications for Rust Driver

1. **Do NOT rely on USB iSerialNumber for device identification**.
   After firmware upload, it is always the literal string "SERNUM".
   Before firmware upload, it is the SPI flash serial which is not used
   for anything by the Saleae host.

2. **Read the EEPROM device ID after CommonDeviceInit**.
   The 64-bit device ID from the EEPROM is the authoritative device
   identifier. It is used for:
   - Calibration data lookup (server-hosted .cal files)
   - User-visible serial number display
   - Multi-device disambiguation

3. **The SPI flash serial could be useful for multi-device firmware upload**.
   If multiple Saleae devices are in the SPI flash boot state simultaneously,
   the SPI flash serial (0000000004BE etc.) could distinguish them before
   firmware is uploaded. However, the Saleae host appears to handle this
   by VID:PID + bus/port, not serial number.

4. **No need to capture SPI serial before firmware upload**.
   The EEPROM device ID (available after init) is sufficient for all
   identification purposes.


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| EEPROM device ID is user-visible "Serial #" | Confirmed | bundle.js deviceIdToSerial, calibration API |
| EEPROM device ID used for calibration lookup | Confirmed | GraphQL API, .cal URL format |
| "SERNUM" is hardcoded in firmware blobs | Confirmed | UTF-16LE at 0x7a69f4 and 0x7c3e06 |
| SPI flash serial not used by host | High | No code path reads pre-firmware USB serial |
| Host reads EEPROM ID after CommonDeviceInit | High | Format strings, ReadEeprom retry logic |
| 0000000004BE is Cypress/factory default | Medium | Small sequential value, not in calibration DB |


## Cross-References

- `re/notes/bootloader-descriptors.md` -- SPI flash boot state descriptors
- `re/notes/i2c-eeprom-protocol.md` -- EEPROM read protocol (opcode 0x88)
- `re/notes/calibration-validation.md` -- calibration device ID format
- `doc/calibration/calibration-format.md` -- calibration URL format
