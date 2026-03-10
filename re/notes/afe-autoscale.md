# AFE Auto-Scale Logic (FullScaleKernel / VerticalScaleKernel)

Analysis date: 2026-03-10
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0072


## Summary

The Saleae auto-scale system automatically finds the optimal voltage range
for each analog channel. It works in two phases:

1. **FullScaleKernel** (phase 1, "horizontal"): Captures at the widest
   voltage range, measures signal amplitude using zero-crossing detection,
   and estimates the signal frequency. This phase determines whether a
   signal is present and roughly where it sits in the ADC range.

2. **VerticalScaleKernel** (phase 2): Narrows the voltage range to tightly
   fit the signal with margin. It measures the actual min/max ADC values,
   converts them to voltages, and computes an optimal range with 20% margin.

The system is managed by `AutoScaleRunner::OnWaveform`, which coordinates
the state machine and dispatches to the appropriate kernel.


## Architecture

### Class Hierarchy

```
AutoScaleKernel (abstract, RTTI: idN6Saleae15AutoScaleKernelE)
  |
  +-- FullScaleKernel (RTTI: N6Saleae15FullScaleKernelE)
  |     source: autoscale_horizontal_kernel.cpp
  |     Ghidra: HandleWaveform @ 0x2e5c600, ctor @ 0x2e5c1e0
  |
  +-- VerticalScaleKernel (RTTI: N6Saleae19VerticalScaleKernelE)
        source: autoscale_vertical_kernel.cpp
        Ghidra: HandleWaveform @ 0x2e5df90

AutoScaleRunner (source: autoscale_runner.cpp)
    Ghidra: OnWaveform @ 0x2e5bb20
```

### Key Types

- `AutoScaleState`: Enum-like value passed between HandleWaveform calls.
  Encodes the current phase of auto-scale and per-channel status.
- `FullScaleChannelResult`: Per-channel result from FullScaleKernel,
  serialized with fields `/error`, `/minVoltage`, `/maxVoltage`.
- `VerticalScaleChannelResult`: Per-channel result from VerticalScaleKernel,
  same serialization fields: `/error`, `/minVoltage`, `/maxVoltage`.
- `RawWaveformData`: Raw ADC samples from the FPGA, as `int16_t` arrays
  (12-bit, range [-2048, 2047]).
- `MsoConstants::MaxScopeChannels`: 4 (confirmed by std::array<T, 4> usage
  and loop bounds `if (3 < local_2dc) goto error`).

### Source File Mapping

| File | Class | Purpose |
|------|-------|---------|
| `autoscale_horizontal_kernel.cpp` | FullScaleKernel | Phase 1: full-range measurement |
| `autoscale_vertical_kernel.cpp` | VerticalScaleKernel | Phase 2: precision range fitting |
| `autoscale_runner.cpp` | AutoScaleRunner | Orchestrator / state machine |


## Phase 1: FullScaleKernel (autoscale_horizontal_kernel.cpp)

### Purpose

Captures waveform data at the widest available voltage range and determines:
1. Whether there is a signal present
2. The signal's approximate frequency (via zero-crossing counting)
3. Whether the signal fits within the current range

### Algorithm

The FullScaleKernel operates on raw ADC waveform data (`int16_t` arrays,
12-bit, range [-2048, 2047]).

#### Zero-Crossing Detection with Hysteresis

The kernel scans the ADC samples and counts zero crossings using a
three-state hysteresis detector:

```
State Machine:
  state = UNDECIDED (0)

  For each sample:
    if state == UNDECIDED:
      if sample > +49:   state = POSITIVE (2)
      elif sample < -50:  state = NEGATIVE (1)
    elif state == NEGATIVE:
      if sample >= 0:
        crossings++
        state = UNDECIDED
    elif state == POSITIVE:
      if sample < 0:   (actually: sVar5 < 1, i.e. <= 0)
        crossings++
        state = UNDECIDED
```

