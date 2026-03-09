# I2C / EEPROM Tunnel Protocol Analysis

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (49,762,176 bytes, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Method: String extraction, RIP-relative LEA cross-reference scanning,
        immediate value extraction, RTTI type name analysis


## Overview

Saleae devices use I2C buses for several purposes:

1. **EEPROM access** -- device ID, calibration metadata, serial number
2. **Crypto IC communication** -- device authentication (anti-counterfeit)
3. **AFE EEPROM** (MSO only) -- analog front-end calibration constants

There are three distinct I2C access paths depending on the device generation:

| Path | Devices | Transport | Code Location |
|------|---------|-----------|---------------|
| FX2 vendor control transfers | Logic, Logic 16 | EP0 control transfer | Fx2DeviceFeatures |
| FX3 EP1 command tunnel | Logic 4/8, Logic Pro 8/16 | EP1 bulk (opcodes 0x55, 0x88) | Fx3DeviceFeatures |
| MSO I2C tunnel | MSO devices | MsoDeviceInterface | Eeprom_AT24C64D, CryptoInterfaceI2c |


## FX3 I2C Tunnel via EP1 Commands

### WriteEeprom (Opcode 0x55)

Used by `Fx3DeviceFeatures::WriteEeprom(U16 address, std::vector<U8>& data)`.

```
Host -> Device (EP1 OUT):  [0x55] [addr_hi] [addr_lo] [data...]
Device -> Host (EP1 IN):   (confirmation response)
```

Evidence:
- MOV byte [rbp+0x02], 0x55 at VA 0x1c50b4d (FX2 path wrapper)
- MOV byte [rsp+0x02], 0x55 at VA 0x1c51fee (FX3 path)
- String "Write eeprom response read" at 0x42781c (response handling)
- String "WriteEeprom cannot write this much data" at 0x382a73 (size check)
- String "WriteEeprom cannot write this much data (ep limit)" at 0x43ba15 (EP limit check)
- String "destination_address < EepromSizeMax()" assertion
- String "( destination_address + data.size() ) <= EepromPageSize()" assertion
- String "buffer.size() <= EepromPageSize()" assertion

The address is a 16-bit value (U16 parameter in function signature). Data is
variable-length but bounded by EepromPageSize() (the write page size of the
EEPROM chip). The FX3 firmware receives this on EP1 OUT and forwards it to
the I2C bus via the CyU3PI2C SDK functions.

### ReadI2c (Opcode 0x88)

Used by `Fx3DeviceFeatures::ReadEeprom(U16 address, U8* buffer, U16 num_bytes)`.

```
Host -> Device (EP1 OUT):  [0x88] [addr/params...]
Device -> Host (EP1 IN):   [data...]
```

Evidence:
- MOV byte [rbp+0x00], 0x88 at VA 0x1c5183d
- MOV byte [rbx], 0x88 at VA 0x1c5311f
- MOV bl, 0x88 at VA 0x1c529e9
- String "ReadI2c request" at 0x3d8aa7 (logged before send)
- String "ReadI2c response" at 0x34ee68 (logged after receive)
- String "ReadEeprom cannot read this much data" at 0x3fc63a
- String "ReadEeprom cannot read this much data (ep limit)" at 0x3d1e18

The read request includes the I2C target address and byte count. The FX3
firmware performs the I2C read and returns the data via EP1 IN.

### WriteI2C (Opcode 0x87)

Used for general I2C write operations (not just EEPROM writes).

```
Host -> Device (EP1 OUT):  [0x87] [i2c_addr] [data...]
Device -> Host (EP1 IN):   (confirmation)
```

Evidence:
- MOV byte [rbp+0x00], 0x87 at VA 0x1c5157d
- String "WriteI2C request" at 0x35c862 (logged before send)
- String "WriteI2C response" at 0x4123f4 (logged after receive)

This is separate from WriteEeprom (0x55). WriteI2C appears to be the
raw I2C write tunnel, while WriteEeprom uses it with EEPROM-specific
addressing (16-bit address in the payload).

### Error Handling

The FX3 firmware returns error codes through the I2C tunnel:
- "I2c tunnel error code {:#X}" at 0x38ff08 -- firmware-level I2C error
- "I2c tunnel op timed out {:#X}" at 0x355106 -- timeout waiting for I2C
- "Firmware error, CypressI2c, code={:x}, msg={}" at 0x376dfd -- CyU3P I2C error
- "TransferI2c returned unexpected number of bytes: expected={}, actual={}" at 0x441abd
- "Unknown I2C read reply" at 0x3ee994
- "Failed to send i2c wake token" at 0x427847 -- wake for crypto IC

### Tunnel Size Limits

The I2C tunnel has maximum transfer sizes:
- "Tunnel write size too large: {}" at 0x409d02
- "Tunnel read size too large: {}" at 0x426a01

These limits are likely constrained by the EP1 bulk transfer maximum
(64 bytes for USB 2.0, 512 bytes for USB 3.0) minus the command header.


## FX2 I2C Access via Vendor Control Transfers

The FX2 path uses USB vendor control transfers (EP0), not the EP1
command protocol.

### Fx2DeviceFeatures I2C Method Signature

From demangled RTTI:
```cpp
typedef i2c_err_t (*i2c_callback)(uint8_t addr, uint8_t* data, uint16_t length);
```

The `i2c_err_t` return type indicates I2C operations return structured
error codes.

### FX2 EEPROM Constants

- **EEPROM size**: 256 bytes ("eeprom is only 256 bytes" at 0x427803)
- **Page size**: 64 bytes (MOV eax, 0x40 at VA 0x1c51df0)
- **Total size**: 256 bytes (MOV eax, 0x100 at VA 0x1c51e00)

Evidence:
- String "eeprom is only 256 bytes" at 0x427803
- String "num_bytes <= EepromPageSize()" assertion

### FX2 I2C WriteRead Operation

The FX2 has a combined write-then-read I2C operation (common for
reading EEPROM: write address, then read data):

- String "WriteRead-Write" at 0x1c51d5e -- first phase: send I2C address
- String "WriteRead-Read" at 0x1c51da7 -- second phase: read data back

### FX2 Device ID Read

The FX2 reads the device ID via I2C from two locations:

- "DEVICEID1" at 0x3e6b10
- "DEVICEID2" at 0x388fb2

These are likely two redundant copies of the device ID stored in
EEPROM at different offsets, read and compared for integrity.


## FX3 EEPROM Constants

### Per-Device EepromSizeMax

The FX3 devices have larger EEPROMs than FX2:

- Assertions: "starting_address < EepromSizeMax()"
- Assertions: "( starting_address + num_bytes ) <= EepromSizeMax()"

EepromSizeMax() is a virtual method, returning different values per device.
The exact values need Ghidra decompilation, but are likely:
- Logic 4/8: 256 bytes (same as FX2, 24C02 or similar)
- Logic Pro 8/16: larger (possibly 8192 bytes, AT24C64 or similar)

### EepromPageSize

The EEPROM page write size is also device-dependent:
- FX2: 64 bytes (confirmed via immediate value)
- FX3: likely 32 bytes (AT24C64 standard) or 64 bytes

### Data Stored in EEPROM

Based on the code flow, the EEPROM stores:

1. **Device ID** -- unique hardware serial number
   - "Logic Pro {} Device ID: {:#x}" at 0x3be23d
   - "Logic 8 Device ID: {:#x}" at 0x3c4e3e
   - "Logic Student Device ID: {:#x}" at 0x3a5b56
   - Read with retry: "LogicAnalyzerDevice::ReadEeprom failed after 3 tries." at 0x3c4f2a
   - "Logic 8 Device ID read failure detected. Correct ID {:#X}, First, failed read: {:#X}" at 0x39e27a
   - "Logic 8 Device ID read complete failure. 3 IDs: {:#X}, {:#X}, {:#X}" at 0x418b2b

2. **Device metadata** -- hardware revision, type information
   - "Getting hardware revision" at 0x3cb765
   - "Hardware revision: {}" at 0x4123ce

3. **Crypto IC provisioning data** -- keys and certificates for authentication


## Crypto IC Authentication

### Three Crypto Interface Implementations

```
CryptoInterface (abstract base)
  +-- CryptoInterfaceI2c    -- FX2 devices, I2C bus to crypto IC
  +-- CryptoInterfaceFpga   -- FX3/FPGA devices, through FPGA registers
  +-- CryptoInterfacePro    -- Logic Pro devices
```

Evidence from RTTI:
- `N6Saleae18CryptoInterfaceI2cE` (Saleae::CryptoInterfaceI2c)
- `N6Saleae19CryptoInterfaceFpgaE` (Saleae::CryptoInterfaceFpga)
- `N6Saleae18CryptoInterfaceProE` (Saleae::CryptoInterfacePro)
- `N6Saleae15CryptoInterfaceE` (Saleae::CryptoInterface)

### Crypto IC Operations

The crypto IC (likely ATECC508A or ATECC608A from Microchip) supports:

| Operation | Method | Block Size |
|-----------|--------|------------|
| Read config zone | ReadConfig32Bytes(offset) | 32 bytes |
| Write config zone | WriteConfigFourBytes(offset, data) | 4 bytes |
| Read data zone | ReadData32Bytes(slot, block) | 32 bytes |
| Write data zone | WriteData32Bytes(slot, block, data) | 32 bytes |
| Write OTP zone | WriteOTP32Bytes(offset, data) | 32 bytes |
| Get public key | GetPublicKey(slot) | EC public key |
| Create private key | NewPrivateKey(slot) | ECDSA key |
| Sign digest | SignDigest(slot, digest) | ECDSA signature |
| Verify signature | ECDSASignature::VerifyDigest(key, digest) | bool |

Evidence:
- CryptoInterface::BlockSize constant (32 bytes, assertion at 0x389077)
- "data.size() == CryptoInterface::BlockSize" assertion
- "offset % CryptoInterface::BlockSize == 0" assertion
- Source: `/workspace/monorepo/graph-io/devices/crypto_device/src/crypto_device/`

### Device Authentication Flow

The crypto IC is used for device authentication:

1. Host reads crypto IC serial number
   - "Error Reading Cyrpto serial number" at 0x40aaa3 (note typo: "Cyrpto")
2. Challenge-response:
   - "nonce not returned" at 0x36e87a
   - "nonce mismatch" at 0x39593c
   - "CHALLENGE_IS_DIFFERENT" at 0x373ad1
   - "challenge is different" at 0x3d614a
   - "invalid challenge length" at 0x416873
3. Signature verification using ECDSA
4. Public key retrieval

### Hardware Revision Determines Crypto Path

```cpp
bool RevisionDetail::UsesNewCryptoInterface()
```
String at 0x3d1d4c. This method on the RevisionDetail object determines
whether the device uses the old I2C-based crypto interface or the newer
FPGA-tunneled version. The "new" crypto interface is likely CryptoInterfaceFpga
or CryptoInterfacePro.

### I2C Wake Token

The crypto IC requires a wake-up sequence on the I2C bus before
communication:
- "Failed to send i2c wake token" at 0x427847

This is a zero-byte I2C write to address 0x00 (general call) that wakes
the ATECC chip from its sleep state.


## MSO I2C / EEPROM (AT24C64D)

### MSO-Specific EEPROM: AT24C64D

MSO devices use an AT24C64D EEPROM (Atmel/Microchip, 64 Kbit = 8192 bytes):

Source file: `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/component/eeprom_at24c64d.cpp`

Constants:
- Page size: 32 bytes (MOV ecx, 0x20 at VA 0x2d6e189, 0x2d6e289)
- Total size: 8192 bytes (EepromSize constant)
- Assertion: "begin_address + data.size() <= EepromSize"
- Assertion: "byte_count <= PageSizeBytes"

### MSO EEPROM Operations

```cpp
void Saleae::Eeprom_AT24C64D::Write(uint32_t address, gsl::span<uint8_t> data)
```

The Write function:
1. Validates address + data.size() <= EepromSize (8192)
2. Validates data fits in a single page (byte_count <= PageSizeBytes, 32)
3. Logs "EEPROM: Writing {} bytes to {}"
4. Performs I2C write through the MSO I2C tunnel

```
Read:
1. Logs "EEPROM: Reading {} bytes from {}"
2. Performs I2C read through the MSO I2C tunnel
```

### MSO AFE EEPROM

The MSO EEPROM stores AFE calibration data:

- `MsoDeviceInterface::BuildAfeEeprom()` -- constructs EEPROM accessor
- `/afeConfigCalibrations` -- JSON path for calibration data
- `/afeConfig` -- JSON path for AFE configuration
- `MsoAfeMetadata::CalibrationConstants` -- calibration data structure
- `MetadataConstantsToCalibration()` / `CalibrationToMetadataConstants()` --
  conversion between calibration profiles and EEPROM metadata format
- "MSO Device Metadata" at 0x37db47
- "/deviceMetadata" at 0x362ef5
- "Failed to read device metadata" at 0x3fb746
- "Failed to deserialize device metadata" at 0x441a43

### MSO I2C Bus Devices

The MSO has multiple I2C devices:

1. **EEPROM (AT24C64D)** -- AFE calibration, device metadata
2. **Crypto IC** -- device authentication
3. **I2C rail control**:
   - "I2C_RAIL_EN1" at 0x3dfbf6
   - "I2C_RAIL_EN2" at 0x441d93
   These are likely GPIO pins that enable different I2C bus segments.

### I2C Serialization Format

The MSO uses a serialized format for I2C transfers:

- `SerializeI2cTransfer(&bb, &transfer)` at 0x42d584
- Uses ByteBuffer serialization (ByteBuffer_WriteBytes, ByteBuffer_SerializeBytes)
- `Saleae::I2cFormatter` class (RTTI: `N6Saleae12I2cFormatterE`)

This suggests I2C operations are serialized into a binary buffer format
before being sent over USB to the MSO device.


## Calibration Data Path

### Server-Side vs EEPROM-Side Calibration

Based on the complete analysis, calibration data comes from TWO sources:

1. **Server-hosted `.cal` files** -- for Logic 4/8/Pro 8/Pro 16
   - Downloaded from `https://downloads.saleae.com/calibration/{deviceIdHex}.cal`
   - Contains IIR biquad filter coefficients, ADC-to-voltage transforms
   - See `doc/calibration/calibration-format.md` for full format

2. **On-device EEPROM** -- for MSO devices only
   - AT24C64D stores AFE calibration constants
   - Read during device initialization
   - Converted to MsoCalibrationProfile via MetadataConstantsToCalibration()

### Calibration Data Flow for Logic Pro 8/16

```
1. Device connects, firmware loads, FPGA programmed
2. CommonDeviceInit reads device ID from EEPROM
   - Fx3DeviceFeatures::ReadEeprom() via EP1 opcode 0x88
   - "Logic Pro {} Device ID: {:#x}"
3. Device ID sent to JS frontend
4. JS frontend downloads calibration from Saleae servers
   - CalibrationManager.getCalibration()
   - Uses device ID as lookup key
5. Calibration sent back to native library
   - SetCalibration, SetAdcFullScaleGain
6. Calibration applied during analog capture
```

The EEPROM on Logic Pro 8/16 stores the device ID and hardware revision,
NOT the calibration data itself. Calibration is server-hosted.

### Calibration Data Flow for MSO

```
1. Device connects, firmware loads
2. MsoDeviceInterface::Initialize(MsoDeviceMetadata)
3. BuildAfeEeprom() constructs EEPROM accessor
4. Read device metadata from AT24C64D EEPROM
   - AFE calibration constants
   - AfeConfig_RawHardware settings
5. MetadataConstantsToCalibration() converts to profile
6. Calibration applied to oscilloscope channels
```

For MSO, calibration data IS stored on the device EEPROM.


## EP1 I2C Command Summary

| Opcode | Name | Direction | Use Case |
|--------|------|-----------|----------|
| 0x55 | WriteEeprom | EP1 OUT | Write EEPROM data (device provisioning) |
| 0x87 | WriteI2C | EP1 OUT | Raw I2C write (crypto IC, etc.) |
| 0x88 | ReadI2c | EP1 OUT | I2C read tunnel (EEPROM read, crypto IC) |

Note: Opcode 0x89 (Device2_1) also appears in this code region and may
be another I2C-related command or device info query.


## I2C Bus Hardware

### GPIO Control

- "I2C_RAIL_EN1" and "I2C_RAIL_EN2" -- GPIO pins controlling I2C bus segments
- The FX3 firmware initializes I2C in `LogicProApplicationInit()`:
  - `InitI2C()` called during startup
  - "InitI2C failed, %d" on error

### FX3 SDK I2C Functions

The FX3 firmware uses Cypress SDK I2C functions (CyU3PI2C*) accessed
through the LPP_THREAD (05_LPP_THREAD -- Low Performance Peripherals).
These are called from within command handlers to tunnel host I2C requests
to the physical I2C bus.


## TestEeprom

`LogicProfessionalDevice::TestEeprom()` validates EEPROM integrity:

- Source: `/workspace/monorepo/graph-io/devices/logic_interface/src/LogicProfessionalDevice.cpp`
- On failure: "EEprom error detected!" at 0x3ee91a
- Likely reads and verifies a known pattern or checksum in the EEPROM

### WipeEeprom

The `WipeEeprom` operation at 0x3a5b4b clears EEPROM contents:

- Source: `LogicAnalyzerDevice.cpp`
- Logs "wiping eeprom" at 0x43511a
- Used during device reprovisioning or factory reset


## Code Regions

### Host-Side Code (.text section, file offsets with +0x1000 VA delta)

| VA Range | Purpose |
|----------|---------|
| 0x1c44200-0x1c44300 | LogicProfessionalDevice::TestEeprom() |
| 0x1c50800-0x1c50900 | WipeEeprom |
| 0x1c50b00-0x1c50d00 | Fx2DeviceFeatures::WriteEeprom (opcode 0x55 construction) |
| 0x1c50e00-0x1c51200 | Fx2DeviceFeatures::ReadEeprom (assertions, size checks) |
| 0x1c51500-0x1c51900 | WriteI2C (opcode 0x87) and ReadI2c (opcode 0x88) construction |
| 0x1c51900-0x1c51E00 | Device2_1 (opcode 0x89), SetIdToDevice, WriteRead-Write/Read |
| 0x1c51E00-0x1c52400 | Fx3DeviceFeatures::WriteEeprom (opcode 0x55, with size checks) |
| 0x1c52500-0x1c52800 | Fx3DeviceFeatures::ReadEeprom (assertions, size checks) |
| 0x1c52900-0x1c53200 | Firmware CY header check, opcode 0x88 send, request/response logging |

### MSO Code

| VA Range | Purpose |
|----------|---------|
| 0x2d52e00-0x2d56000 | MSO I2C tunnel (error handling, timeouts) |
| 0x2d6ce00-0x2d6e200 | Eeprom_AT24C64D (Write, Read, page management) |
| 0x2da2700-0x2da2900 | CryptoInterfaceI2c::ReadTransmission |

### Firmware Code (inside embedded FX3 firmware blobs)

| FW offset | .so offset (Pro8) | Purpose |
|-----------|-------------------|---------|
| 0x8190 | 0x7a6930 | "InitI2C failed, %d" |


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| WriteEeprom uses opcode 0x55 | Confirmed | Immediate value 0x55 at 3 code sites + assertion strings |
| ReadI2c uses opcode 0x88 | Confirmed | Immediate value 0x88 at 3 code sites + request/response strings |
| WriteI2C uses opcode 0x87 | High | Immediate value 0x87 at 1 code site + WriteI2C request string |
| FX2 EEPROM = 256 bytes, 64-byte pages | Confirmed | String + immediate values 0x100, 0x40 |
| MSO AT24C64D = 8192 bytes, 32-byte pages | Confirmed | Source file path + assertion + immediate 0x20 |
| Three crypto interface paths | Confirmed | RTTI type names for all three classes |
| Device ID stored in EEPROM | Confirmed | Multiple device ID format strings with EEPROM read |
| Server-hosted calibration for Logic devices | Confirmed | bundle.js CalibrationManager (see calibration-format.md) |
| EEPROM-stored calibration for MSO | High | AfeMetadata, BuildAfeEeprom, device metadata strings |
| I2C wake token for crypto IC | High | String + crypto IC initialization pattern |
| Two I2C rails (GPIO controlled) | High | I2C_RAIL_EN1, I2C_RAIL_EN2 strings |

### Unknown / Needs Ghidra

1. Exact ReadI2c (0x88) payload format -- the I2C slave address and read
   length encoding needs decompilation to determine byte positions
2. Exact WriteI2C (0x87) payload format -- same
3. FX3 EepromSizeMax() return values per device type
4. I2C slave addresses for EEPROM and crypto IC on each device
5. EEPROM data layout (offset map for device ID, revision, etc.)
6. ReadI2c vs WriteEeprom -- does ReadI2c always read from EEPROM, or is
   it a general-purpose I2C read tunnel?


## References

- `doc/protocol/ep1-commands.md` -- EP1 command protocol (opcodes 0x55, 0x88)
- `doc/calibration/calibration-format.md` -- server-hosted calibration format
- `re/notes/ep1-command-protocol.md` -- command protocol analysis
- `re/notes/fx3-usb-events.md` -- firmware I2C initialization
- `re/notes/device-class-hierarchy.md` -- device class hierarchy and DeviceFeatures
- AT24C64D datasheet -- 64Kbit I2C EEPROM, 32-byte pages, I2C address 0x50-0x57
- ATECC508A/608A datasheet -- Microchip crypto authentication IC
- Cypress FX3 SDK CyU3PI2C API -- I2C master functions
