# SampleRateOption Struct and Performance Tiers

Status: Confirmed (Ghidra decompilation + global data extraction)
Source: TASK-0090
Binary: libgraph_server_shared.so v2.4.40
Source path: `/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceSettings.cpp`


## SampleRateOption Struct Layout

Size: 0x18 bytes (24 bytes, accessed as 6 x int32 in Set* comparisons)

```
Offset  Size  Type    Field
------  ----  ------  -----
0x00    4     U32     digitalSampleRateHz
0x04    1     bool    hasDigital        (1 = digital rate is valid)
0x05-7  3     pad
0x08    4     U32     analogOutputSampleRateHz  (downsampled output rate)
0x0C    1     bool    hasAnalog         (1 = analog rate is valid)
0x0D    1     bool    isSet             (1 = option has been initialized)
0x0E    1     bool    hasDownsample     (1 = analog physical != output rate)
0x0F    1     pad
0x10    4     U32     analogPhysicalSampleRateHz (actual ADC sample rate)
0x14    4     U32     downsampleGroupIndex  (selects from AnalogAdvanced groups)
```

Evidence:
- `piVar5 = piVar5 + 6` (stride 24 bytes, 6 x u32)
- Comparisons in Set*: `piVar5[0]==param[0]`, `piVar5[1]==param[1]`, `piVar5[2]==param[2]`, `piVar5[3]==param[3]`, `piVar5[4]==param[4]`, `piVar5[5]==param[5]`
- `*(char *)((long)param_1 + 0xd)` tested for "attempt to read un-set SampleRateOption" (offset 0x0D = isSet flag)
- `(char)piVar11[1]` tested for hasDigital (offset 0x04)
- `(char)piVar11[3]` tested for hasAnalog (offset 0x0C)
- `piVar11[2]` compared for analog rate match (offset 0x08 = analogOutputSampleRateHz)
- `piVar11[4]` = analogPhysicalSampleRateHz (offset 0x10)
- `piVar11[5]` = downsampleGroupIndex (offset 0x14)
- Assertion string: `"U32 SampleRateOption::DigitalSampleRateHz() const"` checks isSet then hasDigital
- Assertion string: `"U32 SampleRateOption::AnalogPhysicalSampleRateHz() const"` checks isSet then hasAnalog then `*(char *)(offset + 0xc)` (the hasAnalog flag indicates the analog field is meaningful)


## Performance Tier System

Three performance tiers exist, selected based on the device type and current channel configuration:

| Tier              | Return Type                           | Who Uses It           |
|-------------------|---------------------------------------|-----------------------|
| Digital-Only      | `vector<SampleRateOption>`            | Legacy devices (0,1), devices with no analog channels enabled |
| Analog Basic      | `vector<SampleRateOption>`            | Pro devices (5,6,7,8) when analog is enabled |
| Analog Advanced   | `vector<vector<SampleRateOption>>`    | Pro devices (5,6,7,8) when analog is enabled |

The tier is chosen by `GetPerformanceInterfaceType()` which uses a lookup table indexed by the device type enum value (field at DeviceSettings+0x00).


## GetPerformanceInterfaceType

ELF VA: `0x1c49e40` (Ghidra: `0x1d49e40`)

Returns a PerformanceInterfaceType enum based on `DeviceSettings[0]` (device type):

```c
uint GetPerformanceInterfaceType(DeviceSettings *this) {
    uint device_type = this->deviceType;
    if (device_type < 9 && ((0x1fb >> device_type) & 1)) {
        return LOOKUP_TABLE[device_type];  // table at ELF 0x5748cc
    }
    ASSERT_FAIL("Invalid device");
}
```

The lookup table at 0x5748cc is in obfuscated .rodata and could not be extracted statically (the Ghidra values look like XOR-obfuscated or relocated data). However, from the code flow in the large `FUN_01d48ee0` function:

| Device Type Value | Bitmask Test | Performance Interface |
|-------------------|--------------|----------------------|
| 0 (Logic)         | `(3 >> 0) & 1 = 1` | Digital-Only (legacy) |
| 1 (Logic16)       | `(3 >> 1) & 1 = 1` | Digital-Only (legacy) |
| 3 (Logic 4)       | `0x118 >> 3 = 0x23, & 1 = 1` | Analog Basic |
| 4 (Logic 8)       | `0x118 >> 4 = 0x11, & 1 = 1` | Analog Basic |
| 5 (LogicPro8)     | `0xe0 >> 5 = 0x7, & 1 = 1` | Analog Advanced |
| 6 (LogicPro16)    | `0xe0 >> 6 = 0x3, & 1 = 1` | Analog Advanced |
| 7 (LogicPro16)    | `0xe0 >> 7 = 0x1, & 1 = 1` | Analog Advanced |
| 8 (special?)      | explicit check | Analog Basic |