**Hysteresis thresholds**: +49 and -50 ADC codes (approximately 0.24V on
a +/-10V range with 12-bit ADC). This prevents noise from triggering
false crossings.

#### Decision Logic

After counting crossings across all waveform chunks:

1. **Too many crossings (>= 2600)**: Signal frequency is too high for
   auto-scale. Returns without changing state. The kernel does NOT
   adjust the range -- it reports "no change needed" (`result.has_change = 0`).

2. **Too few crossings (< 6)**: Signal is DC or very low frequency.
   Falls through to the voltage range adjustment path (see below).

3. **6 to 2599 crossings**: Valid signal detected. Estimates frequency:
   ```
   estimated_frequency = (crossings / 2) / (total_samples / sample_rate)
   ```
   Logs: `"Estimated frequency: {}"`. Then returns without range change
   (frequency info is stored for the horizontal kernel to use, but the
   vertical range is not adjusted in this phase).

#### Voltage Range Adjustment (DC / Slow Signal Path)

When crossings < 6 (DC or very slow signal), the kernel proceeds to
adjust the per-channel voltage range:

For the primary channel:
- Reads the current channel's min/max voltage from the state structure
- Computes a 20% margin: `margin = (max - min) * 0.2`
- Clamps margin to a minimum of 1.0V
- Sets new range: `[min - margin * 0.2, max + margin * 0.2]`
- Enables the channel's "has_data" flag if not already set

The result is packaged with:
```
result.has_change = 1
result.config = new OscilloscopeConfiguration with adjusted ranges
```

#### Iteration

The FullScaleKernel iterates up to 5 times (param_2 + 0x2e0 is the
waveform_count, checked against 5). Each iteration may refine the range.
The first call also dispatches to `VerticalScaleKernel::HandleWaveform`
(at FUN_02e5df90) before deciding the final state.


## Phase 2: VerticalScaleKernel (autoscale_vertical_kernel.cpp)

### Purpose

Given a signal that has been detected by FullScaleKernel, narrows the
voltage range to optimally fit the signal with appropriate margin.

### Algorithm

The VerticalScaleKernel processes each channel (up to 4) iteratively:

```
For channel ch = 0 to 3:
  if channel state == 0 (needs measurement):
    Assert waveform data is present for this channel

    Find min/max ADC values across all waveform chunks:
      min_adc = 0x7FFF (32767, initialized to max int16)
      max_adc = -0x8000 (-32768, initialized to min int16)
      For each chunk:
        Iterate through int16 samples
        Track global min and max

    Check for ADC saturation:
      If min_adc == *(param_2 + 0x70) OR max_adc == *(param_2 + 0x72):
        Channel is saturated (signal exceeds current range)
        Set channel state = 2 (saturated/error)
      Else:
        Convert min/max ADC codes to voltages using FUN_02dfe390()
        Store [min_voltage, max_voltage] in per-channel result

        Compute margin:
          signal_range = max_voltage - min_voltage
          range_margin = signal_range * 0.2
          full_scale_margin = (full_max - full_min) * 0.0078125  (1/128)
          margin = max(range_margin, full_scale_margin)

        Set channel "has_data" flag
        Compute new range: [min_voltage - margin, max_voltage + margin]

        Log: "Found: ch={}, {} to {}"
        Log: "Channel range: ch-{}, range={}, {:4f} to {:4f}"

  Advance to next channel
```

### Iteration and Convergence

The VerticalScaleKernel also iterates (up to 5 times, checked via
`*(param_2 + 8) < 5`). If after processing all 4 channels:

- At least one channel adjusted: Returns a new configuration with the
  adjusted ranges and `result.has_change = 1`
- No channels needed adjustment: Returns `result.has_change = 0`

### Key Constants

