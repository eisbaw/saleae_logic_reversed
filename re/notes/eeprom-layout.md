# EEPROM Layout: Device Identity, Serial, Hardware Revision

Analysis date: 2026-03-11
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0106
Method: Ghidra headless decompilation of ReadDeviceId, SetIdToDevice, WipeEeprom,
        TestEeprom, ParseBoardRevision, CommonDeviceInit, and related functions.
        RIP-relative LEA cross-reference scanning for string localization.


## Summary

Each Saleae FX3-based device (Logic 8, Logic Pro 8, Logic Pro 16) has an I2C
EEPROM on the board that stores device identity and hardware revision
information. The EEPROM is accessed via the FX3 I2C tunnel: EP1 opcodes 0x55
(write) and 0x88 (read), which the FX3 firmware forwards to the physical I2C
bus.

The EEPROM is NOT used for calibration data. Calibration is downloaded from
Saleae's servers using the device ID as a lookup key. The EEPROM device ID is
also the user-visible "Serial #" in the Saleae Logic 2 GUI.

For MSO devices, the EEPROM is an AT24C64D (8192 bytes, 32-byte pages) that
stores both device metadata AND calibration data. MSO EEPROM layout is NOT
covered here; see `re/notes/i2c-eeprom-protocol.md` for MSO details.


## EEPROM Specifications by Device

| Property      | FX2 (Logic, Logic 16) | FX3 (Logic 8, Pro 8/16) | MSO         |
|---------------|----------------------|-------------------------|-------------|
| EEPROM chip   | Unknown (small)      | Unknown (likely AT24C32) | AT24C64D    |
| Total size    | 256 bytes            | 4096 bytes (0x1000)     | 8192 bytes  |
| Page size     | 64 bytes (0x40)      | Device-dependent        | 32 bytes    |
| I2C access    | FX2 vendor control   | EP1 I2C tunnel          | MSO I2C     |
| Read opcode   | EP0 vendor request   | 0x88 (ReadI2c)          | MSO-specific|
| Write opcode  | EP0 vendor request   | 0x55 (WriteEeprom)      | MSO-specific|

Source: FX2 size confirmed by "eeprom is only 256 bytes" assertion. FX3 size
confirmed by TestEeprom loop `for addr = 8; addr < 0x1000; addr += 0x100`.
MSO size confirmed by AT24C64D source path and assertion.

EepromSizeMax() and EepromPageSize() are virtual methods on the DeviceFeatures
class, returning different values per device type. The exact page sizes for FX3
devices need dynamic verification (likely 32 or 64 bytes based on EEPROM chip).


## FX3 EEPROM Address Map (Logic 8, Logic Pro 8, Logic Pro 16)

```
Offset  Size  Field                    Description
------  ----  -----                    -----------
0x00    8     Reserved / Unknown       Wiped to 0xFF by WipeEeprom.
                                       Not explicitly read by any identified function.
                                       Possibly unused padding, factory test data,
                                       or a legacy field (FX2 DEVICEID1 equivalent).

0x08    8     Device ID (uint64)       The authoritative device identifier.
                                       64-bit unsigned integer, little-endian.
                                       Read by ReadDeviceId() via ReadEeprom(addr=8, count=8).
                                       Written by SetIdToDevice() via WriteEeprom(addr=8, data).
                                       Triple-read with consistency check for error detection.
                                       Displayed as hex in UI: "Serial #: 764051B51BD4D9F1"
                                       Used for calibration download: {device_id_hex}.cal

0x10    16    Unknown                  Wiped to 0xFF by WipeEeprom.
                                       No read operations found targeting these addresses.
                                       Possibly reserved for future use or factory data.

0x20    6     Hardware Revision        ASCII string in format "REVx.y" where:
                                       byte[0] = 'R' (0x52)
                                       byte[1] = 'E' (0x45)
                                       byte[2] = 'V' (0x56)
                                       byte[3] = major version (single byte, 0-11 observed)
                                       byte[4] = '.' (0x2E)
                                       byte[5] = minor version (single byte)
                                       Read by ParseBoardRevision() via ReadEeprom(addr=0x20, count=6).
                                       Determines FPGA type, ADC type, bitstream selection.

0x26    ...   Unused / test area       TestEeprom writes incrementing pattern starting at
                                       address 8 through 0xFFF in 256-byte pages.
                                       WipeEeprom only clears bytes 0x00-0x25 (38 bytes).
```

