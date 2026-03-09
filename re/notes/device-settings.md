# Device Settings and Capabilities Analysis

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Asar: resources/app.asar (extracted to re/extracted-asar/)
Task: TASK-0044


## Summary

Per-device capabilities (channel counts, max sample rates, ADC ranges) are
defined in two places:

1. **Native code** (`libgraph_server_shared.so`): The `GetDeviceSettings()`
   virtual method on each device class returns a `DeviceSettings` object
   containing channel capabilities, sample rate options, and digital
   threshold options. These are serialized to JSON and sent to the Electron
   frontend.

2. **JS bundle** (`bundle.js`): Contains default presets, ADC-to-voltage
   transforms, and UI constants that must match the native values. The JS
   side does NOT define the capabilities -- it receives them from native.


## Vtable Slot 13: GetDeviceSettings()

Confirmed via relocation analysis that vtable slot 13 (0-indexed from
first virtual method after typeinfo pointer) is `GetDeviceSettings()` for
the Logic2FpgaDevice hierarchy.

| Device Class                | Vtable Slot 13 Address | Function Address |
|-----------------------------|------------------------|------------------|
| LogicGraduateDevice         | 0x2e48700              | 0x1c3f920        |
| LogicProfessionalDevice     | 0x2e48a28              | 0x1c44560        |
| LogicProfessional8Device    | 0x2e48b58              | 0x1c449d0        |
| LogicProfessional16Device   | 0x2e48ac0              | 0x1c44750        |
| LogicStudentDevice          | 0x2e49288              | 0x1c57980        |

Note: The function at vtable slot 0 (0x1c44560 for LogicProfessionalDevice)
was initially misidentified as GetDeviceSettings but is actually
`GetPublicKey()` -- confirmed by the string reference
`"virtual std::vector<U8> LogicProfessionalDevice::GetPublicKey()"`.

The actual GetDeviceSettings functions (0x1c3f920, 0x1c449d0, 0x1c44750,
0x1c57980) use XOR obfuscation to construct a 65-byte blob. They allocate
0x41 bytes, fill 8 qwords + 1 byte from XOR-encrypted global data.
The XOR key is a 64-bit value stored adjacent to the encrypted data in
the `.bss`/`.data` segment. This obfuscation makes static extraction of
the settings blob impractical without runtime instrumentation.


## Device Constructor Constants (Confirmed)

The device constructors pass explicit numeric constants for max sample
rates and channel counts. These were extracted by tracing callers of the
`Logic2FpgaDevice` base constructor at VA 0x1c20460 and its variant at
VA 0x1c201f0.

### Constructor at 0x1c201f0 (LogicGraduateDevice path)

Called from LogicGraduateDevice constructor at 0x1c3d1a0.

Arguments:
- `ecx` = 0x0BEBC200 = 200,000,000 (200 MHz digital)
- `r8d` = 0x01312D00 = 20,000,000 (20 MHz analog)
- `r9d` = 0x08 = 8 (channel count)

Stored at struct offsets:
- offset 0x194: digital sample rate (u32)
- offset 0x198: analog sample rate (u32)
- offset 0x190: forwarded parameter (channel count / performance option count)

### Constructor at 0x1c20460 (LogicProfessionalDevice path)

Called from three locations:

**LogicGraduateDevice (Logic 8)** at 0x1c3d73f:
- `r8d` = 0x0BEBC200 = 200,000,000 (200 MHz digital)
- `r9d` = 0x01312D00 = 20,000,000 (20 MHz analog)
- stack = 0x08 = 8 (channel count)

**LogicProfessional16Device (Logic Pro 16)** at 0x1c446e1:
- `r8d` = 0x1DCD6500 = 500,000,000 (500 MHz digital)
- `r9d` = 0x02FAF080 = 50,000,000 (50 MHz analog)
- stack = 0x10 = 16 (channel count)

**LogicProfessional8Device (Logic Pro 8)** at 0x1c44961:
- `r8d` = 0x1DCD6500 = 500,000,000 (500 MHz digital)
- `r9d` = 0x02FAF080 = 50,000,000 (50 MHz analog)
- stack = 0x08 = 8 (channel count)

**LogicProfessionalDevice base** at 0x1c41545 (forwarding wrapper):
- `r8d` = 0x1DCD6500 = 500,000,000 (500 MHz digital)
- `r9d` = 0x02FAF080 = 50,000,000 (50 MHz analog)
- stack = forwarded from caller's `r8d`


### LogicStudentDevice (Logic 4) Sample Rate Options

The LogicStudentDevice does NOT use the same constructor path. Instead,
its performance options function at VA 0x1c56310 constructs sample rate
options inline by calling a builder function (0x1c47bd0) multiple times.

Constants extracted from the builder calls:

| Call Site | esi (digital Hz) | ecx (analog Hz) | Stack arg 1 | Stack arg 2 |
|-----------|------------------|------------------|-------------|-------------|
| 0x1c563e7 | 12,000,000       | 6,000,000        | 6,000,000   | 0           |
| 0x1c56445 | 12,000,000       | 6,000,000        | 125,000     | 1           |
| 0x1c564a3 | 12,000,000       | 6,000,000        | 5,000       | 1           |

Pattern: 12 MHz digital, 6 MHz analog, with varying analog downsample
parameters (6 MHz raw, 125 kHz downsampled, 5 kHz downsampled).

The `mov $0x4,%esi` at 0x1c5633f confirms 4 channels for LogicStudentDevice.


## JS Bundle Constants (from re/extracted-asar/dist/logic/bundle.js)

### Device Type Enumeration

