# LogicGraduateDevice (Logic 8) Class Analysis

Analysis date: 2026-03-10
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0074
Method: RTTI relocation analysis, Ghidra headless decompilation, cross-reference with
        existing device class hierarchy (TASK-0089), multi-device comparison (TASK-0073)


## Summary

LogicGraduateDevice is the concrete device class for the Saleae Logic 8
(VID:PID 21a9:1004, external DeviceType 5, internal DeviceType 4). It inherits
from Logic2FpgaDevice, which in turn inherits from LogicAnalyzerDevice.

The class is structurally similar to LogicProfessionalDevice but simpler: no
VGA/attenuator control, no full EP1 encryption, fixed 8-channel configuration
with all channels both digital and analog, and bandwidth-gated sample rates
with a 200 MHz digital / 20 MHz analog maximum.

Source file: `/workspace/monorepo/graph-io/devices/logic_interface/src/LogicGraduateDevice.cpp`


## Vtable Layout

Vtable VA: 0x2e48688 (offset_to_top) / 0x2e48690 (typeinfo ptr) / 0x2e48698 (slot 0)
Typeinfo VA: 0x2e48720
17 virtual method slots.

| Slot | ELF VA      | Name / Purpose                   | Shared With        |
|------|-------------|----------------------------------|--------------------|
| 0    | 0x1c3d790   | ~LogicGraduateDevice() [D1]      | Unique (Graduate)  |
| 1    | 0x1c3d7d0   | ~LogicGraduateDevice() [D0]      | Unique (Graduate)  |
| 2    | 0x1c3d810   | Initialize                       | Unique (Graduate)  |
| 3    | 0x1c4f910   | StopDeviceThunk (calls slot 12)  | Shared (L2FD, Pro) |
| 4    | 0x1c4e7c0   | GetForcedSpeedFlag               | Shared (L2FD, Pro) |
| 5    | 0x1c4fa90   | GetSomeConfig (12 bytes @ +0xa8) | Shared (L2FD, Pro) |
| 6    | 0x1c3d970   | GetUsbSpeed                      | Unique (Graduate)  |
| 7    | 0x1c3d990   | GetDefaultUsbSpeed (returns 2)   | Unique (Graduate)  |
| 8    | 0x1c3d9a0   | SetCaptureParametersWrapper      | Unique (Graduate)  |
| 9    | 0x1c3e0a0   | GetDeviceSettings (constructor)  | Unique (Graduate)  |
| 10   | 0x1c3e5b0   | StartSampling                    | Unique (Graduate)  |
| 11   | 0x1c50230   | ComputeTransferSize              | Shared (all FX3)   |
| 12   | 0x1c3f700   | StopDevice                       | Unique (Graduate)  |
| 13   | 0x1c3f920   | GetPublicKey (65-byte ECDSA key) | Unique (Graduate)  |
| 14   | 0x1c24310   | ReadEepromBlock (16 bytes)       | Shared (L2FD, Pro) |
| 15   | 0x1c3fa20   | WriteRegisterArray (up to 16)    | Unique (Graduate)  |
| 16   | 0x1c3fcf0   | CreateCryptoInterface            | Unique (Graduate)  |

L2FD = Logic2FpgaDevice, Pro = LogicProfessionalDevice


## Slot Details

### Slot 0/1: Destructors

Standard C++ destructors. D1 (complete) stores the Graduate vtable ptr, calls
slot 12 (StopDevice), then chains to the Logic2FpgaDevice D1 destructor at
0x1c20660. D0 (deleting) does the same then calls operator_delete.

### Slot 2: Initialize (0x1c3d810, 286 bytes)

Called during device initialization. Steps:

1. Calls vtable slot at +0x20 (param_1->vtable[4] = GetForcedSpeedFlag). If
   forced speed is NOT set:
   - Calls FUN_01d23840 (ReadHwRevision) to read the hardware revision
   - Stores revision data at param_1[0x43..0x49]
   - Logs: "Hardware revision: {}" (source line 0x4f)
   - Calls FUN_01d21fb0 (CommonDeviceInit) -- the shared initialization
     sequence that sets up ADC, FPGA, JESD link, and crypto