Total provisioned region: 38 bytes (0x00-0x25).
Total EEPROM capacity: 4096 bytes (0x1000).


## FX2 EEPROM Address Map (Logic, Logic 16)

The FX2 devices store two redundant copies of the device ID:

```
Offset  Size  Field         Description
------  ----  -----         -----------
???     8     DEVICEID1     First copy of device ID
???     8     DEVICEID2     Second copy of device ID
```

The exact offsets for DEVICEID1 and DEVICEID2 on FX2 devices are not
determined. The FX2 EEPROM is only 256 bytes with 64-byte pages. The FX2
code uses vendor control transfers (EP0) for I2C access, not the EP1
tunnel protocol.

Source: String references "DEVICEID1" at 0x3e6b10, "DEVICEID2" at 0x388fb2.


## Device ID Lifecycle

### Provisioning (Factory)

1. `SetIdToDevice(DeviceFeatures* features, uint64 device_id)` writes 8 bytes
   to EEPROM address 0x08.
2. Reads back from address 0x08 and compares. Throws exception on mismatch:
   "SetIdToDevice failed, {written_id}, {read_id}"
3. Source: `LogicAnalyzerDevice.cpp`
4. ELF VA: 0x1c51a50 (Ghidra 0x1d51a50)

### Reading (Runtime)

1. `ReadDeviceId(DeviceFeatures* features) -> uint64` reads 8 bytes from
   EEPROM address 0x08 via ReadEeprom.
2. Called THREE times consecutively by device constructors (triple-read).
3. If all three reads agree, the value is accepted.
4. If two of three agree, the majority value is used with a warning:
   "Logic 8 Device ID read failure detected. Correct ID {}, First, failed read: {}"
5. If all three disagree, an exception is thrown:
   "Logic 8 Device ID read complete failure. 3 IDs: {}, {}, {}"
6. Source: `LogicGraduateDevice.cpp` (line 53), `LogicProfessionalDevice.cpp` (line 83)
7. ReadDeviceId ELF VA: 0x1c51a30 (Ghidra 0x1d51a30), 26 bytes

### Storage in Device Object

After reading, the device ID is stored:
- `this->field_0xA8` (at `param_1[0x15]`) = device ID (uint64)
- `this->field_0xB0` (at `param_1[0x16]`) = device type enum (uint32)

The device type is set to:
- 5 = Logic 8 (LogicGraduateDevice)
- 6 = Logic Pro 8 (LogicProfessionalDevice, param_4 == 8)
- 7 = Logic Pro 16 (LogicProfessionalDevice, param_4 == 16)

### Wiping (Factory Reset)

`WipeEeprom()` writes 38 bytes (0x26) of 0xFF starting at EEPROM address 0.
This clears both the device ID and the hardware revision string, effectively
factory-resetting the identity data.
Source: `LogicAnalyzerDevice.cpp` (line 497)
ELF VA: 0x1c50820 (Ghidra 0x1d50820)


## Hardware Revision Parsing

### ParseBoardRevision

`ParseBoardRevision(RevisionDetail* out, Logic2FpgaDevice* device)` reads
6 bytes from EEPROM address 0x20 and parses the hardware revision string.

ELF VA: 0x1c23840 (Ghidra 0x1d23840), 1997 bytes.

Algorithm:
1. Check for `SALEAE_ENABLE_PROTOTYPE` environment variable
2. Read 6 bytes from EEPROM address 0x20
3. Validate format: bytes must be 'R', 'E', 'V', major, '.', minor
4. Extract major version (byte[3]) and minor version (byte[5])
5. Check for `SALEAE_REVISION_OVERRIDE` environment variable (format "X.Y.Z")
   - If set, overrides the EEPROM values with logging:
     "Revision override! Original revision (ignored) {major}.{minor}.0"
     "Revision override! Using {new_major}.{new_minor}.0"
