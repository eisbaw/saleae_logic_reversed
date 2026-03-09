# AFE Voltage Range Configurations

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Asar: resources/app.asar (extracted to re/extracted-asar/)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0040


## Summary

The Saleae Logic Pro devices use a multi-stage analog front-end (AFE) with
digitally controlled attenuators and a variable gain amplifier. The voltage
range is NOT a fixed discrete set of predefined configs (like a traditional
oscilloscope's 1x/2x/5x sequence). Instead, the frontend sends the desired
min/max voltage range to the native library, which computes the optimal AFE
hardware settings to cover that range.

Two different AFE config types exist:

1. **`AfeConfig_VoltageScale`** -- Used by the Electron frontend to
   specify a desired voltage range as `{minVoltageScale, maxVoltageScale}`.
   The native code maps this to hardware settings.

2. **`AfeConfig_RawHardware`** -- The actual hardware register values:
   `{attn1, attn2, dac, preamp, ladder, filter, power}`. Used internally
   and for the numbered config system (`ForConfigNumber`/`GetConfigNumber`).

The mapping from `AfeConfig_VoltageScale` to `AfeConfig_RawHardware` is
performed by `MsoOscilloscopeInstrument::AnalogChannelConfigToHardwareConfig`.


## AFE Signal Chain (Logic Pro 8/16)

```
Input
  |
  v
Attenuator Stage 1 (attn1) -- digital control via IO expander
  |                            (100x or bypass, based on /attenuator1)
  v
Attenuator Stage 2 (attn2) -- digital control via IO expander
  |                            (10x or bypass, based on /attenuator2)
  v
LMH6518 VGA (Variable Gain Amplifier)
  |  - preamp: High Gain (HG) or Low Gain (LG) mode
  |  - ladder: 0 to -20 dB attenuation in 2 dB steps
  |  - gain/filter: auxiliary gain and bandwidth settings
  |  - power: power mode control
  v
DC Offset DAC
  |  - range: -2.5V to +2.5V (confirmed by assertion)
  |  - quantized by ClampAndQuantizeDacVoltage()
  v
ADC (AD9637 on Xilinx, ADS528x on Lattice)
  |  - 12-bit, range [-2048, 2047]
  v
FPGA -> USB -> Host
```

### IO Expander Layout

Four IO expanders are used, each controlling different AFE subsystems:

| IO Expander | Function | Controls |
|-------------|----------|----------|
| `BuildIoExpander_Attenuation` | Attenuator relays | attn1, attn2 per channel |
| `BuildIoExpander_DcVgaAdcDac` | DC/VGA/ADC/DAC | Coupling relays, VGA SPI, ADC power, DAC |
| `BuildIoExpander_SmartCableA` | Smart Cable A | Port A (MSO only) |
| `BuildIoExpander_SmartCableB` | Smart Cable B | Port B (MSO only) |

Log message: `"Configuring IO Expander for Dc/Vga/Adc/Dac"`


## AfeConfig_VoltageScale (Frontend -> Native)

The Electron frontend sends the desired voltage range to the native code as
part of the capture configuration. Each analog channel has:

```json
{
  "analogSettings": {
    "enabled": true,
    "coupling": "DcCoupling",
    "probeAttenuation": 10,
    "afeConfig": {
      "type": "AfeConfig_VoltageScale",
      "minVoltageScale": -5.0,
      "maxVoltageScale": 5.0
    }
  }
}
```

### How the Voltage Range is Determined

The frontend uses one of two approaches:

1. **Auto (default)**: `desiredVoltageRange` is set to the current viewable
   vertical range (`getViewableVerticalRange()`). As the user zooms/pans
   vertically, the AFE automatically adjusts to the visible range. This
   means the AFE gain changes during user interaction.

2. **Fixed**: When `fixVoltageRange` is `true`, `desiredVoltageRange` uses
   `{fixedMinVoltage, fixedMaxVoltage}` from the channel settings. Defaults
   to `{-1, +1}` V.

The native assertion `min_voltage_scale < max_voltage_scale` confirms the
native code validates the range.

### Displayable Voltage Range (UI limits)

The maximum voltage range the UI allows depends on device and probe:

| Device | Probe | Min Voltage | Max Voltage | Total Range |
|--------|-------|-------------|-------------|-------------|
| Logic Pro 8 | 1x | -15 V | +15 V | 30 V |
| Logic Pro 16 | 1x | -15 V | +15 V | 30 V |
| Logic 4 | any | -2.5 V | +7.5 V | 10 V |
| Logic 8 | any | -2.5 V | +7.5 V | 10 V |
| MSO | 1x | -75 V | +75 V | 150 V |
| MSO | 10x | -750 V | +750 V | 1500 V |

Source: `displayableVoltageRange` getter in bundle.js

### Fixed Voltage Range UI Limits

```javascript
DeviceInputRangeLimits = { min: -100, max: 100 }
```

When the user enables "Fixed Voltage Range", they can enter any min/max
between -100V and +100V. The native code then selects the nearest achievable
AFE configuration.


## AfeConfig_RawHardware (Native Internal)

The actual hardware configuration sent to the AFE consists of these fields:

```
AfeConfig_RawHardware {
    attn1:   bool/int    -- Attenuator 1 (100x or bypass)
    attn2:   bool/int    -- Attenuator 2 (10x or bypass)
    dac:     double      -- DAC voltage (6 decimal places precision)
    preamp:  enum        -- LMH6518 preamp mode (HG/LG)
    ladder:  int         -- LMH6518 ladder attenuation (0 to -20 dB)
    filter:  enum        -- LMH6518 filter/bandwidth setting
    pwoer:   enum        -- LMH6518 power mode (typo: "pwoer" in binary)
}
```

Format string from binary:
```
AfeConfig_RawHardware(attn1={}, attn2={}, dac={:.6f}, preamp={}, ladder={}, filter={}, pwoer={})
```

### Resulting Voltage Range

Each `AfeConfig_RawHardware` configuration maps to a voltage range:

```
AfeConfig_VoltageScale(min={}, max={})
```

The per-channel expanded debug output shows all fields together:

```
    0: attn1={}, attn2={}, vga_gain={}, vga_preamp={}, vga_ladder={},
       dac={} V, min={} V, max={} V
    1: attn1={}, attn2={}, vga_gain={}, vga_preamp={}, vga_ladder={},
       dac={} V, min={} V, max={} V
    2: attn1={}, attn2={}, vga_gain={}, vga_preamp={}, vga_ladder={},
       dac={} V, min={} V, max={} V
    3: attn1={}, attn2={}, vga_gain={}, vga_preamp={}, vga_ladder={},
       dac={} V, min={} V, max={} V
```


## LMH6518 Variable Gain Amplifier

The LMH6518 is a Texas Instruments (formerly National Semiconductor)
900 MHz digitally controlled VGA. It provides the fine-grained gain
control in the AFE signal chain.

Reference: TI datasheet SNOS560B (https://www.ti.com/product/LMH6518)

### SPI Control

```cpp
Saleae::Vga_LMH6518 Saleae::MsoDeviceInterface::BuildVga(uint8_t)
uint16_t Saleae::EncodeVgaConfig(const Saleae::VgaConfig &)
```

Source file: `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/component/vga_lmh6518.cpp`

Log message: `"Updating VGA ch {} gain={} attn={}"`

### VGA JSON Field Paths

| Path | Description |
|------|-------------|
| `/vgaConfig` | Complete VGA configuration |
| `/vgaLinearGain` | Linear gain value |
| `/vgaPreampHigh` | Preamp high-gain mode flag |
| `/vgaLadderAttenuation` | Ladder attenuator setting |
| `/preampMode` | Preamp mode selection |
| `/gain` | Gain value |
| `/powerMode` | Power mode |

### Preamp Modes

The LMH6518 has two preamplifier gain settings:

| Mode | JSON Field | Typical Gain |
|------|-----------|-------------|
| High Gain (HG) | `vgaPreampHigh: true` | +20 dB |
| Low Gain (LG) | `vgaPreampHigh: false` | 0 dB |

### Ladder Attenuation

The LMH6518 ladder attenuator provides 0 to -20 dB attenuation in 2 dB steps.

Binary assertions confirm the valid range:
```
config.mLadderAttenuation <= 0
config.mLadderAttenuation >= -20
config.mLadderAttenuation % 2 == 0
```

Error messages:
- `"Ladder attenuation cannot be above 0"`
- `"Ladder attenuation cannot be below -20"`
- `"Ladder attenuation must be a multiple of 2"`

Valid values: 0, -2, -4, -6, -8, -10, -12, -14, -16, -18, -20 (11 values)

### Total VGA Gain Range

With preamp (0/+20 dB) and ladder (0 to -20 dB), the LMH6518 provides:
- **Minimum gain**: Low Gain + 20 dB attenuation = 0 - 20 = -20 dB (hypothesis)
- **Maximum gain**: High Gain + 0 dB attenuation = +20 + 0 = +20 dB (hypothesis)
- **Step size**: 2 dB
- **Total range**: ~40 dB

The exact gain values depend on the baseline gain of each mode, which
needs Ghidra decompilation or datasheet confirmation to determine precisely.
The TI datasheet specifies total gain from -1.16 dB to +38.8 dB.


## Attenuator Stages

### Attenuator 1 (100x)

JSON path: `/attenuator1`
Calibration transform: `/attn100Transform` (with `/scale` and `/offset`)

When enabled, attenuates the input by 100x (40 dB). This allows measuring
signals up to approximately +/-100V (when combined with ADC full-scale range).

### Attenuator 2 (10x)

JSON path: `/attenuator2`
Calibration transform: `/attn10Transform` (with `/scale` and `/offset`)

When enabled, attenuates the input by 10x (20 dB). Provides a second
attenuation stage between the input and the VGA.

### Voltage Limit After Attenuators

The binary contains an assertion:
```
std::abs( adjusted_after_attn_max ) <= VoltageLimitAfterAttenuators
std::abs( adjusted_after_attn_min ) <= VoltageLimitAfterAttenuators
```

This confirms that the signal level after the attenuator stages must not
exceed `VoltageLimitAfterAttenuators` (a constant, likely the LMH6518's
maximum input range). The exact value needs Ghidra extraction.


## Offset DAC

### Range

```
mMinDacVoltage == -2.5 && mMaxDacVoltage == 2.5
```

The offset DAC has a range of -2.5V to +2.5V. This allows shifting the
signal center to bring asymmetric signals into the ADC range.

### Functions

```cpp
double Saleae::MsoDeviceConstants::ClampAndQuantizeDacVoltage(double) const
```

JSON paths:
- `/dacVoltage` -- current DAC voltage
- `/minDacVoltage` -- minimum DAC voltage (-2.5)
- `/maxDacVoltage` -- maximum DAC voltage (+2.5)
- `/adcToDacVoltageScale` -- conversion scale between ADC codes and DAC voltage

### CalibrationIntegration

The transform from ADC codes to DAC voltage uses a separate scale:
```
cal.mAdcToDacVoltageScale != 0.0
```

Per-channel transform logging:
```
Ch {} transform, gain={}, offset={}
```


## Config Number System (ForConfigNumber/GetConfigNumber)

The binary contains a config number indexing system for AFE configurations:

```cpp
static Saleae::AfeConfig_RawHardware
    Saleae::AfeConfig_RawHardware::ForConfigNumber(int)

int Saleae::AfeConfig_RawHardware::GetConfigNumber() const
```

### Config Number Range

Assertion:
```
number >= 0 && number <= MsoConstants::TotalAfeConfigurations
```

`MsoConstants::TotalAfeConfigurations` is a compile-time constant. Its
exact numeric value has not been extracted from the binary (needs Ghidra
decompilation of `ForConfigNumber` to find the range check constant).

### Relationship to Calibration

The calibration metadata stores data indexed by AFE config number:

```
i % MsoAfeMetadata::AFE_CONFIG_COUNT == 0
i == MsoAfeMetadata::MAX_AFE_CHANNEL_COUNT * MsoAfeMetadata::AFE_CONFIG_COUNT
```

This means:
- Each channel has `AFE_CONFIG_COUNT` calibration entries
- Total entries = `MAX_AFE_CHANNEL_COUNT * AFE_CONFIG_COUNT`
- The EEPROM calibration data for MSO/newer Logic devices stores per-config
  calibration constants

Error: `"Scope calibration is missing constants for one or more AFE settings"`

### Hypothesized Config Count

Given the LMH6518 parameters:
- 2 preamp modes (HG/LG)
- 11 ladder values (0 to -20 dB in 2 dB steps)
- 3 attenuator combinations (none, 10x, 100x) -- or 4 if both independently

The total number of configs = 2 * 11 * 3 = 66 (or 2 * 11 * 4 = 88).

However, not all combinations may be valid/useful. The actual
`TotalAfeConfigurations` value needs binary extraction.


## Coupling Modes

```
AcCoupling
DcCoupling
```

JSON path: `/coupling`

Two coupling modes:
- **DC Coupling** (default): No DC blocking. Passes DC and AC components.
- **AC Coupling**: DC blocking capacitor in the signal path. Removes DC bias.

Schema definition confirms:
```json
"Coupling": {"enum": ["AcCoupling", "DcCoupling"], "type": "string"}
```

Coupling is controlled via the IO expander (`BuildIoExpander_DcVgaAdcDac`),
likely switching a relay that inserts/bypasses a DC blocking capacitor.


## Probe Attenuation

```
probeAttenuation: enum [1, 10]
```

Two probe settings:
- **1x**: No probe attenuation. Direct connection.
- **10x**: Standard 10x oscilloscope probe. The software compensates by
  multiplying the measured voltage by 10.

Default: 10x (from `getDefaultAnalogInputChannelSetting`)

UI text: "10x (default) increases the signal from the probe to account for
the attenuator in the probe. 1x applies no change to the signal"

**Important**: Probe attenuation is a software-only setting. It does NOT
change any AFE hardware. It only scales the displayed voltage by the probe
factor. The AFE config (attenuators, VGA, DAC) is computed from the
`desiredVoltageRange` which already accounts for probe attenuation.


## Per-Device AFE Differences

### Logic Pro 8 / Logic Pro 16

- Full AFE with attenuators, LMH6518 VGA, offset DAC
- AD9637 ADC (Xilinx revisions) or ADS528x (Lattice revisions)
- 12-bit ADC, range [-2048, 2047]
- Default voltage range: -10V to +10V (without calibration)
- Maximum displayable: +/-15V (1x probe), +/-150V (10x probe)
- AC and DC coupling supported
- 4 analog channels per oscilloscope instrument
  (`maxChannelsPerOscilloscopeInstrument = 4`)
- Logic Pro 8: 8 channels total (2 instruments)
- Logic Pro 16: 16 channels total (4 instruments)

### Logic 8

- Simpler AFE (no LMH6518 VGA, no attenuator stages)
- HMCAD1100 ADC (Xilinx revisions) or ADS528x (Lattice revisions)
- 8-bit ADC on HMCAD1100, normalized to 12-bit range
- Default voltage range: 0V to +5V (without calibration)
- Maximum displayable: -2.5V to +7.5V
- No coupling selection (DC only)

### Logic 4

- Simplest AFE (fixed gain path)
- HMCAD1100 ADC (Xilinx revisions)
- 8-bit ADC, range [0, 255] (unsigned, NOT normalized to 12-bit)
- Default voltage range: 0V to +5V
- Maximum displayable: -2.5V to +7.5V
- No coupling selection (DC only)


## AnalogChannelConfigToHardwareConfig

This is the key function that maps user-facing voltage range settings to
hardware register values:

```cpp
static Saleae::ChannelHardwareConfiguration
    Saleae::MsoOscilloscopeInstrument::AnalogChannelConfigToHardwareConfig(
        const Saleae::MsoDeviceConstants &,
        double,    // likely: voltage range span or max voltage
        double,    // likely: voltage offset or center voltage
        double,    // likely: another parameter (probe? scale?)
        Saleae::MsoChannelCalibration)
```

This function produces a `ChannelHardwareConfiguration` containing:
- Attenuator settings (attn1, attn2)
- VGA config (preamp, ladder, gain)
- DAC voltage
- Resulting min/max voltage range

The `MsoDeviceConstants` input provides the device-specific limits (DAC
range, ADC range, etc.). The `MsoChannelCalibration` input provides
per-channel calibration offsets that affect the hardware config selection.

The function is called from `PreprocessConfiguration`:
```cpp
std::tuple<OscilloscopeConfiguration, HardwareConfiguration>
    Saleae::MsoOscilloscopeInstrument::PreprocessConfiguration(
        Saleae::OscilloscopeConfiguration) const
```


## AFE Settling Time

After changing AFE settings:
```
AFE Settling Time: {} ns, {} samples
```

The AFE hardware (attenuator relays, VGA gain change) requires a settling
time before ADC data is valid. The settling time is computed in both
nanoseconds and sample counts, so the capture engine can discard the
appropriate number of initial samples.


## Auto-Scale

The auto-scale system uses `FullScaleKernel` and `VerticalScaleKernel` to
automatically find the optimal voltage range for a signal:

```cpp
Saleae::FullScaleKernel::FullScaleKernel(
    const Saleae::MsoDeviceConstants &,
    Saleae::OscilloscopeConfiguration,
    std::array<bool, MsoConstants::MaxScopeChannels>,
    int)

virtual Saleae::AutoScaleState
    Saleae::FullScaleKernel::HandleWaveform(
        Saleae::AutoScaleState,
        std::array<std::optional<Graph::RawWaveformData>,
                   MsoConstants::MaxScopeChannels>)
```

Auto-scale works in two phases:
1. **FullScaleKernel**: Starts with the widest voltage range, captures a
   waveform, measures the signal amplitude, returns `FullScaleResult`.
2. **VerticalScaleKernel**: Refines the voltage range to fit the signal
   closely, returns `VerticalScaleResult`.

Status messages: `"AutoScaleInProgress"`, `"AutoScaleDone"`
Error: `"Cannot start auto scale while it is already active"`


## Calibration per AFE Config

### Logic Devices (server-hosted calibration)

Logic devices download calibration from the server. The calibration file
contains `fullScaleVoltageRanges` per channel -- these represent the
actual measured full-scale voltage range for each channel:

```xml
<mFullScaleMinVoltage><_text>-10.0</_text></mFullScaleMinVoltage>
<mFullScaleMaxVoltage><_text>10.0</_text></mFullScaleMaxVoltage>
```

The IIR filter coefficients (5 biquad sections) vary by channel and
sample rate, but NOT by AFE config (voltage range). This means Logic
devices apply the same calibration filter regardless of the AFE setting.

### MSO / Newer Devices (EEPROM calibration)

MSO and newer Logic devices store calibration in EEPROM organized by
AFE config number:

```
/afeConfigCalibrations
MsoAfeMetadata::CalibrationConstants
```

Each AFE config has its own calibration constants stored per channel.
The total calibration array size is:
```
MAX_AFE_CHANNEL_COUNT * AFE_CONFIG_COUNT
```

This means newer devices have per-voltage-range calibration, which is
more accurate than the older Logic devices' single-range calibration.

Conversion functions:
```cpp
MsoAfeMetadata::CalibrationConstants
    Saleae::CalibrationToMetadataConstants(const Saleae::MsoCalibrationProfile &)

Saleae::MsoCalibrationProfile
    Saleae::MetadataConstantsToCalibration(const MsoAfeMetadata::CalibrationConstants &)
```


## Confidence Assessment

| Finding | Confidence | Evidence Sources |
|---------|-----------|-----------------|
| AfeConfig_VoltageScale {min, max} sent from frontend | **Confirmed** | bundle.js + binary `/minVoltageScale`, `/maxVoltageScale` |
| AfeConfig_RawHardware fields (attn1, attn2, dac, preamp, ladder, filter, pwoer) | **Confirmed** | Binary format string |
| LMH6518 VGA is the gain element | **Confirmed** | Binary class name, source path, TI part number |
| Ladder attenuation: 0 to -20 dB in 2 dB steps | **Confirmed** | Binary assertions (3 assertions with exact values) |
| DAC range: -2.5V to +2.5V | **Confirmed** | Binary assertion `mMinDacVoltage == -2.5 && mMaxDacVoltage == 2.5` |
| Attenuator stages: 100x (attn1) and 10x (attn2) | **Confirmed** | Binary `/attn100Transform`, `/attn10Transform` |
| Coupling: AC/DC | **Confirmed** | Binary + JS schema enum |
| Probe: 1x/10x | **Confirmed** | JS schema enum + UI strings |
| displayableVoltageRange per device | **Confirmed** | bundle.js getter |
| DeviceInputRangeLimits: {-100, +100} | **Confirmed** | bundle.js static constant |
| maxChannelsPerOscilloscopeInstrument: 4 | **Confirmed** | bundle.js static constant |
| Config number system (ForConfigNumber/GetConfigNumber) | **Confirmed** | Binary function signatures |
| TotalAfeConfigurations exact value | **Unknown** | Needs Ghidra extraction |
| AFE_CONFIG_COUNT exact value | **Unknown** | Needs Ghidra extraction |
| Attenuator bypass/enable interpretation | **High** | Inferred from transform names, needs hardware validation |
| VGA preamp gain values (20 dB hypothesis) | **Hypothesis** | LMH6518 datasheet typical, not confirmed from binary |
| AnalogChannelConfigToHardwareConfig parameter meanings | **Hypothesis** | Function signature + 3 double params of unknown meaning |


## Open Questions

1. **TotalAfeConfigurations value**: What is the exact integer value of
   `MsoConstants::TotalAfeConfigurations`? Ghidra decompilation of
   `ForConfigNumber` would reveal the range check immediate.

2. **AFE_CONFIG_COUNT and MAX_AFE_CHANNEL_COUNT values**: What are these
   constants? They define the calibration array dimensions.

3. **Attenuator relay implementation**: Are attn1 and attn2 simple bypass
   relays (on/off), or do they have multiple positions? The names suggest
   binary (on/off) but the IO expander could support more states.

4. **EncodeVgaConfig bit layout**: How does `EncodeVgaConfig(const VgaConfig &)`
   pack the preamp, ladder, filter, and power settings into a uint16_t?
   This directly reveals the LMH6518 SPI register format.

5. **VoltageLimitAfterAttenuators value**: What is the maximum voltage
   allowed after the attenuator stages? This determines the LMH6518's
   maximum input voltage (likely 2-3V peak-to-peak differential).

6. **Filter settings**: The `filter` field in AfeConfig_RawHardware
   controls the LMH6518's bandwidth filter. What filter options are
   available? The LMH6518 has selectable bandwidth limits.

7. **Power modes**: The `pwoer` (sic) field in AfeConfig_RawHardware.
   What power modes are available? The LMH6518 has full-power and low-power
   modes.

8. **Logic 8 and Logic 4 AFE**: These non-Pro devices have a simpler AFE
   without the LMH6518. What hardware is in their signal path?
   `ConfigureAfe` takes `std::array<AfeChannelConfiguration, 4>` which
   suggests it is only for the 4-channel-per-instrument MSO/Pro path.

9. **Calibration per AFE config**: The older Logic devices have a single
   set of IIR filter coefficients per (channel, sampleRate). Do they also
   need different transforms per AFE config, or is the linear
   ADC-to-voltage transform sufficient?

10. **DAC quantization**: `ClampAndQuantizeDacVoltage` quantizes the DAC
    voltage. What is the DAC resolution? This determines the minimum
    voltage offset step.


## New Leads (Backlog Candidates)

- The `FullScaleKernel` auto-scale algorithm would reveal the AFE config
  selection strategy. Decompiling it would show how the system picks
  attenuator and VGA settings for a given signal amplitude.

- The `ConvertToAnalogState` function converts between
  `HardwareConfiguration` and `OscilloscopeAnalogState`. This could reveal
  the mapping between AFE register values and state enums.

- The `MsoAfeBoardPartNumber` enum has values like `MsoAfe_2ch_9b`,
  `MsoAfe_2ch_12b`, `MsoAfe_4ch_9b`, `MsoAfe_4ch_12b`. The part numbers
  (913'00004, 913'00006, 913'00007, 913'00008) might refer to different AFE
  board revisions with different attenuator/VGA configurations.

- The `SetDacVoltage` function should be traceable to the actual I2C
  commands sent to set the DAC. The DAC part number would confirm the
  resolution and interface.

- The `GetAdcRegisterConfiguration(MsoDeviceConstants)` function returns
  a static table of ADC register values. Cross-referencing this with
  the ADC datasheet register maps would confirm ADC output mode settings.