2. Starts a crypto authentication sequence (FUN_02e902c0)

The revision is read by sending register address 0x20 to the FPGA and
parsing the response as "REV{major}.{minor}" (see ReadHwRevision analysis).

### Slot 6: GetUsbSpeed (0x1c3d970, 27 bytes)

```c
uint GetUsbSpeed(LogicGraduateDevice *this) {
    if (this->forcedSpeed != 0) {  // offset +0xa0
        return 1;  // USB 2.0 when forced
    }
    return UsbDevice::GetSpeed(this->usbDevice);  // offset +0x90
}
```

Key finding: when forced speed is set, Logic 8 defaults to USB 2.0 (speed=1),
unlike Pro devices which default to USB 3.0 (speed=2). This matches the
multi-device-differences.md findings.

### Slot 7: GetDefaultUsbSpeed (0x1c3d990, 6 bytes)

```c
uint GetDefaultUsbSpeed() {
    return 2;  // USB 3.0 SuperSpeed (when not forced)
}
```

Returns constant 2 (USB 3.0). This appears to be the default expected USB
speed before actual speed detection. Note the difference from GetUsbSpeed
slot 6 which returns 1 when forced. This suggests the device supports USB 3.0
but falls back to USB 2.0 when `SALEAE_FORCE_SPEED` environment variable is set.

### Slot 8: SetCaptureParametersWrapper (0x1c3d9a0, 1687 bytes)

This is a Logic 8-specific wrapper that:

1. Checks if forced speed is NOT set (offset +0xa0 == 0)
2. Runs IsConnectedTest (0x1c50530) -- if it fails, throws
   "IsConnectedTest Failed: {error}"
3. Builds a register command vector with these bytes:
   - 0x7b (register address)
   - 0xb8 (register value)
   - 0xc2 (register value)
   - 0x00 (register value)
   - 0xff (register value)
   - param_2 (caller-supplied)
   - param_3 (caller-supplied)
   - param_4 (caller-supplied)
4. Sends the register array via UsbDevice->WriteRegisters (vtable offset 0x20
   on the UsbDevice at offset +0x90 in the LogicGraduateDevice object)

The register writes use opcode 0x17 through FpgaDeviceFeatures vtable. This
is a WriteRegister command writing capture configuration to the FPGA.

The fixed bytes (0x7b, 0xb8, 0xc2, 0x00, 0xff) are likely FPGA configuration
register addresses (channel enables, clock dividers, etc.) while param_2-4
are the dynamic capture parameters (sample rate divisor, channel mask, etc.).

### Slot 9: GetDeviceSettings (0x1c3e0a0, 1145 bytes)

Builds the DeviceSettings object for Logic 8. Steps:

1. Calls thunk_FUN_01d484c0 to initialize DeviceSettings with:
   - 5 performance levels
   - Max digital rate from param_2[0x15] (device ID area)
   - Max analog rate from param_2[0x16]
   - Active flag from param_2[0x14]

2. Adds 8 channel configurations via FUN_01d4c000:
   ```c
   // Called 8 times with identical parameters:
   FUN_01d4c000(0, 0x40a00000, local_180, 2, 1, 1);
   ```
   - param_1 = 0 (channel base type: digital)
   - param_2 = 0x40a00000 (IEEE 754 float = 5.0, the max voltage)
   - param_3 = DeviceSettings pointer
   - param_4 = 2 (channel type: both digital and analog)
   - param_5 = 1 (digital capability: toggleable)
   - param_6 = 1 (analog capability: toggleable)

   All 8 channels are identical and fully toggleable between digital and
   analog modes. This differs from Logic 4 (which has restricted channels)
   and Logic Pro 16 (which has 16 channels, upper 8 digital-only at USB 2.0).

3. Gets USB speed via vtable[6] (GetUsbSpeed) and stores it in DeviceSettings

4. Copies digital rate array from runtime globals DAT_0307aa30/38
   (u32 elements, populated at startup)

5. Copies analog rate pair array from runtime globals DAT_0307aa48/50
   (u64 elements = physical_rate:u32 + output_rate:u32)