6. Construct version string "{major}.{minor}.0" (patch always 0)
7. Switch on major version to determine FPGA type and ADC type
8. Look up expected bitstream CRC from per-device-type x per-revision table

### Revision to FPGA/ADC Mapping

The major revision byte determines the hardware platform:

| Major Rev | FpgaType         | AdcType      | Notes                    |
|-----------|------------------|--------------|--------------------------|
| 0         | 0 (Xilinx)       | 0 (AD9637)   | Original Xilinx Spartan  |
| 1         | 1 (Xilinx alt?)  | 0 (AD9637)   | Second Xilinx variant    |
| 3         | 2 (Lattice ECP5) | 0 (AD9637)   | First Lattice generation |
| 4, 5      | 2 (Lattice ECP5) | 1 (ADS528x)  | Lattice with JESD ADC    |
| 6, 9, 10, 11 | 2 (Lattice ECP5) | 1 (ADS528x) | Newer Lattice revisions |
| 7         | 1               | 2            | Unidentified variant     |
| 8         | 2               | 2            | Unidentified variant     |
| 2 (default) | THROWS          | --           | "Device hardware revision not supported" |

Source: Switch statement in ParseBoardRevision at ELF VA 0x1c23bc0.
Gaps (major rev 2) throw a device-hardware-revision-not-supported exception.

### Bitstream CRC Lookup Tables

The function uses a two-level switch to select a bitstream CRC lookup table
entry. First level: device internal type (5=Pro8, 6=Pro16, 7=MSO). Second
level: major revision. Each entry is 16 bytes (4 x uint32), where the first
uint32 is the expected bitstream CRC.

Selected CRC values (confirming the FPGA bitstream selection per revision):

| Device  | Rev 0  | Rev 1  | Rev 3  | Rev 4  | Rev 5/6 | Rev 9  | Rev 11 |
|---------|--------|--------|--------|--------|---------|--------|--------|
| Pro 8   | 0x53394| 0x5327c| 0x5327c| 0x35296| 0x457d2 | 0x46537| 0x34ccc|
| Pro 16  | 0x5327c| 0x5327c| 0x5327c| 0x3e332| 0x4fdc6 | --     | --     |
| MSO     | 0x71544| 0x71544| 0x71544| 0x576fe| 0x6dc55 | --     | --     |

Different CRC values across revisions confirm that different bitstream files
are loaded depending on the hardware revision read from EEPROM.


### RevisionDetail Fields

The `RevisionDetail` struct (output of ParseBoardRevision) contains:

```
Offset  Size  Field           Description
------  ----  -----           -----------
0x00    4     FpgaType        0=Xilinx, 1=Xilinx-alt, 2=Lattice ECP5
0x04    4     Unknown         Set to specific values for some revisions
0x08    4     AdcType         0=AD9637, 1=ADS528x, 2=unknown
0x0C    4     Unknown
0x10    16    BitstreamCRC    Expected CRC from lookup table
0x20    24    VersionString   "{major}.{minor}.0" std::string
```

Key methods on RevisionDetail:
- `RevisionDetail::AdcType` -- determines ADC initialization sequence
- `RevisionDetail::UsesNewCryptoInterface()` -- determines crypto IC access path

The ADC type determines:
- AD9637: SPI register init via `SetAd9637Enabled`, 15-bit address width
- ADS528x: JESD204B link init, different register set
- HMCAD1100: 7-bit SPI address (Logic 4/8 Xilinx revisions only)

### Important: ADC Type is Per-Revision, Not Per-Device

A Logic Pro 8 with revision 0 (Xilinx) has an AD9637 ADC.
A Logic Pro 8 with revision 4 (Lattice) has an ADS528x ADC.
The device model alone does NOT determine the ADC hardware.