| Constant | Value | Meaning |
|----------|-------|---------|
| MaxScopeChannels | 4 | Maximum analog channels per instrument |
| Max iterations | 5 | Maximum auto-scale waveform captures |
| Margin factor | 0.2 (20%) | Signal range margin multiplier |
| Min margin factor | 0.0078125 (1/128) | Minimum margin as fraction of full scale |
| Crossing threshold low | +49 / -50 ADC codes | Hysteresis for zero-crossing detection |
| Max crossings | 2600 | Signal too fast for auto-scale |
| Min crossings | 6 | Signal too slow (DC) threshold |

### ADC Saturation Detection

The kernel stores the ADC range limits in the state structure at offsets
+0x70 and +0x72 (two int16_t values). When measured min/max ADC values
equal these limits, the channel is marked as saturated (state = 2). This
triggers a range expansion on the next iteration.

The saturation values are likely:
- For 12-bit ADC: min = -2048 (0xF800), max = +2047 (0x07FF)
- Stored at param_2 + 0x70 (min) and param_2 + 0x72 (max)


## AutoScaleRunner (autoscale_runner.cpp)

### Entry Point

`HandleStartAutoScale` (Ghidra 0x1cbe3d0) handles the IPC request
`Saleae::Graph::StartAutoScaleRequest`. It registers request handlers
on the `MsoOscilloscopeInstrumentNode`.

Error: `"Cannot start auto scale while it is already active"` -- prevents
concurrent auto-scale operations.

### OnWaveform (Ghidra 0x2e5bb20)

`AutoScaleRunner::OnWaveform` is called each time a new waveform arrives
during auto-scale. It dispatches to the appropriate kernel based on the
current state.

The function signature:
```
bool Saleae::AutoScaleRunner::OnWaveform(
    Saleae::MsoOscilloscopeInstrument &,
    const Graph::RawWaveformData &)
```

#### Initial State Setup

When state == 0 (initial), OnWaveform initializes the per-channel default
configuration:

For each of 4 channels:
- `enabled` = true (byte at sub-struct offset 4)
- `min_voltage` = -5.0 (double)
- `max_voltage` = +5.0 (double)
- `attn_factor` = 1.0 (double)
- Additional fields: 0, 2.0, 0.8 (possibly gain and filter settings)

Global settings:
- `sample_count_target` = 0x80 (128)
- `max_samples` = 5000
- `some_factor` = 0.1
- Per-channel `sample_counts` = [1000, 1000, 1000, 1000]

Then calls `FUN_02e477d0(instrument, -1)` to configure the instrument
for auto-scale capture (the -1 argument likely means "all channels").

#### State 1 (Full-Scale Result Processing)

When state == 1, OnWaveform receives the result from FullScaleKernel:
- Reads the config number from the result
- Stores it at param_1 + 0x650
- Clears a flag at param_1 + 0x1c0

#### IPC Result Reporting

Auto-scale progress is reported via:
- `"AutoScaleInProgress"` -- during capture iterations
- `"AutoScaleDone"` -- when auto-scale completes
- `/autoScaleResult` -- JSON path for the result data
- `/autoScaleId` -- identifier for the auto-scale session


## Auto-Scale State Machine

```
                          StartAutoScale request
                                  |
                                  v
                   +--------------+---------------+
                   |  AutoScaleRunner::OnWaveform  |
                   |    (state == 0: initialize)   |
                   +--------------+---------------+
                                  |
                    Set default configs: +/-5V
                    Configure instrument
                                  |
                                  v
                   +--------------+---------------+
                   | FullScaleKernel::HandleWaveform |
                   |  (Phase 1: zero-crossing scan)  |
                   +--------------+---------------+
                                  |
                    +-----------+-----------+
                    |           |           |
                   DC       Valid        Too Fast
                (< 6)     (6-2599)     (>= 2600)
                    |           |           |
                    v           v           v
               Adjust     Estimate     No change
               range      frequency    (give up)
                    |           |
                    v           v
                   +-----------+-----------+
                   | VerticalScaleKernel::HandleWaveform |
                   |  (Phase 2: min/max measurement)     |
                   +-----------+---------------------------+
                               |
                    +----------+----------+
                    |                     |
                Saturated            Not Saturated
                    |                     |
                    v                     v
               Widen range          Compute optimal
               (state = 2)         range with 20% margin
                    |                     |
                    v                     v
                   Iterate (up to 5x)    Return result
                               |
                               v
                         AutoScaleDone
```