6. Validates digital rates: `200000000.0 / rate` must be an integer
   (assertion at LogicGraduateDevice.cpp line 0xa9). This confirms the max
   digital clock is 200 MHz and all rates must be integer divisors.

7. Validates analog rates: `20000000.0 / rate` must be an integer
   (assertion at LogicGraduateDevice.cpp line 0xb2). This confirms the max
   analog clock is 20 MHz.

8. Calls FUN_01d4c2a0 (BuildSampleRateOptions) with the rate arrays and
   bitsPerSample = 0xc (12).

The BuildSampleRateOptions function:
- Copies digital rates to DeviceSettings+0x108
- Copies analog rate pairs to DeviceSettings+0x120
- Sorts digital rates descending (highest first)
- Sorts analog rate pairs descending
- Sets bitsPerSample at DeviceSettings+0x68 = 12

### Slot 10: StartSampling (0x1c3e5b0, 1948 bytes)

The capture start sequence for Logic 8:

1. Calls FUN_01d4bbb0 to extract sample rate from DeviceSettings
2. Calls vtable[0x58/8=11] (ComputeTransferSize) to determine USB transfer size
3. Stores transfer size at param_1[0x51]
4. Checks if device has been initialized (param_1[0x14] flag)

**If initialized (param_1[0x14] != 0)** -- the "already connected" fast path:
- Joins any existing read thread (param_1[0x26])
- Cleans up existing channel processing state
- Iterates over channels, creates channel processing entries
- Allocates new DeviceSettings copy (0x150 bytes via FUN_01d48610)
- Calls PrepareCapture (0x1c50310): locks mutex, clears capture flags
- Creates a new read thread via CreateReadThread (0x1c3ffb0)
- Thread entry: FUN_01d407f0 -> calls FUN_01d26880 (the actual capture loop)

**If NOT initialized** -- the full startup path:
- Runs IsConnectedTest; if failed, logs error and stores disconnect event
- Calls FUN_01d21c90 (CommonDeviceInit): initializes FPGA, ADC, crypto
- Calls FUN_01d24ca0 (SetCaptureParameters): writes capture config to FPGA
- Sends EP1 command to stop any existing stream: `[0x02]` via vtable[0x20]
  on UsbDevice at offset +0x12 (the FpgaDeviceFeatures command pipe)
- Asserts `!mReadStreamId.has_value()` (source line 0x105)
- Sets up USB read stream parameters:
  - Transfer size from slot 11 (ComputeTransferSize)
  - Timeout: 1000 ms
  - Max queue: 0x40000000000 (4 TB, effectively unlimited)
  - Max memory: 0x40000000 (1 GB)
  - Reserved: 0
  - Max time: 5,000,000,000 ns (5 seconds)
- Registers three callbacks:
  - 0x1c3f150 (BufferCallback): sends EP1 `[0x01]` to start streaming,
    then processes received data
  - 0x1c29370 (shared callback, likely progress/timing)
  - 0x1c3f350 (ErrorCallback): handles read stream errors
- Starts the USB read stream via UsbDevice vtable[0x48]
- Stores stream ID at param_1[0x52], sets has_value flag at param_1[0x53]

The capture start command is EP1 byte `0x01` (start stream) and stop is
EP1 byte `0x02` (stop stream).

### Slot 12: StopDevice (0x1c3f700, 326 bytes)

Called from the destructor and from slot 3 (StopDeviceThunk).

If NOT in forced-speed mode (offset +0xa0 == 0):
- Logs "Stopping Device!" (source line 0x120)
- If a read stream is active (offset +0x298 != 0):
  - Calls UsbDevice::StopReadStream(streamId, waitForCompletion=true)
  - Clears the stream active flag

If in forced-speed mode:
- If a thread exists at offset +0x130:
  - Calls FUN_01d50350 (signal thread to stop)
  - Joins the thread

### Slot 13: GetPublicKey (0x1c3f920, 241 bytes)

Returns a 65-byte (0x41) XOR-deobfuscated blob. The data is stored at ELF VA
0x2eeda20..0x2eeda68, XOR'd against a key at 0x2eeda68.