```javascript
legacyDeviceTypes = ["Logic", "Logic16", "Logic4", "Logic8", "LogicPro8", "LogicPro16"]
// "Mso" is a separate device type
```

### Calibration IDs

```javascript
{Logic: 0, Logic16: 1, Logic4: 4, Logic8: 5, LogicPro8: 6, LogicPro16: 7}
```

### ADC Ranges (Default, before calibration)

```javascript
d = {Logic4: {min: 0, max: 255}, Default: {min: -2048, max: 2047}}
```

- Logic 4: 8-bit unsigned (HMCAD1100 ADC), range 0-255
- All others: 12-bit signed (AD9637 or ADS528x ADC), range -2048 to 2047

### ADC-to-Voltage Default Transforms

```javascript
// compute1DTransform(inputMin, inputRange, outputMin, outputRange)
// returns {m: outputRange/inputRange, b: outputMin - inputMin * m}

Logic4:    compute1DTransform(0, 255, 0, 5)       // 0V-5V over ADC 0-255
Logic8:    compute1DTransform(-2048, 4095, 0, 5)   // 0V-5V over ADC -2048..2047
LogicPro8: compute1DTransform(-2048, 4095, -10, 20) // -10V to +10V over ADC -2048..2047
LogicPro16: same as LogicPro8
Logic:     {m: 1, b: 0}  // identity (no analog)
Logic16:   {m: 1, b: 0}  // identity (no analog)
```

### Default Sample Rate Presets

For limited bandwidth devices (USB 2.0 fallback):
```javascript
f = {
  Logic:      {digital: 16000000},                     // 16 MHz digital only
  Logic4:     {digital: 12000000, analog: 6000000},    // 12 MHz / 6 MHz
  Logic8:     {digital: 100000000, analog: 2500},      // 100 MHz / 2.5 kHz
  Logic16:    {digital: 100000000},                     // 100 MHz digital only
  LogicPro8:  {digital: 100000000, analog: 3125},       // 100 MHz / 3.125 kHz
  LogicPro16: {digital: 100000000, analog: 3125},       // 100 MHz / 3.125 kHz
}
```

For full bandwidth devices (USB 3.0):
```javascript
m = {
  LogicPro8:  {digital: 500000000, analog: 50000000},  // 500 MHz / 50 MHz
  LogicPro16: {digital: 500000000, analog: 50000000},  // 500 MHz / 50 MHz
}
```

### Analog Downsampled Rates

```javascript
analogDownsampledRates = [31250, 15625, 6250, 1250, 250, 50]  // Hz
devicesSupportingDownsampling = ["Logic4", "Logic8", "LogicPro8", "LogicPro16"]
```

### Channel Capability Schema

Each channel has one of three capability types:
```
AlwaysEnabled    -- channel cannot be disabled by user
HardwareDisabled -- channel is not available on this hardware
Toggleable       -- user can enable/disable
```

The `channelCapabilities` array returned from native code describes
every channel (both Analog and Digital) with its type and capability.

### Capabilities Object (from native, consumed by JS)

```javascript
capabilities = {
  channelCapabilities: [{type: "Analog"|"Digital", index: number, capability: "..."}],
  sampleRateOptions: [{digital: number, analog: number}],
  digitalThresholdOptions: [{description: string}],  // optional
  isPhysicalDevice: boolean
}
```

### Default Enabled Channels

```javascript
NUM_ENABLED_CHANNELS_PER_TYPE = 2
// On connection, first 2 channels of each type are enabled
```

### Devices Requiring Calibration

```javascript
devicesRequiringCalibration = ["Logic4", "Logic8", "LogicPro8", "LogicPro16"]
```

### Voltage Range Defaults (from getDefaultAnalogRanges)

```javascript
// Used when no calibration data is available
LogicPro8:  {min: -15, max: 15}   // -15V to +15V
Other:      {min: -2.5, max: 7.5} // -2.5V to +7.5V
```


## Device Settings JSON Serialization Keys

Found in binary `.rodata`:

```
/channelCapabilities
/sampleRateOptions
/digitalThresholdOptions
/isPhysicalDevice
/enabledChannels
/bitRate
/analog
/digital
/type
/index
/capability
/description
/thresholdVoltage
/sampleRate
```


## Source File Path

```
/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceSettings.cpp
```


## Open Questions

1. **Exact sample rate option vectors**: The binary constructs these via
   XOR-obfuscated blobs or inline builder calls. Full extraction requires
   either Ghidra decompilation or runtime instrumentation. The constructor
   passes max rates, but the actual option vectors (which rate combinations
   are valid for which channel configurations) are computed internally.

2. **Digital threshold options**: Pro devices support configurable voltage
   thresholds (1.2V, 1.8V, and possibly others). The exact set per device
   is in the native GetDeviceSettings return value.

3. **USB 2.0 vs 3.0 rate limiting**: The `hasLimitedBandwidth` flag in JS
   is set when the device connects at USB 2.0 speed. This limits available
   sample rate options. The exact limited options need runtime capture.

4. **Logic 4 analog channels**: Logic 4 has 4 digital channels but the
   number of analog channels is unclear. The HMCAD1100 8-bit ADC can
   operate in various channel modes. The `mov $0x4,%esi` suggests 4 total
   channels. Given that bundle.js shows Logic4 with `analog:6e6`, it does
   have analog capability.

5. **Performance tiers**: Three tiers exist (DigitalOnly, AnalogBasic,
   AnalogAdvanced) with different rate combinations. The AnalogAdvanced
   tier returns `vector<vector<SampleRateOption>>` (nested), suggesting
   per-channel rate configuration.