## Per-Channel Result Structure

Both `FullScaleChannelResult` and `VerticalScaleChannelResult` serialize
with the same fields:

| JSON Path | Type | Description |
|-----------|------|-------------|
| `/error` | int/enum | 0 = success, 2 = saturated |
| `/minVoltage` | double | Minimum voltage of range |
| `/maxVoltage` | double | Maximum voltage of range |


## ADC-to-Voltage Conversion

The VerticalScaleKernel calls `FUN_02dfe390` to convert ADC codes to
voltages. This function takes:
- The ADC-to-voltage transform parameters (scale and offset from
  calibration data, stored in the waveform data at offsets +0xC0 and +0xC8)
- The ADC code to convert

The formula is the standard linear transform:
```
voltage = adc_code * scale + offset
```

Where scale and offset come from the per-channel calibration
(see doc/calibration/calibration-format.md).


## Initial Range Defaults

When auto-scale starts, it initializes all channels to:

| Parameter | Value |
|-----------|-------|
| min_voltage | -5.0 V |
| max_voltage | +5.0 V |
| attenuation factor | 1.0 |

This is a conservative starting range that covers common signal levels.
The FullScaleKernel then refines based on the actual signal. Note that
the default initial range of +/-30.0V is used in the `FUN_02e5de70` path
(VerticalScaleKernel init), matching the Logic Pro's maximum displayable
range of +/-15V with probe attenuation.


## VerticalScaleKernel Default Range

When the VerticalScaleKernel initializes (FUN_02e5de70), it uses a wider
default range:

| Parameter | Value |
|-----------|-------|
| min_voltage | -30.0 V |
| max_voltage | +30.0 V |
| enabled | true |

This 60V range (4 channels, each [-30, +30]) represents the absolute
maximum measurable range of the Logic Pro with 1x probe and is used as
the initial "full scale" measurement window.


## Decompiled Function Map