The 65-byte size matches an uncompressed SEC1-format ECDSA P-256 public key
(0x04 prefix + 32-byte X + 32-byte Y). This is used for crypto authentication
with the ATECC508A/608A on the device.

Decrypted key (hex):
```
04 27 19 04 df 39 5c 85 11 36 68 22 23 16 31 6b
12 ee 9c d4 9b 9b 13 23 f8 02 85 de 51 cc dd f7
ea 62 49 5c ab f8 e9 5c 3f 82 95 4e a9 98 64 8b
c5 36 35 51 14 50 82 69 6e 17 67 e3 33 09 3c 7c
0c
```

The first byte 0x04 confirms this is an uncompressed EC point, consistent with
ECDSA P-256 (secp256r1).

### Slot 14: ReadEepromBlock (0x1c24310, 95 bytes, shared)

Reads a 16-byte block from the device EEPROM via the FpgaDeviceFeatures
interface. Uses EP1 read command at vtable offset 0x18 with register address
0x10 and length 0x10 (16 bytes).

### Slot 15: WriteRegisterArray (0x1c3fa20, 712 bytes)

Writes up to 16 register values from a vector to the FPGA via
FpgaDeviceFeatures. The function iterates through the vector bytes (up to 16)
and calls FpgaDeviceFeatures vtable[0x10/8=2] (WriteRegister) for each byte
with register address 0x17 and the byte value.

The deeply nested if-chain handles variable-length vectors up to 16 elements.
If the vector has more than 16 elements, throws out_of_range.

Register 0x17 is used consistently for all writes, suggesting this is a
serialized register port where multiple bytes form a compound configuration
command to the FPGA.

### Slot 16: CreateCryptoInterface (0x1c3fcf0, 206 bytes)

Creates the crypto authentication interface based on the CryptoType field
at offset +0x220:

- CryptoType 0: assertion failure (no crypto, should not happen)
- CryptoType 1: Creates CryptoInterfaceFpga (vtable PTR_FUN_02f48970)
  - Allocates 0x28 bytes
  - Initialized with FUN_01d594d0 (FpgaDeviceFeatures at offset +0x180)
- CryptoType 2: Creates CryptoInterfaceI2c (via FUN_01d5acc0)
  - Uses USB device at offset +0x80

This confirms Logic 8 uses CryptoInterfaceFpga (type 1) for the standard
Xilinx hardware revision, which routes ATECC508A commands through FPGA
register writes. For Lattice hardware revisions (type 2), it falls back to
CryptoInterfaceI2c.


## Constructor Analysis (0x1c3d1a0, 1097 bytes)

The constructor:

1. Creates a mutex (FUN_02ed1290)
2. Increments shared_ptr ref count on the USB device
3. Calls Logic2FpgaDevice constructor (FUN_01d201f0) with:
   - Max digital rate: 200,000,000 (200 MHz)
   - ADC calibration data pointer at 0x1312d00 (8 bytes)
4. Sets vtable pointer to LogicGraduateDevice vtable (PTR_FUN_02f48698)
5. Initializes state flags (offset +0x52 = 0, +0x53 = 0)
6. Creates three EP1 endpoint objects:
   - EP1 OUT (0x01) via FUN_01d4e5e0 -> stored at offset +0x2d
   - EP1 IN (0x81) via FUN_01d4e5e0 -> stored at offset +0x2e
   - EP2 IN (0x82) via FUN_01d4e5e0 -> stored at offset +0x2f
7. Creates FpgaDeviceFeatures (NOT ProFpgaDeviceFeatures):
   - 0x28 bytes via FUN_01d509a0
   - Uses EP1 OUT and EP1 IN endpoints
   - Stored at offset +0x10 (overwriting any parent class value)
8. Creates a second features object:
   - Also 0x28 bytes via FUN_01d53440
   - Also uses EP1 OUT and EP1 IN endpoints
   - Stored at offset +0x30
9. Reads device ID three times via ReadDeviceId (FUN_01d51a30):
   - Reads 8 bytes from EEPROM register 0x08
   - Compares all three reads for consistency
   - If 2 of 3 match, uses the matching value with a warning
   - If all 3 differ, throws "Logic 8 Device ID read complete failure"
   - Stores device ID at offset +0x15