This is confirmed by the assertion:
```
mBoardRevision.mAdcType == RevisionDetail::AdcType::AD9637
```
which appears in Xilinx-specific code paths.


## Opcode 0x8B (GetDeviceInfo)

The LogicProfessionalDevice constructor sends EP1 opcode 0x8B after EEPROM
device ID and hardware revision setup. This retrieves additional device
information from the FX3 firmware.

### Protocol

```
Host -> Device (EP1 OUT):  [0x8B]              (1 byte)
Device -> Host (EP1 IN):   [data...]           (up to 128 bytes)
```

The host reads up to 0x80 (128) bytes in the response. The response content
is parsed as a string and logged as:
```
Firmware build date: {}
```

Source: LogicProfessionalDevice constructor at ELF VA 0x1c40c00, lines
referencing opcode 0x8B at Ghidra VA 0x1d40fa2.

The response buffer is also used to extract the firmware build date string.
The exact field layout within the 128-byte response is not fully determined
from static analysis alone. The first portion likely contains:

- Firmware version information
- Build date string
- Possibly firmware build hash or identifier

The GetDeviceInfo opcode is sent AFTER the device ID read and hardware
revision parsing, as part of the device constructor -- not during
CommonDeviceInit.


## CommonDeviceInit Flow

`Logic2FpgaDevice::CommonDeviceInit()` at ELF VA 0x1c21fb0 (2260 bytes)
performs FPGA post-programming initialization. The EEPROM-related parts are:

1. Check if device already initialized (`param_1 + 0x220`)
2. If not initialized, call board-revision-dependent initialization:
   - FPGA responsiveness check: write 0xAA to register 0x7F twice, read back
     and verify it returns 0xAA (FPGA readback test)
   - On failure: "FPGA readback got {:#x} instead of 0xAA"
   - On critical failure: "Fpga initialization failed"
3. ADC initialization based on `param_1 + 0x218` (FPGA type enum):
   - Type 0 (Xilinx): HMCAD1100 default settings, then ADC init
   - Type 1 (Lattice JESD204B): JESD link setup
   - Type 2 (Lattice newer): Register writes for JESD204B link with
     register values 0x2a=0x1111, 0x2b=0x1111, 0x42=0x8040, 0x46=0x8204
4. For Pro devices (internal type 6 or 7): register 0x15 write, then
   FPGA register 0x0F configuration based on FPGA type
5. EEPROM integrity check (non-Pro): read EEPROM and check for all-0xFF
   pattern. If non-0xFF found, call device-specific EEPROM handler
   (likely TestEeprom or crypto provisioning check)
6. Crypto interface setup based on `RevisionDetail::UsesNewCryptoInterface()`


## TestEeprom

`LogicProfessionalDevice::TestEeprom()` at ELF VA 0x1c44010 (516 bytes)
verifies EEPROM integrity by writing and reading back a test pattern.

Algorithm:
1. For address = 8 to 0xFFF, stepping by 256:
   a. Build a 256-byte buffer (or smaller for the last page)
   b. Fill with incrementing values (continuing across pages)
   c. Write to EEPROM via WriteEeprom(address, data)
2. Read entire EEPROM content back (via FUN_01d43660)
3. Verify: starting at byte 8, compare each byte against expected
   incrementing pattern (0, 1, 2, 3, ...)
4. On mismatch: assertion failure "EEprom error detected!"

Key observations:
- Starts at address 8, NOT address 0 -- preserves the device ID field
- Tests up to address 0xFFF (4095), confirming 4096-byte EEPROM capacity
- The first 8 bytes (0x00-0x07) are never touched by the test
- The device ID at 0x08-0x0F IS overwritten by the test pattern
- The hardware revision at 0x20-0x25 IS overwritten by the test pattern
- This suggests TestEeprom is only used during factory provisioning, before
  the device ID and revision are written


## FX3 ReadEeprom Wire Format

`Fx3DeviceFeatures::ReadEeprom(U16 address, U8* buffer, U16 count)` at
ELF VA 0x1c52460 constructs a 7-byte EP1 command packet:

```
Byte  Value                 Description
----  -----                 -----------
0     0x07                  Command sub-type (read?)
1     0x33                  I2C bus/device flags
2     0x81                  I2C command mode
3     address & 0xFF        EEPROM address low byte
4     (address >> 8) & 0xFF EEPROM address high byte
5     count & 0xFF          Read count low byte
6     (count >> 8) & 0xFF   Read count high byte
```

Sent via vtable[0x20] (EP1 OUT command write). Response received via
vtable[0x28] (EP1 IN command read) with `count` bytes.

Note: The first 3 bytes (0x07, 0x33, 0x81) are pre-encryption constants.
On Pro devices, the entire packet is XOR-encrypted via SimpleEncryption
before transmission. The actual on-wire bytes will differ from these
constants at runtime.


## FX3 WriteEeprom Wire Format

`Fx3DeviceFeatures::WriteEeprom(U16 address, vector<U8>& data)` at
ELF VA 0x1c51f50 constructs a 7-byte header followed by data:

```
Byte  Value                 Description
----  -----                 -----------
0     0x06                  Command sub-type (write?)
1     0x42                  I2C bus/device flags
2     0x55                  WriteEeprom opcode
3     address & 0xFF        EEPROM address low byte
4     (address >> 8) & 0xFF EEPROM address high byte
5     size & 0xFF           Data size low byte
6     (size >> 8) & 0xFF    Data size high byte
7+    data[0..N]            Data bytes to write
```

After sending, reads 1 byte response. Expects 0x01 for success. On failure:
`abort()`.

Both ReadEeprom and WriteEeprom check:
- `starting_address < EepromSizeMax()` (virtual, device-dependent)
- `(starting_address + count) <= EepromSizeMax()`
- `count <= EepromPageSize()` (virtual, device-dependent)


## Environment Variables

| Variable                   | Effect                                          |
|----------------------------|-------------------------------------------------|
| SALEAE_ENABLE_PROTOTYPE    | Enables prototype device support (checked first) |
| SALEAE_REVISION_OVERRIDE   | Overrides EEPROM revision with format "X.Y.Z"   |


## Confidence Assessment

| Finding                                    | Confidence  | Evidence                          |
|--------------------------------------------|-------------|-----------------------------------|
| Device ID at EEPROM address 0x08, 8 bytes  | **Confirmed** | ReadDeviceId decompilation: addr=8, count=8 |
| HW revision at address 0x20, 6 bytes "REVx.y" | **Confirmed** | ParseBoardRevision decompilation: addr=0x20, count=6 |
| WipeEeprom clears 38 bytes at address 0    | **Confirmed** | WipeEeprom decompilation: 0x26 bytes of 0xFF |
| FX3 EEPROM total size = 4096 bytes         | **Confirmed** | TestEeprom loop: addr 8..0xFFF              |
| FX2 EEPROM total size = 256 bytes          | **Confirmed** | Assertion string "eeprom is only 256 bytes"  |
| Triple-read device ID for integrity        | **Confirmed** | LogicGraduateDevice constructor decompilation |
| Major revision determines FPGA/ADC type    | **Confirmed** | ParseBoardRevision switch statement           |
| Opcode 0x8B returns firmware build date    | **Confirmed** | LogicProfessionalDevice constructor: 0x8B -> "Firmware build date" |
| ADC type per-revision not per-device       | **Confirmed** | RevisionDetail::AdcType set by ParseBoardRevision, not device class |
| Bitstream CRC varies by revision           | **Confirmed** | Distinct CRC values in lookup tables per revision |
| Bytes 0x00-0x07 purpose unknown            | **Uncertain** | No read operation found, only wiped by WipeEeprom |
| ReadEeprom 7-byte header constants         | **High**      | Decompilation of FX3 ReadEeprom at 0x1c52460 |
| WriteEeprom 7-byte header constants        | **High**      | Decompilation of FX3 WriteEeprom at 0x1c51f50 |