## Digital-Only Rate Array (Pro Devices)

From `.rodata` at ELF VA `0x471a28` (confirmed at three locations: 0x471a28, 0x472220, 0x4742b0):

```
500 MHz, 250 MHz, 125 MHz, 100 MHz, 50 MHz, 25 MHz, 20 MHz,
12.5 MHz, 10 MHz, 6.25 MHz, 5 MHz, 4 MHz, 2.5 MHz, 2 MHz, 1 MHz
```

15 entries. These are the available digital sample rates for Logic Pro 8/16.


## Analog Rate Array (Pro Devices)

From `.rodata` at ELF VA `0x471a64` (also at 0x473ce0):

```
100 MHz, 50 MHz, 40 MHz, 25 MHz, 20 MHz, 10 MHz, 8 MHz,
5 MHz, 4 MHz, 2 MHz, 1 MHz
```

11 entries. These represent the analog ADC physical sample rates (before decimation).

The second copy at `0x472260` starts at 50 MHz (omitting 100 MHz):
```
50 MHz, 40 MHz, 25 MHz, 20 MHz, 10 MHz, 8 MHz, 5 MHz, 4 MHz, 2 MHz, 1 MHz
```

10 entries. This may be the USB 2.0 version (100 MHz analog rate requires USB 3.0 bandwidth).


## GetDigitalOnlyPerformanceOptions

ELF VA: `0x1c49ec0` (Ghidra: `0x1d49ec0`)

```
void GetDigitalOnlyPerformanceOptions(DeviceSettings *this, vector<SampleRateOption> &out)
```

Only valid for device types 0 and 1 (Logic, Logic16). Asserts on Pro devices.

Algorithm:
1. Count enabled digital channels (`numDigitalChannels`)
2. Get USB max bit rate from `GetInterfaceMaximumBitRate()`
   - Indexed by `this->usbSpeed` (offset 0x28 in DeviceSettings, u32)
   - Values from table at ELF 0x5748f0 (3 entries for USB speeds)
3. Iterate digital rate array (offset 0x36-0x38 in DeviceSettings = pointer pair)
4. For each rate: if `rate * numDigitalChannels <= maxBitRate`, add a SampleRateOption with:
   - digitalSampleRateHz = rate
   - hasDigital = true, hasAnalog = false
   - isSet = true


## GetAnalogBasicPerformanceOptions

ELF VA: `0x1c4a140` (Ghidra: `0x1d4a140`)

```
void GetAnalogBasicPerformanceOptions(DeviceSettings *this, vector<SampleRateOption> &out)
```

Valid for device types 3, 4, 8 (Logic 4, Logic 8, special). Stored at
DeviceSettings offset 0x3c (pointer to stored options vector).

Returns the pre-computed options stored in the DeviceSettings object at offset 0xF0 (0x3c * 4).
The options are populated during GetDeviceSettings construction.


## GetAnalogAdvancedPerformanceOptions

ELF VA: `0x1c4a250` (Ghidra: `0x1d4a250`)

```
void GetAnalogAdvancedPerformanceOptions(DeviceSettings *this, vector<vector<SampleRateOption>> &out)
```

Valid for device types 5, 6, 7 (Pro 8, Pro 16). This is the most complex function.

Algorithm:
1. Get USB max bit rate from USB speed table
2. Build a bandwidth tier table with 5 tiers:
   ```
   tier[0] = maxBitRate          (100%)
   tier[1] = maxBitRate * 8 / 10 (80%)
   tier[2] = maxBitRate * 6 / 10 (60%)
   tier[3] = maxBitRate * 4 / 10 (40%)
   tier[4] = maxBitRate / 5      (20%)
   ```
3. Count enabled digital channels (`numDigital`) and analog channels (`numAnalog`)
4. Three sub-cases:
   - **numAnalog == 0**: Only digital channels enabled
     - For each digital rate, add digital-only option if `rate * numDigital <= maxBitRate`
   - **numDigital == 0**: Only analog channels enabled
     - For each analog rate pair (physical, output): if `this->bitsPerSample * rate * numAnalog <= maxBitRate`, add analog-only option
   - **Both digital and analog**: Full bandwidth allocation
     - 5 groups (one per tier). For each tier:
       - Try each analog rate pair and each digital rate
       - Call `FUN_01d19fa0()` to validate packet configuration
       - Check: `bitsPerSample * analogRate * numAnalog + digitalRate * numDigital <= tier[group]`
       - Check: packet sizes fit in uint8 (< 256)
       - First valid combination per group is used