10. Sets internal device type = 5 at offset +0x16
    (This is the external DeviceType, matching the table)
11. Logs "Logic 8 Device ID: {:#x}"


## Object Layout (Partial)

```
LogicGraduateDevice extends Logic2FpgaDevice:
  +0x0000: vtable pointer (LogicGraduateDevice vtable)
  +0x0008..0x000f: (inherited from GenericDevice)
  +0x0010: shared_ptr<FpgaDeviceFeatures> (EP1 command interface)
  +0x0012: shared_ptr<FpgaDeviceFeatures> (second features? for FPGA reg)
  +0x0014: initialized flag (bool)
  +0x0015: device ID (8 bytes, from EEPROM)
  +0x0016: internal device type (u32, = 5 for Logic 8)
  +0x0026: read thread (pthread_t)
  +0x0027: stop flag (atomic bool)
  +0x002d: EP1 OUT endpoint object
  +0x002e: EP1 IN endpoint object
  +0x002f: EP2 IN endpoint object (bulk data)
  +0x0030: second features object
  +0x0034: channel processing vector (start)
  +0x0035: channel processing vector (end)
  +0x0036: channel processing vector (capacity)
  +0x0037: digital rate divisor (u32, from DeviceSettings)
  +0x003b: current DeviceSettings copy (heap allocated)
  +0x0043..0x0049: hardware revision data
  +0x0044: ADC type (from RevisionDetail)
  +0x0047..0x0049: revision string data
  +0x004b: calibration data reference
  +0x0051: computed transfer size (u32)
  +0x0052: read stream ID (u64)
  +0x0053: has_read_stream flag (bool)
  +0x0080: shared_ptr<UsbDevice> (low-level USB)
  +0x0090: shared_ptr<UsbDevice> (same? or secondary)
  +0x00a0: forced speed flag (bool)
  +0x00a8: config bytes (12 bytes, returned by slot 5)
  +0x00b0: device type for revision lookup (u32)
  +0x00d0: capture active flag
  +0x00d8: capture mutex
  +0x0130: fake read thread (for forced-speed/offline mode)
  +0x0138: fake read stop flag
  +0x0168: EP1 command context (passed to WriteRegisters)
  +0x0180: shared_ptr<FpgaDeviceFeatures> (used for register writes)
  +0x0188: JESD/ADC controller object (from CommonDeviceInit)
  +0x0208: crypto interface factory args
  +0x0218: ADC type enum (0=HMCAD1100/Xilinx, 1=AD9637, 2=ADS528x/Lattice)
  +0x021c: crypto type flag
  +0x0220: crypto interface type (0=none, 1=FPGA, 2=I2C)
  +0x0228..0x0230: LFSR seed data
  +0x0278..0x0280: JESD lane params
  +0x0290: read stream ID (for StopDevice)
  +0x0298: read stream active flag
```

Note: offsets above 0x100 are based on the decompiled code patterns and may
have alignment gaps. Byte offsets are multiples of 8 (pointer-aligned).


## Key Differences from LogicProfessionalDevice

| Feature                    | Logic 8 (Graduate)         | Logic Pro 8 (Professional)    |
|----------------------------|----------------------------|-------------------------------|
| Channel count              | 8                          | 8                             |
| Channel config             | All same (dig+analog, 5V)  | All same (dig+analog, varies) |
| Max digital rate           | 200 MHz                    | 500 MHz                       |
| Max analog rate            | 20 MHz                     | 50 MHz (100 MHz USB3)         |
| ADC bits per sample        | 12                         | 12                            |
| Validation divisor (dig.)  | 200,000,000                | (different constant)          |
| Validation divisor (ana.)  | 20,000,000                 | (different constant)          |
| DeviceFeatures class       | FpgaDeviceFeatures         | ProFpgaDeviceFeatures         |
| Register write verification| No (fire-and-forget)       | Yes (read-back check)         |
| EP1 command encryption     | FW download only           | All commands                  |
| Default forced USB speed   | 1 (USB 2.0)               | 2 (USB 3.0)                   |
| Crypto interface           | CryptoInterfaceFpga        | CryptoInterfacePro            |
| VGA/attenuator control     | No                         | Yes (LMH6518 + attenuators)   |
| Voltage threshold ctrl     | No (fixed 5V)              | Yes (1.2V, 1.8V, 3.3V)       |
| FPGA reg addr 0x17         | Direct byte writes         | N/A (uses different path)     |
| Max voltage (float)        | 5.0 (0x40a00000)           | Configurable per channel      |
| Public key                 | 65 bytes (XOR-obfuscated)  | Different key (same format)   |