| Ghidra VA | ELF VA | Function | Source |
|-----------|--------|----------|--------|
| 0x2e5bb20 | 0x2d5bb20 | AutoScaleRunner::OnWaveform | autoscale_runner.cpp |
| 0x2e5c1e0 | 0x2d5c1e0 | ~FullScaleKernel (destructor) | autoscale_horizontal_kernel.cpp |
| 0x2e5c600 | 0x2d5c600 | FullScaleKernel::HandleWaveform | autoscale_horizontal_kernel.cpp |
| 0x2e5cfb0 | 0x2d5cfb0 | FullScaleKernel::waveform_to_state | autoscale_horizontal_kernel.cpp |
| 0x2e5d190 | 0x2d5d190 | span_cast<int16_t> (byte span to typed span) | utility |
| 0x2e5d2c0 | 0x2d5d2c0 | Logging helper (format + output) | logging |
| 0x2e5ddc0 | 0x2d5ddc0 | VerticalScaleKernel constructor | autoscale_vertical_kernel.cpp |
| 0x2e5de70 | 0x2d5de70 | VerticalScaleKernel initial config (full range) | autoscale_vertical_kernel.cpp |
| 0x2e5df90 | 0x2d5df90 | VerticalScaleKernel::HandleWaveform | autoscale_vertical_kernel.cpp |
| 0x2e5e680 | 0x2d5e680 | Extract VerticalScaleKernel results | autoscale_vertical_kernel.cpp |
| 0x2e5e6e0 | 0x2d5e6e0 | min/max ADC value finder (SIMD-like) | utility |
| 0x2e5ea00 | 0x2d5ea00 | Logging helper (with channel args) | logging |
| 0x2e5ecf0 | 0x2d5ecf0 | Logging helper (with range args) | logging |
| 0x1cbe3d0 | 0x1bbe3d0 | HandleStartAutoScale | IPC handler |
| 0x2e8c2e0 | 0x2d8c2e0 | FullScaleChannelResult serializer | serialization |
| 0x2e8d3d0 | 0x2d8d3d0 | VerticalScaleChannelResult serializer | serialization |


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|---------|
| Two-phase auto-scale (FullScale then VerticalScale) | **Confirmed** | Binary RTTI, source paths, decompiled call chain |
| MaxScopeChannels = 4 | **Confirmed** | Loop bounds `if (3 < idx) goto error` in both kernels |
| Zero-crossing hysteresis thresholds: +49/-50 ADC codes | **Confirmed** | Direct comparison constants in decompiled FullScaleKernel |
| Max crossings threshold: 2600 | **Confirmed** | `local_6a8 < 0xa28` comparison in decompiled code |
| Min crossings threshold: 6 | **Confirmed** | `local_6a8 < 6` comparison in decompiled code |
| 20% voltage range margin | **Confirmed** | `* 0.2` multiplication in both kernels |
| Min margin = 1/128 of full scale | **Confirmed** | `* 0.0078125` in VerticalScaleKernel |
| Min margin = 1.0V floor (FullScale) | **Confirmed** | `if (1.0 <= margin)` check in FullScaleKernel |
| Max 5 iterations | **Confirmed** | Count check against 5 in both kernels |
| ADC saturation detection | **Confirmed** | Min/max comparison at param_2+0x70/0x72 in VerticalScale |
| Result fields: /error, /minVoltage, /maxVoltage | **Confirmed** | Serialization functions at 0x2e8c2e0 and 0x2e8d3d0 |
| Initial range: +/-5V | **High** | Hex constants 0xc014 and 0x4014 (-5.0 and +5.0) in OnWaveform |
| VerticalScale initial range: +/-30V | **High** | Hex constants 0xc03e and 0x403e (-30.0 and +30.0) in init |
| Frequency estimation formula | **High** | Decompiled division chain, log message "Estimated frequency" |
| FullScaleKernel == "horizontal kernel" | **Confirmed** | Source path is autoscale_horizontal_kernel.cpp |


## Open Questions

1. **Config number table at 0x89ab80**: The FullScaleKernel references a
   table of uint32 values indexed by waveform_count. The values
   [262144, 0, 1073741824, 0, 0] do not map obviously to AFE config
   numbers. They may be sample count targets or timing parameters.

2. **Exact AutoScaleState enum values**: The state transitions use integer
   values (0, 1, 2) but the full enum definition is not known. Value 2
   appears to mean "saturated" for per-channel status.

3. **FUN_02dfe390 implementation**: The ADC-to-voltage conversion function
   was not decompiled in this session. It takes transform parameters
   from offsets +0xC0 and +0xC8 of the waveform data structure.

4. **Horizontal kernel frequency usage**: The estimated frequency is stored
   but it's unclear how it influences the subsequent VerticalScaleKernel
   behavior. It may be used for the "horizontal" part of auto-scale
   (adjusting timebase), which is separate from the vertical (voltage range)
   auto-scale documented here.

5. **FUN_02e5b780 (result construction)**: This function builds the
   OscilloscopeConfiguration result from the per-channel data. Its exact
   behavior was not fully decompiled. It takes parameters that include the
   per-channel min/max voltages, enabled flags, and the device constants.

6. **Per-channel saturation thresholds**: The values at param_2+0x70 and
   param_2+0x72 are assumed to be the ADC min/max (-2048 and +2047 for
   12-bit), but could be device-specific or range-dependent.