The `bitsPerSample` field is at DeviceSettings offset 0x1a (uint32, = `this[0x1a]` or byte offset 0x68).

For Pro 8 and Pro 16, the first arg to `FUN_01d19fa0` includes the expression
`((*this & 0xfffffffe) == 6) * 0x10 + 0x10` which evaluates to:
- Pro 8 (type 6): 0x10 + 0x10 = 32
- Pro 16 (type 7): 0x00 + 0x10 = 16

This is the number of interleave slots (32 for Pro 8, 16 for Pro 16), matching the
FPGA data interleaving format.


## SetDigitalOnlyPerformanceOption

ELF VA: `0x1c4af40` (Ghidra: `0x1d4af40`)

Validates the provided SampleRateOption is in the GetDigitalOnlyPerformanceOptions list,
then stores it at DeviceSettings offset 0x29-0x2d (the "current" SampleRateOption):
```
this[0x29..0x2d] = option  (24 bytes = SampleRateOption)
```

The current sample rate option is stored at DeviceSettings byte offset 0xA4:
- offset 0xA4 (0x29 * 4): digitalSampleRateHz (from Save function: `*(param_2 + 0xa4)`)
- offset 0xA8: hasDigital
- offset 0xAC: analogOutputSampleRateHz
- offset 0xB0: hasAnalog
- offset 0xB1: isSet


## SetAnalogBasicPerformanceOption

ELF VA: `0x1c4ac00` (Ghidra: `0x1d4ac00`)

Same validation pattern as SetDigitalOnly but validates against
GetAnalogBasicPerformanceOptions list. Also: if the chosen option has `hasAnalog == false`,
disables all toggleable analog channels.


## SetAnalogAdvancedPerformanceOption

ELF VA: `0x1c4b130` (Ghidra: `0x1d4b130`)

Validates against all SampleRateOption entries across all groups from
GetAnalogAdvancedPerformanceOptions. Iterates `vector<vector<SampleRateOption>>`.


## LogicStudentDevice (Logic 4) Sample Rate Options

ELF VA: `0x1c56310` (Ghidra: `0x1d56310`)
Source: `LogicStudentDevice.cpp`

The builder function `FUN_01d47bd0` is called with these parameters:
```
FUN_01d47bd0(output, digitalRate, hasDigital, analogRate, hasAnalog,
             hasDownsample, isDownsampled, downsampledRate)
```

Channel setup: 4 channels (1 digital-capable + 3 analog-only + digital-only variants)

| # | Digital (Hz) | Analog Physical (Hz) | Analog Output (Hz) | Type |
|---|-------------|---------------------|--------------------|----- |
| 1 | 12,000,000  | 6,000,000           | 6,000,000          | Analog basic (full rate) |
| 2 | 12,000,000  | 6,000,000           | 125,000            | Analog downsampled |
| 3 | 12,000,000  | 6,000,000           | 5,000              | Analog downsampled |
| 4 | 12,000,000  | 6,000,000           | 1,000              | Analog downsampled |
| 5 | 12,000,000  | 6,000,000           | 100                | Analog downsampled |
| 6 | 12,000,000  | 6,000,000           | 10                 | Analog downsampled |
| 7 | 12,000,000  | --                  | --                 | Digital only (12 MHz) |
| 8 | 6,000,000   | --                  | --                 | Digital only (6 MHz) |
| 9 | 3,000,000   | --                  | --                 | Digital only (3 MHz) |
|10 | 1,000,000   | --                  | --                 | Digital only (1 MHz) |

The analog physical rate is always 6 MHz. The output rate is the decimated rate
delivered to the user. The ADC always samples at 6 MHz; decimation is host-side.

Channel config for Logic 4:
- `FUN_01d4c000(0, 12000000, local_168, 2, 0, 1)` - channel 0: digital type 0, toggleable(1)
- `FUN_01d4c000(0, 0, local_168, 0, 0, 2)` - channels 1-3: analog type 0, hardware-disabled digital(2)


## LogicGraduateDevice (Logic 8) Sample Rate Options

ELF VA: `0x1c3e0a0` (Ghidra: `0x1d3e0a0`)
Source: `LogicGraduateDevice.cpp`