## Key Differences from LogicStudentDevice (Logic 4)

| Feature                    | Logic 8 (Graduate)         | Logic 4 (Student)             |
|----------------------------|----------------------------|-------------------------------|
| USB controller             | FX3 (ARM9)                 | FX2LP (8051)                  |
| Inherits from              | Logic2FpgaDevice           | LogicAnalyzerDevice           |
| Channel count              | 8                          | 4                             |
| Channel config             | All toggleable             | 1 toggleable + 3 analog-only  |
| Max digital rate           | 200 MHz                    | 12 MHz                        |
| Max analog rate            | 20 MHz                     | 6 MHz                         |
| ADC bits per sample        | 12                         | 8 (HMCAD1100)                 |
| Sample rate selection      | Bandwidth-gated (dynamic)  | 10 fixed presets              |
| Firmware format            | FX3 CY image               | Intel HEX                     |
| Performance tier           | AnalogBasic                | AnalogBasic                   |
| FPGA bitstream chunk       | 62 bytes (USB2), larger(USB3)| 62 bytes                    |


## Hardware Revision Handling

The Initialize function (slot 2) reads the hardware revision from the FPGA
by sending register address 0x20 and parsing the 6-byte response as
"REV{major}.{minor}". The revision selects:

- **ADC type**: HMCAD1100 (rev 0,1) vs Lattice ADS528x (rev 3+)
- **FPGA bitstream**: Xilinx Spartan-6 vs Lattice ECP5
- **Crypto interface**: FPGA-routed (rev 0,1) vs I2C (rev 7,8)
- **Calibration parameters**: Per-revision tables at 0x2fe5bc0..0x2fe5ce0

The revision override environment variable `SALEAE_REVISION_OVERRIDE` is
checked, allowing forced revision values in format "X.Y.Z". This is used
for development and testing.

The CommonDeviceInit (FUN_01d21fb0) performs these steps based on revision:

1. FPGA responsiveness check: writes 0xAA to register 0x7F twice, reads
   back, expects 0xAA response
2. ADC initialization based on AdcType:
   - Type 0 (HMCAD1100/Xilinx): calls InitHmcad1100 with device-specific
     settings. For Logic 8 (device type 5): channel=5, input=6, mode=2.
     For Pro (device type 6|7): channel=4, input=7, mode=0.
   - Type 1 (AD9637): calls SetAd9637Enabled
   - Type 2 (ADS528x/Lattice): JESD204B link initialization with register
     writes to 0x2a, 0x2b, 0x42, 0x46 and sleep delays
3. For Pro devices only (device type & 0xfffffffe == 6): additional register
   writes to register 0x15 and clock configuration
4. Calibration data loading


## Capture Protocol (Logic 8 Specific)

1. **Connection test**: EP1 ping via TestFirmwareResponsivenessAndReseed
2. **CommonDeviceInit**: ADC + FPGA + JESD initialization
3. **SetCaptureParameters**: FPGA register configuration for sample rate,
   channel enables, clock dividers
4. **Stop existing stream**: EP1 command `[0x02]`
5. **Start USB read stream**: with callbacks for data, progress, and errors
6. **BufferCallback sends**: EP1 command `[0x01]` to begin data flow
7. **Data streams** on EP2 IN (0x82) until stop
8. **StopDevice**: calls UsbDevice::StopReadStream with wait=true

Stream commands on EP1:
- `0x01` = start streaming
- `0x02` = stop streaming


## Confidence Assessment