### Unknown / Needs Further Investigation

1. **Bytes 0x00-0x07**: No code reads from EEPROM address 0x00-0x07 in any
   identified function. These bytes are wiped to 0xFF by WipeEeprom but never
   explicitly used. Possible explanations: (a) legacy field from an older
   provisioning format, (b) reserved for future use, (c) used by FX3 firmware
   directly (not host-side code), (d) crypto IC provisioning flag.

2. **Bytes 0x10-0x1F**: Similarly, no reads target this region. These 16 bytes
   are wiped by WipeEeprom but never read by identified host-side functions.

3. **FX3 EepromPageSize() per device**: The exact return value of the virtual
   EepromPageSize() method for each FX3 device type. Likely 32 or 64 bytes.

4. **ReadEeprom/WriteEeprom first 3 bytes**: The meaning of the constant
   header bytes (0x07/0x33/0x81 for read, 0x06/0x42/0x55 for write). These
   may be FX3 I2C API parameters or command framing bytes.

5. **Opcode 0x8B response fields**: Beyond the firmware build date string,
   the full layout of the 128-byte response is not determined.

6. **Logic 4 EEPROM**: Logic 4 uses FX2LP with a separate code path. Its
   EEPROM layout may differ from the FX3 devices documented here.


## Address Summary

| ELF VA      | Ghidra VA   | Function                    | Size  |
|-------------|-------------|-----------------------------|-------|
| 0x1c21fb0   | 0x1d21fb0   | CommonDeviceInit            | 2260  |
| 0x1c23840   | 0x1d23840   | ParseBoardRevision          | 1997  |
| 0x1c39bfb   | 0x1d39bfb   | Logic Device ID read (FX2)  | --    |
| 0x1c3d1a0   | 0x1d3d1a0   | LogicGraduateDevice ctor    | 1097  |
| 0x1c40c00   | 0x1d40c00   | LogicProfessionalDevice ctor| 1403  |
| 0x1c44010   | 0x1d44010   | TestEeprom                  | 516   |
| 0x1c50820   | 0x1d50820   | WipeEeprom                  | 304   |
| 0x1c50ad0   | 0x1d50ad0   | Fx2 WriteEeprom             | 542   |
| 0x1c51a30   | 0x1d51a30   | ReadDeviceId                | 26    |
| 0x1c51a50   | 0x1d51a50   | SetIdToDevice               | 414   |
| 0x1c51f50   | 0x1d51f50   | Fx3 WriteEeprom             | 1045  |
| 0x1c52460   | 0x1d52460   | Fx3 ReadEeprom              | 858   |
| 0x1c55970   | 0x1d55970   | LogicStudentDevice ctor     | 1157  |
| 0x2d62780   | 0x2e62780   | GetFpgaBitstreamVersion     | 276   |
| 0x2d34890   | 0x2e34890   | GetFirmwareVersion          | 533   |


## References

- `re/notes/i2c-eeprom-protocol.md` -- I2C/EEPROM tunnel protocol details
- `re/notes/device-serial-number.md` -- Device ID as serial number lifecycle
- `re/notes/ep1-command-protocol.md` -- EP1 command opcodes
- `re/notes/multi-device-differences.md` -- Per-device ADC/FPGA differences
- `re/notes/device-type-enum.md` -- DeviceType enum mapping
- `re/notes/adc-initialization.md` -- ADC init sequences per ADC type
- `re/notes/xor-obfuscation.md` -- SimpleEncryption affecting wire format
- `doc/protocol/ep1-commands.md` -- Confirmed EP1 command map
- `doc/devices/device-types.md` -- Confirmed device types
- `re/ghidra/decompiled-eeprom.txt` -- Full decompilation output
- `re/ghidra/decompiled-eeprom2.txt` -- Additional helper function decompilation
- `re/ghidra/scripts/DecompileEeprom.java` -- Ghidra script used for analysis
- `re/ghidra/scripts/DecompileEeprom2.java` -- Additional Ghidra script
