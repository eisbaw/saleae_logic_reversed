# Logic 4/8 Firmware Mapping

Investigation date: 2026-03-09
Source: libgraph_server_shared.so v2.4.40
Task: TASK-0027


## Question

Only 2 FX3 firmware images were found (Pro 8 PID=0x1005, Pro 16
PID=0x1006), but 4 FX3-based devices were assumed to exist (Logic 4,
Logic 8, Logic Pro 8, Logic Pro 16). Where is the firmware for Logic 4
and Logic 8?


## Answer

**Logic 4 is NOT FX3-based.** It uses FX2LP (8051) and gets Intel HEX
firmware, just like the original Logic and Logic 16. The PRD assumption
that Logic 4 uses FX3 was incorrect.

**Logic 8 shares the Pro 8 FX3 firmware.** After upload, the device
re-enumerates with the PID embedded in the firmware image. The "reseed"
mechanism is unrelated to PID patching -- it establishes an XOR seed for
command obfuscation (see `re/notes/reseed-mechanism.md`).

**Logic Pro 16 has no embedded firmware fallback.** It checks for a
custom firmware file first; if none is found, it throws an exception.
In practice, Logic Pro 16 likely relies on the custom firmware path
(firmware file on disk) or the two embedded images work because the
reseed mechanism patches the PID anyway and the code path is handled
differently than traced here.


## Evidence

### GetFirmwareForDeviceType jump table

Function at VA 0x02d9ad60 (`UsbDevice::GetFirmwareForDeviceType`).
Takes a `DeviceType` enum in `esi`, guards with `cmp esi, 7; ja` (assert
for values > 7), then indexes a jump table at VA 0x0079db70:

| Case | DeviceType | Jump Target    | Firmware                                   |
|------|-----------|----------------|---------------------------------------------|
| 0    | 0         | VA 0x02d9ad85  | IHex group 1 at 0x0037e0ac (FX2, unknown)  |
| 1    | 1         | VA 0x02d9ad8e  | IHex group 3 at 0x003ac65e (FX2, Logic original) |
| 2    | 2         | VA 0x02d9ae29  | ASSERTION: "Unknown device type"            |
| 3    | 3         | VA 0x02d9ae29  | ASSERTION: "Unknown device type"            |
| 4    | 4         | VA 0x02d9ad97  | IHex group 4 at 0x003cb8cd (FX2, Logic 16) |
| 5    | 5         | VA 0x02d9ada0  | IHex group 2 at 0x0039743d (FX2, Logic 4)  |
| 6    | 6         | VA 0x02d9addc  | FX3 Pro 8 FW at 0x0079e7a0 (119,824 bytes) |
| 7    | 7         | VA 0x02d9ade5  | FX3 Pro 16 FW at 0x007bbbb0 (119,824 bytes)|

Cases 0-5 return Intel HEX firmware (IHexFirmwareImageDescriptor).
Cases 6-7 return FX3 CY firmware (FirmwareImageDescriptor as gsl::span).

FX3 firmware descriptors are stored in `.data.rel.ro`:
- VA 0x2ee5b60: {size=119824, ptr=0x79e7a0} (Pro 8 FW, PID 0x1005)
- VA 0x2ee5b70: {size=119824, ptr=0x7bbbb0} (Pro 16 FW, PID 0x1006)


### VID:PID to DeviceType table

Table at file offset 0x004718dc, format: {VID:u16-LE, PID:u16-LE, DeviceType:u32-LE}:

| VID    | PID    | DeviceType | Device              |
|--------|--------|-----------|---------------------|
| 0x0925 | 0x3881 | 1         | Original Logic (FX2)|
| 0x21a9 | 0x1001 | 4         | Logic 16 (FX2LP)    |
| 0x21a9 | 0x1003 | 5         | Logic 4 (FX2LP)     |
| 0x21a9 | 0x1004 | 6         | Logic 8 (FX3)       |
| 0x21a9 | 0x1005 | 7         | Logic Pro 8 (FX3)   |
| 0x21a9 | 0x1006 | 12        | Logic Pro 16 (FX3)  |
| 0x21a9 | 0x1007 | 1         | Unknown (MSO?)      |


### Caller logic (DownloadFirmware)

The DownloadFirmware function (in UsbDevice.cpp, around VA 0x02d9b920)
has three stages before selecting firmware:

1. **Custom firmware file check** (bitmask 0x10c0 = bits 6, 7, 12):
   For DeviceTypes 6 (Logic 8), 7 (Logic Pro 8), and 12 (Logic Pro 16),
   the code checks a string at object+0xb0 for a custom firmware file path.
   If a file is found, it reads and parses it instead of using the embedded
   firmware. The string "Loading custom firmware image to FX3: {}" confirms
   this path.

2. **DeviceType 12 special case** (VA 0x02d9bbab):
   `cmp esi, 0xc; je` -- if DeviceType is 12 and no custom firmware was
   loaded, throws exception: "Attempting to program MSO, but the revision
   is unknown." This means Logic Pro 16 (DeviceType 12) has NO embedded
   firmware fallback in the jump table.

3. **GetFirmwareForDeviceType** (VA 0x02d9bbbc):
   For all other device types, the embedded firmware is selected via the
   jump table described above.


### Class hierarchy (from RTTI)

```
LogicAnalyzerDevice (base)
  +-- LogicDevice (FX2, original Logic)
  +-- Logic16Device (FX2LP, Logic 16)
  +-- LogicStudentDevice (Logic 4)

Logic2FpgaDevice (FX3 + FPGA base)
  +-- LogicGraduateDevice (Logic 8)
  +-- LogicProfessionalDevice
        +-- LogicProfessional8Device (Logic Pro 8)
        +-- LogicProfessional16Device (Logic Pro 16)
```

LogicStudentDevice (Logic 4) inherits from LogicAnalyzerDevice, NOT
from Logic2FpgaDevice. This is consistent with Logic 4 being FX2LP-based
and receiving Intel HEX firmware, same as LogicDevice and Logic16Device.

LogicGraduateDevice (Logic 8) and LogicProfessionalDevice (Pro 8/16)
both inherit from Logic2FpgaDevice, confirming they are FX3-based.


### Firmware-to-device mapping summary

| Device           | PID    | DeviceType | Firmware Source      | USB Controller |
|-----------------|--------|-----------|----------------------|---------------|
| Original Logic  | 0x3881 | 1         | IHex group 3         | FX2           |
| Logic 16        | 0x1001 | 4         | IHex group 4         | FX2LP         |
| **Logic 4**     | 0x1003 | 5         | **IHex group 2**     | **FX2LP**     |
| **Logic 8**     | 0x1004 | 6         | **FX3 Pro 8 FW**     | **FX3**       |
| Logic Pro 8     | 0x1005 | 7         | FX3 Pro 16 FW        | FX3           |
| Logic Pro 16    | 0x1006 | 12        | Custom file or assert | FX3           |

Logic 8 gets the Pro 8 firmware image (PID 0x1005 embedded). After
upload, the FX3 re-enumerates with PID 0x1005. The host identifies the
device by its original pre-firmware PID, not the re-enumerated PID.
(Note: the "reseed" mechanism is unrelated to PID; it sets an XOR seed
for command obfuscation.)

Logic Pro 8 gets the Pro 16 firmware image (PID 0x1006 embedded). The
same logic applies.


## Corrections to prior documentation

1. **PRD table**: Logic 4 should be listed as FX2LP, not FX3.
2. **fx3-firmware-protocol.md open question 1**: Answered. Logic 4 does
   NOT share FX3 firmware; it is FX2LP-based and uses Intel HEX firmware.
   Logic 8 shares the Pro 8 FX3 firmware with PID patching via reseed.
3. **firmware-extraction.md**: The IHex group 2 firmware (at 0x0039743d,
   25,992 bytes, 10,794 data bytes) is for Logic 4 (21a9:1003).


## Open questions

1. **Logic Pro 16 firmware path**: DeviceType 12 has no embedded firmware
   fallback. The custom firmware file path (object+0xb0) must provide it.
   Where does this file come from? Is it extracted from the .so at first
   run and cached on disk?

2. **Reseed protocol**: RESOLVED. ReseedDevice does NOT change the PID.
   It sends 4 random bytes as an XOR seed for SimpleEncryption. See
   `re/notes/reseed-mechanism.md` for full analysis.

3. **DeviceType 0**: What device uses IHex group 1? Might be a test/debug
   device type or an older Saleae product variant.

4. **Why the offset firmware assignment?**: Logic 8 gets Pro 8 FW (next PID
   up) and Pro 8 gets Pro 16 FW (next PID up). Since the reseed mechanism
   is NOT PID patching (it's XOR seed exchange), the offset firmware
   assignment may indicate that the firmware images are actually shared /
   generic across device types, and the host differentiates devices by
   their pre-firmware VID:PID. The FX3 re-enumeration PID is a firmware
   build artifact, not necessarily the device's true identity.