| Finding                              | Confidence   | Evidence                           |
|--------------------------------------|-------------|-------------------------------------|
| 17 vtable entries                    | Confirmed   | Relocation table extraction         |
| Constructor at 0x1c3d1a0             | Confirmed   | Ghidra decompilation                |
| 8 identical channels (dig+analog)    | Confirmed   | 8x FUN_01d4c000 with same params    |
| Max digital 200 MHz                  | Confirmed   | floor(200000000.0/rate) assertion    |
| Max analog 20 MHz                    | Confirmed   | floor(20000000.0/rate) assertion     |
| 12 bits per sample                   | Confirmed   | BuildSampleRateOptions param = 0xc  |
| Uses FpgaDeviceFeatures (not Pro)    | Confirmed   | Constructor calls 0x1c509a0         |
| Device type 5 (external enum)        | Confirmed   | Constructor stores *(+0x16) = 5     |
| Max voltage 5.0V (float 0x40a00000)  | Confirmed   | AddChannelConfig param              |
| GetPublicKey returns 65-byte EC key  | Confirmed   | Allocates 0x41, first byte = 0x04   |
| Stream start = EP1 0x01              | Confirmed   | BufferCallback sends [0x01]         |
| Stream stop = EP1 0x02               | Confirmed   | StartSampling sends [0x02]          |
| Device ID read 3x for consistency    | Confirmed   | Constructor triple-read + compare   |
| CryptoInterfaceFpga for type 1       | Confirmed   | CreateCryptoInterface switch         |
| CryptoInterfaceI2c for type 2        | Confirmed   | CreateCryptoInterface switch         |
| Register 0x17 for FPGA config        | High        | WriteRegisterArray uses 0x17        |
| Register 0x08 for device ID          | High        | ReadDeviceId reads 8 bytes @ 0x08   |
| Register 0x20 for revision           | High        | Initialize reads 6 bytes @ 0x20    |
| Forced speed returns USB 2.0 (1)     | Confirmed   | GetUsbSpeed if-branch returns 1     |


## Open Questions

1. **Rate arrays at runtime**: The digital and analog rate arrays are stored
   in BSS globals (DAT_0307aa30/38/48/50) and populated at program startup.
   The exact values cannot be extracted statically. The divisibility constraints
   (200 MHz for digital, 20 MHz for analog) limit them to integer divisors.

2. **Second features object**: The constructor creates two FpgaDeviceFeatures
   objects (at +0x10 and +0x30) using identical constructors but different
   vtable pointers (PTR_FUN_02f48ec8 vs PTR_FUN_02f48fb8). The second may
   be a read-only features interface or a separate command channel.

3. **FPGA register 0x17 semantics**: WriteRegisterArray writes up to 16
   bytes to register 0x17 serially. This may be a FIFO-style configuration
   port rather than a single 8-bit register. The byte sequence likely encodes
   capture parameters (channel mask, clock config, etc.) but the exact format
   is unknown.

4. **The 0x7b/0xb8/0xc2/0x00/0xff prefix**: SetCaptureParametersWrapper
   prepends these 5 fixed bytes before the dynamic parameters. These may be
   FPGA command opcodes or fixed configuration values.

5. **Calibration data at 0x1312d00**: The constructor passes 8 bytes from
   this .rodata address to Logic2FpgaDevice. This likely contains ADC
   calibration constants or factory default offsets.


## References

- `re/notes/device-class-hierarchy.md` -- RTTI hierarchy (TASK-0089)
- `re/notes/multi-device-differences.md` -- Device comparison (TASK-0073)
- `re/notes/device-type-enum.md` -- DeviceType mapping (TASK-0089)
- `re/notes/fpga-features-vtable.md` -- FpgaDeviceFeatures vtable (TASK-0045)
- `re/notes/sample-rate-options.md` -- Rate selection system (TASK-0090)
- `re/notes/capture-start-stop.md` -- Capture protocol (TASK-0044)
- `re/ghidra/decompiled-graduate.txt` -- Primary decompilation output
- `re/ghidra/decompiled-graduate2.txt` -- Supporting function decompilation
- `re/ghidra/scripts/DecompileGraduate.java` -- Ghidra decompilation script
- `re/ghidra/scripts/DecompileGraduate2.java` -- Supporting decompilation script