The function:
1. Initializes DeviceSettings with 5 performance levels, max digital 200 MHz, max analog 20 MHz, 8 channels
2. Sets up 8 channel configs (all toggleable, both digital and analog)
3. Gets USB speed from the device
4. Copies the digital rate array from `DAT_0307aa30/38` (runtime-populated pointer pair, u32 elements)
5. Copies the analog rate pair array from `DAT_0307aa48/50` (runtime-populated, u64 elements = physical_rate + output_rate)
6. Validates: `DAT_0044e3b8 / rate` is an integer (divisibility check for digital rates)
7. Validates: `DAT_0044e278 / rate` is an integer (divisibility check for analog rates)
8. Calls `FUN_01d4c2a0(settings, &analog_pairs, &digital_rates, 0xc)` to build options

The 0xc parameter is the analog bits-per-sample (12 for Logic 8's 12-bit ADC).

The rate arrays are populated at runtime into `.bss` globals and could not be
extracted statically. From the JS bundle and constructor analysis:
- Max digital: 200 MHz
- Max analog: 20 MHz
- Expected digital rates: 200M, 100M, 50M, 25M, 20M, 10M, 5M, 2.5M, 1.25M (factors of 200M)
- Expected analog rates: 20M, 10M, 5M (factors of 20M)


## LogicProfessionalDevice (Logic Pro 8/16) Sample Rate Options

ELF VA: `0x1c418c0` (Ghidra: `0x1d418c0`)
Source: `LogicProfessionalDevice.cpp`

Similar to LogicGraduateDevice but:
- Max digital: 500 MHz, max analog: 50 MHz
- Channel count: 8 (Pro 8) or 16 (Pro 16) passed from sub-class constructor
- Has digital threshold strings (DAT_0307aa90/98)
- Threshold option is set based on USB speed and device ID
- Channel config depends on USB speed and a flag at `param_2[0x51]`
  - When USB 3.0 and flag clear: channels 0-7 enabled, channels 8-15 digital-only (for Pro 16)
  - Otherwise: all channels both digital and analog

The digital rate array is confirmed from .rodata:
```
500M, 250M, 125M, 100M, 50M, 25M, 20M, 12.5M, 10M, 6.25M, 5M, 4M, 2.5M, 2M, 1M
```

The analog rate pair array contains entries with (physical_rate, output_rate).
From the second array copy (starts at 0x472260, likely USB 2.0 set):
```
50M, 40M, 25M, 20M, 10M, 8M, 5M, 4M, 2M, 1M
```
From the first array copy (starts at 0x471a64):
```
100M, 50M, 40M, 25M, 20M, 10M, 8M, 5M, 4M, 2M, 1M
```
Note: 100 MHz analog is only in the first set (USB 3.0 only).

Divisibility validation constants:
- `DAT_0044d9d0 / digital_rate` must be integer
- `DAT_0044e2e8 / analog_rate` must be integer


## USB Speed and Bandwidth

The `GetInterfaceMaximumBitRate()` function at ELF VA `0x1c4a0c0` uses a table at
ELF VA `0x5748f0`, indexed by `DeviceSettings[0x28]` (USB speed enum, 0-2).

The USB speed enum values are:
- 0 = USB 1.x (or unknown)
- 1 = USB 2.0 High-Speed (480 Mbps)
- 2 = USB 3.0 SuperSpeed (5 Gbps)

The table values could not be read directly (they appear obfuscated in the binary).
However, the algorithm is clear: `rate * numChannels <= maxBitRate` gates which
sample rates are available.


## DeviceSettings Object Layout (Partial)

| Offset (bytes) | Size | Field |
|----------------|------|-------|
| 0x00           | 4    | deviceType (external enum value) |
| 0x04-0x4F      |      | (channel configuration data) |
| 0x50-0x5B      | 12   | channelConfigs vector (ptr, end, capacity) |
| 0x58           | 8    | channelConfigs end ptr |
| 0x68 (0x1a*4)  | 4    | bitsPerSample (12 for Pro/Logic8, 8 for Logic4) |
| 0xA0 (0x28*4)  | 4    | usbSpeed (0=unknown, 1=USB2, 2=USB3) |
| 0xA4 (0x29*4)  | 24   | currentSampleRateOption (SampleRateOption struct) |
| 0xBC (0x2f*4)  | 4    | (padding/reserved) |
| 0xC0 (0x30*4)  |      | (more settings) |
| 0xD8 (0x36*4)  | 8    | digitalRates vector ptr |
| 0xDC (0x37*4)  |      | (part of vector) |
| 0xE0 (0x38*4)  | 8    | digitalRates end ptr |
| 0xF0 (0x3c*4)  | 8    | analogBasicOptions vector ptr (for Basic devices) |
| 0xF8 (0x3e*4)  | 8    | analogBasicOptions end ptr |
| 0x108 (0x42*4) | 8    | analogDigitalRates vector ptr (for Advanced) |
| 0x110 (0x44*4) | 8    | analogDigitalRates size/end |
| 0x120 (0x48*4) | 8    | analogRatePairs vector ptr |
| 0x128 (0x4a*4) | 8    | analogRatePairs end ptr |
| 0x130 (0x4c*4) |      | (reserved) |
| 0x138 (0x4e*4) | 8    | thresholdStrings vector ptr |
| 0x140 (0x50*4) | 8    | thresholdStrings end ptr |
| 0x144 (0x51*4) | 1    | limitedBandwidthFlag |


## Analog Rate Pair Format

The analog rate pairs at offset 0x48 are stored as 8-byte entries:

```
Offset  Size  Field
0       4     analogPhysicalSampleRateHz
4       4     analogOutputSampleRateHz
```

When `physicalRate != outputRate`, the `hasDownsample` flag in the SampleRateOption
is set to true. This indicates that the ADC samples at the physical rate but the
data is decimated to the output rate on the host side.


## Bandwidth Allocation Algorithm (Analog Advanced)

The AnalogAdvanced tier creates 5 groups representing different bandwidth allocation
strategies between digital and analog channels:

```
Group 0: 100% bandwidth budget
Group 1:  80% bandwidth budget
Group 2:  60% bandwidth budget
Group 3:  40% bandwidth budget
Group 4:  20% bandwidth budget
```

For each group, the algorithm tries to find the highest sample rate combination
where: `bitsPerSample * analogRate * numAnalogChannels + digitalRate * numDigitalChannels <= budgetBitRate`

The packet size constraints are also checked via FUN_01d19fa0, which ensures the
resulting packet configuration fits in uint8 fields (< 256).

This design allows the user to trade off between digital and analog sample rates.
Group 0 maximizes the combined rate, while Groups 1-4 progressively reduce the
budget, allowing lower rates to be selected for reduced bandwidth/CPU usage.


## Cross-Reference with JS Bundle

The JS bundle's default sample rate presets align with these findings:

| Device     | JS Default Digital | JS Default Analog | Binary Max Digital | Binary Max Analog |
|------------|-------------------|-------------------|--------------------|--------------------|
| Logic 4    | 12 MHz            | 6 MHz             | 12 MHz             | 6 MHz              |
| Logic 8    | 100 MHz           | 2.5 kHz           | 200 MHz            | 20 MHz             |
| Logic Pro 8| 100 MHz (USB2) / 500 MHz (USB3) | 3.125 kHz (USB2) / 50 MHz (USB3) | 500 MHz | 50 MHz |
| Logic Pro 16| Same as Pro 8    | Same as Pro 8     | 500 MHz            | 50 MHz             |

The JS defaults are conservative (especially for Logic 8 and Pro devices at USB 2.0)
while the binary supports the full range up to the maximum rates.

The `analogDownsampledRates` from JS `[31250, 15625, 6250, 1250, 250, 50]` represent
additional host-side downsampling options beyond what the binary provides. These are
applied after the initial ADC capture, not part of the DeviceSettings rate selection.


## Open Questions

1. **Exact USB bit rate values**: The table at 0x5748f0 could not be read due to
   apparent obfuscation or relocation artifacts. Runtime extraction would confirm.

2. **Logic 8 rate arrays**: These are populated at runtime via `.bss` globals
   (DAT_0307aa30/38/48/50) and could not be extracted statically. The divisibility
   validation against DAT_0044e3b8 and DAT_0044e278 constrains them to factors of
   200 MHz and 20 MHz respectively.

3. **The 0x1e848 constant**: In LogicStudentDevice, the second rate option uses
   0x1e848 = 125,000 Hz as the analog output rate (125 kHz downsampled from 6 MHz).
   This differs from the JS bundle's downsampled rates [31250, 15625, 6250, 1250, 250, 50].
   The discrepancy suggests the 125 kHz rate is a hardware-level decimation option
   built into the Logic 4's HMCAD1100 ADC or FPGA, separate from the host-side
   downsampling in the JS bundle.

4. **PerformanceInterfaceType enum values**: The actual enum values in the lookup
   table could not be read (obfuscated). From code flow analysis:
   - 0 or 1 = DigitalOnly (for Logic, Logic16)
   - 2 or 3 = AnalogBasic (for Logic4, Logic8)
   - 4 or 5 = AnalogAdvanced (for Pro 8, Pro 16)

5. **Device type 8**: Referenced in several bitmask checks alongside device types
   3 and 4 (AnalogBasic). This may be an internal testing device or alternate
   hardware revision.
