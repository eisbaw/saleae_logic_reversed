# FPGA Glitch Filter and Hardware Trigger Registers

RE notes from TASK-0091 analysis of libgraph_server_shared.so v2.4.40.
Based on Ghidra decompilation of MsoFpga functions in the address range
0x02e63000-0x02e68000 (Ghidra VAs; subtract 0x100000 for ELF VAs).


## Summary

Hardware trigger support exists ONLY in MsoFpga (the MSO/oscilloscope device
path). Logic Pro 8/16, Logic 8, and Logic 4 devices have no FPGA-level trigger
hardware -- all their trigger detection is software-only via DigitalTriggerNode
and AnalogTriggerNode graph kernels.

The MSO FPGA trigger hardware provides:
- Configurable analog trigger threshold with hysteresis
- Multiple trigger modes (Normal, Auto, Single, Disabled)
- Multiple trigger types (at least edge and pulse)
- Glitch filter on digital channels (per-channel, configurable pulse width)
- Holdoff and timeout timers
- Software trigger injection


## TRIGGER_CONTROL Register (0x0070) -- Corrected Bit Layout

From decompiled `SetAnalogTriggerConfiguration` (ELF 0x02d654a0), the
register 0x0070 is packed as follows:

```
Decompiled packing expression:
  value = pbVar3[10] & 3                           // bits [1:0]  = GlitchFilterEnabled
        | ((pbVar3[9] & 0xf) << 4                  // bits [7:4]  = TriggerType
        |  (int16)(pbVar3 + 2) << 16               // bits [31:16] = TriggerThreshold
        |  (uint)pbVar3[0] << 8)                   // bits [15:8]  = ChannelSource
        + (pbVar3[8] & 3) * 4                      // bits [3:2]  = TriggerMode
```

Full bit-field layout:

| Bits    | Width | Field              | Source                        |
|---------|-------|--------------------|-------------------------------|
| [31:16] | 16    | TriggerThreshold   | config.mTriggerThreshold (int16, ADC code) |
| [15:8]  | 8     | ChannelSource      | config.mChannelSource (uint8) |
| [7:4]   | 4     | TriggerType        | config.mTriggerType (MsoFpga::TriggerType_impl::TriggerType) |
| [3:2]   | 2     | TriggerMode        | config.mTriggerMode (MsoFpga::TriggerMode_impl::TriggerMode) |
| [1:0]   | 2     | GlitchFilterEnabled | config.mGlitchFilterEnabled (uint8, 2 bits) |

**Correction from prior documentation**: Previous notes (fpga-register-map.md)
had the bit layout partially wrong. The ChannelSource field at bits [15:8]
was not previously identified. The enable bit at [0] is actually
GlitchFilterEnabled, not a simple enable/disable flag.


## TriggerMode Enum (MsoFpga::TriggerMode_impl::TriggerMode)

2-bit field at register 0x0070 bits [3:2]. Validated by SetField assertion:
`(value & 0xFC) == 0` (value must fit in 2 bits).

| Value | Name     | Evidence |
|-------|----------|----------|
| 0     | Normal   | Default/zero-initialized; uses mTriggerNormal_HoldoffSamples/TimeoutSamples |
| 1     | Auto     | ProcessPendingConfiguration writes UI mode 0 + 1 = 1; uses mTriggerAuto_HoldoffSamples/TimeoutSamples |
| 2     | Single   | ProcessPendingConfiguration writes UI mode 1 + 1 = 2; "Single" string in binary |
| 3     | Disabled | DisableAnalogTrigger writes 0x0C to reg 0x70 (bits [3:2] = 3); also UI mode 2 + 1 = 3 |

**Confidence**: **Confirmed** (TASK-0214). All four values verified from
decompiled ProcessPendingConfiguration (ELF 0x2d4a280,
oscilloscope_instrument.cpp).

The UI-to-FPGA mapping applies a +1 offset: UI trigger mode values {0,1,2}
are stored as FPGA register values {1,2,3}. Value 0 (Normal) is the default
state used when no explicit trigger mode is set, or before trigger
configuration is applied. The UI mode field is at config struct offset 0x18c;
when mode >= 3, no mode value is written (the field is left at its previous
value).

Decompiled evidence for the +1 transform:
```
// From ProcessPendingConfiguration (oscilloscope_instrument.cpp)
uVar6 = *(uint *)(config + 0x18c);  // UI trigger mode
if (uVar6 < 3) {
    triggerStruct.mTriggerMode = (char)uVar6 + 1;  // +1 offset
}
// if uVar6 >= 3, mode is not set (left at previous value)
```

DisableAnalogTrigger (ELF 0x02d65280) writes literal 0x0C = 0b00001100,
confirming value 3 = Disabled independently.

### DisableAnalogTrigger (ELF 0x02d65280)

```
Source: mso_fpga.cpp line 0x2a9, function DisableAnalogTrigger
1. Log: "Disabling analog trigger"
2. Write FPGA reg 0x70 = 0x0C
```

Value 0x0C breakdown:
- bits [1:0] = 0  (GlitchFilter disabled)
- bits [3:2] = 3  (TriggerMode = Disabled)
- bits [7:4] = 0  (TriggerType = 0)
- bits [15:8] = 0 (ChannelSource = 0)
- bits [31:16] = 0 (Threshold = 0)


## TriggerType Enum (MsoFpga::TriggerType_impl::TriggerType)

4-bit field at register 0x0070 bits [7:4]. Validated by SetField assertion:
`(value & 0xF0) == 0` (value must fit in 4 bits, range 0-15).

| Value | Name         | Evidence |
|-------|--------------|----------|
| 0     | EdgeTrigger  | DigitalTriggerNode switch: iVar1==0 -> FUN_01be1960 (EdgeTrigger_Detect) |
| 1     | PulseTrigger | DigitalTriggerNode switch: iVar1==1 -> FUN_01be23c0 (PulseTrigger_Detect) |
| 2-15  | **Reserved** | DigitalTriggerNode throws exception for any value != 0 and != 1 |

**Confidence**: **Confirmed** (TASK-0208). Only two trigger types exist in
the binary. The DigitalTriggerNode::CheckTrigger function (ELF 0x1ae1720)
contains a definitive two-way switch:

```
// From DigitalTrigger_CheckTrigger (digital_trigger_node.cpp)
iVar1 = *(int *)(config + 0x58);  // trigger type
if (iVar1 == 1) {
    FUN_01be23c0(...);  // PulseTrigger_Detect
} else {
    if (iVar1 != 0) {
        FUN_01be5210();  // throws exception (invalid trigger type)
    }
    FUN_01be1960(...);  // EdgeTrigger_Detect
}
```

FUN_01be5210 (ELF 0x1ae5210) is a 47-byte function that allocates and
throws a C++ exception via __cxa_throw. It is NOT an assertion with a
message -- it throws an unnamed exception type. Any trigger type value
other than 0 or 1 is a hard error.

The ProcessPendingConfiguration caller (ELF 0x2d4a280) passes the trigger
type value from config struct offset 0x198 with only a `< 4` bounds check,
meaning the UI allows values 0-3. However, the software trigger engine only
accepts 0-1. Values 2-3 would reach the FPGA hardware but crash the software
trigger path. The 4-bit FPGA field (supporting 0-15) is reserved for
potential future FPGA-only trigger types not implemented in v2.4.40.

The binary contains strings "EdgeTrigH" and "PulseTriH" which appear to be
truncated hardware trigger type names (EdgeTriggerHardware, PulseTriggerHardware),
matching the two implemented types.


## GlitchFilterEnabled Field

2-bit field at register 0x0070 bits [1:0]. Validated by SetField assertion:
`(value & 0xFC) == 0` (value must fit in 2 bits).

| Value | Meaning |
|-------|---------|
| 0     | Glitch filter disabled |
| 1-3   | Glitch filter enabled (specific modes unknown) |

**Confidence**: Partial (TASK-0209). The 2-bit width is confirmed from the
SetField assertion. Value 0 = disabled is confirmed from
DisableAnalogTrigger (writes 0x0C with bits [1:0] = 0). The specific
meanings of values 1, 2, and 3 could not be determined from static analysis.

The ProcessPendingConfiguration caller (ELF 0x2d4a280) sets this field from
the AnalogTriggerConfiguration struct at offset 0x0a (mGlitchFilterEnabled).
No enum type name or string constants for this field were found in the
binary. The field is passed straight through without transformation -- unlike
TriggerMode, no +1 offset is applied. The FPGA firmware likely implements
different glitch filter behaviors (e.g., different filter widths or
algorithms) for values 1-3, but this is not visible from the host software.

This field controls whether the FPGA applies a glitch filter to the trigger
input before trigger detection. It is separate from the per-channel digital
glitch filter in registers 0x0170-0x017C.


## AnalogTriggerConfiguration Struct (Corrected)

From decompiled SetAnalogTriggerConfiguration assertions and register writes:

| Offset | Field                           | Type    | Validation / Notes |
|--------|---------------------------------|---------|--------------------|
| +0x00  | mChannelSource                  | uint8   | < mDeviceConstants.mAnalogChannelCount (max 4) |
| +0x02  | mTriggerThreshold               | int16   | ADC code value (signed) |
| +0x04  | mTriggerHysteresisFalling        | int16   | >= mTriggerThreshold |
| +0x06  | mTriggerHysteresisRising         | int16   | <= mTriggerThreshold |
| +0x08  | mTriggerMode                    | uint8   | 2 bits [0:1], SetField<TriggerMode> |
| +0x09  | mTriggerType                    | uint8   | 4 bits [0:3], SetField<TriggerType> |
| +0x0a  | mGlitchFilterEnabled            | uint8   | 2 bits [0:1] |
| +0x10  | mTriggerInitialHoldoffSamples    | uint64  | <= MaxInitialHoldoffSamples, must be multiple of 8 |
| +0x18  | mTriggerNormal_HoldoffSamples    | uint64  | <= MaxHoldoffSamples, must be multiple of 8 |
| +0x20  | mTriggerNormal_TimeoutSamples    | uint64  | <= MaxTimeoutSamples, must be multiple of 8 |
| +0x28  | mTriggerAuto_HoldoffSamples      | uint64  | <= MaxHoldoffSamples, must be multiple of 8 |
| +0x30  | mTriggerAuto_TimeoutSamples      | uint64  | <= MaxTimeoutSamples, must be multiple of 8 |


## Hysteresis Register (0x007C)

The TRIGGER_AUTO_CONFIG register packs hysteresis values:

```
Decompiled expression:
  value = (uint16)(param_2 + 6) | (int16)(param_2 + 4) << 16
```

| Bits     | Width | Field                       |
|----------|-------|-----------------------------|
| [15:0]   | 16    | mTriggerHysteresisRising    |
| [31:16]  | 16    | mTriggerHysteresisFalling   |


## Holdoff and Timeout Register Writes

SetAnalogTriggerConfiguration writes holdoff/timeout values in this order:

| Register | Content | Samples Source |
|----------|---------|----------------|
| 0x0080   | Initial holdoff (low 32 bits) | mTriggerInitialHoldoffSamples |
| 0x0090   | Initial holdoff (high 13 bits) | (holdoff >> 35) & 0x1FFF |
| 0x0074   | Normal holdoff | mTriggerNormal_HoldoffSamples |
| 0x0078   | Normal timeout | mTriggerNormal_TimeoutSamples |
| 0x0084   | Auto holdoff | mTriggerAuto_HoldoffSamples |
| 0x0088   | Auto timeout | mTriggerAuto_TimeoutSamples |
| 0x008C   | Combined high bits | (timeout_hi & 0xFFF) << 16 | (holdoff_hi & 0xFFF) |
| 0x007C   | Hysteresis | rising[15:0] | falling[31:16] |
| 0x0070   | Trigger control | Full packed register (see above) |

All holdoff/timeout values undergo SamplesToTicks conversion before writing:
`ticks = (samples + 7) >> 3`

If the sample count is not a multiple of 8, a warning is logged:
"Setting a trigger holdoff or timeout in samples that is not a multiple of 8: {}"


## TRIGGER_TICK_SIZE Register (0x0094)

Function at ELF 0x02d668e0 (Ghidra 0x02e668e0) writes register 0x0094
with a SamplesToTicks-converted value:

```
void SetTriggerTickSize(MsoFpga* this, uint64_t samples) {
    if (samples % 8 != 0) {
        warn("Setting a trigger holdoff or timeout in samples that is not a multiple of 8: {}");
    }
    WriteRegister(0x0094, (uint32_t)((samples + 7) >> 3));
}
```

This register likely informs the FPGA of the tick granularity for its
internal counters, allowing it to convert between sample-domain and
tick-domain values.


## SamplesToTicks Function (ELF 0x02d65420)

Standalone utility used across trigger configuration:

```c
int64_t SamplesToTicks(uint64_t samples) {
    if ((samples & 7) != 0) {
        Log(WARNING, "Setting a trigger holdoff or timeout in samples "
                     "that is not a multiple of 8: %llu", samples);
    }
    return (int64_t)(samples + 7) >> 3;  // divide by 8, round up
}
```


## Per-Channel Digital Glitch Filter (MSO Only)

### InternalSetDigitalConfiguration (ELF 0x02d663f0)

This function writes a single digital channel configuration to an FPGA register.
The register address is computed by the caller and passed as param_2.

```
Source: mso_fpga.cpp line 0x32b, function InternalSetDigitalConfiguration

Parameters:
  param_1 = MsoFpga* this
  param_2 = register address (uint16)
  param_3 = packed: mPulseWidth (upper 32 bits) | mEnabled (bit 0 of lower 32 bits)

Validation:
  1. mPulseWidth must be in range [5, 250005]
     (MinGlitchFilterLength=5, MaxGlitchFilterLength=250005)
     Checked as: (pulseWidth - 5) < 0x3D08C (250001-5+1)
  2. mPulseWidth must be exactly 5, or a multiple of 10
     Checked as: pulseWidth == 5 || (pulseWidth % 10) == 0

Register value packed as:
  value = (mEnabled & 1) << 20 | mPulseWidth
```

Note: The previous analysis in fpga-register-map.md mentioned
`(pulse_width & 1) << 20` for bit 20, but the decompilation shows it is
actually the **enabled** flag at bit 20, with pulse_width in bits [19:0].
The `param_3` parameter packs both: `enabled` in the low 32 bits (bit 0)
and `pulseWidth` in the upper 32 bits.

### Glitch Filter Constraints

| Constant | Value | Source |
|----------|-------|--------|
| MinGlitchFilterLength | 5 | Runtime config `/digitalLimit_MinGlitchFilterLength` |
| MaxGlitchFilterLength | ~250,005 | Runtime config `/digitalLimit_MaxGlitchFilterLength` |
| Valid values | 5, or any multiple of 10 | Assertion check in InternalSetDigitalConfiguration |

Error message for invalid pulse width:
"Pulse width must be 5 or a multiple of 10"

### Address Calculation

Two address calculation functions route to InternalSetDigitalConfiguration:

**CalculateDigitalConfigurationAddress_DigitalDerived** (ELF 0x02d66870):
```
address = 0x0170 + channel_index * 4
Constraint: channel_index < 4 (address < DigitalConfiguration_DigitalDerived_END)
Registers: 0x0170, 0x0174, 0x0178, 0x017C
```

**CalculateDigitalConfigurationAddress_SmartCable** (ELF 0x02d66380):
```
address = 0x0120 + port * 16 + sub_channel * 4
Constraint: port * 16 + sub_channel * 4 < 0x50
Registers: 0x0120-0x016C (5 ports x 4 channels)
```

### Per-Channel Register Format

| Bits     | Width | Field       |
|----------|-------|-------------|
| [19:0]   | 20    | PulseWidth  |
| 20       | 1     | Enabled     |
| [31:21]  | 11    | Unused      |


## Graph-Level Glitch Filter Configuration

The DisaggregatorNode handles glitch filter configuration at the graph level:

**DisaggregatorActions::ConfigureGlitchFilters**: A request action type used
to configure glitch filters before data acquisition begins. Cannot be sent
after the node has started receiving data ("Cannot configure glitch filters
after node has received data").

**DisaggregatorSettingsData**: Settings data that must be provided before any
other data to the DisaggregatorNode.

The digital channel config logging format:
```
"Updating LA ch {}, invert={} glitch={} pattern_gen_enabled={}"
```
shows that inversion, glitch filtering, and pattern generation are all
per-channel digital settings at the graph level.

The per-channel config display format:
```
"0: enabled={}, low={} V, high={} V, invert={}, min_pulse_width={} us"
```
shows that the glitch filter is exposed to users as "minimum pulse width"
in microseconds.


## Hardware Trigger Support by Device

| Feature | Logic 4/8 | Logic Pro 8/16 | MSO |
|---------|-----------|----------------|-----|
| FPGA register interface | FpgaDeviceFeatures (8-bit addr) | FpgaDeviceFeatures (8-bit addr) | MsoFpga (16-bit addr) |
| Digital trigger | Software only | Software only | Software only |
| Analog trigger | Software only | Software only | **FPGA hardware** |
| Glitch filter | **Not available** | **Not available** | **FPGA per-channel** |
| Trigger modes | N/A | N/A | Normal/Auto/Single/Disabled |
| Trigger types | EdgeTrigger/PulseTrigger (SW) | EdgeTrigger/PulseTrigger (SW) | EdgeTrigger/PulseTrigger (HW + SW) |
| Holdoff/timeout | Software | Software | FPGA registers 0x74-0x90 |
| Software trigger injection | N/A | N/A | FPGA regs 0x98/0x9c |
| Trigger threshold | Software (ADC samples) | Software (ADC samples) | FPGA reg 0x70 bits [31:16] |
| Hysteresis | Software | Software | FPGA reg 0x7C |

The FpgaDeviceFeatures (8-bit) register interface used by Logic 4/8 and Logic
Pro 8/16 has NO trigger-related registers. The only registers are for capture
configuration, channel enables, sample rate dividers, and ADC SPI. All trigger
logic for these devices runs host-side in the graph processing pipeline.


## DisableAnalogChannels (ELF 0x02d650e0)

```
Source: mso_fpga.cpp line 0x2a1, function DisableAnalogChannels
1. Log: "Disabling analog channels"
2. Write FPGA reg 0x40 = 0x00000000
```

Zeroes the CAPTURE_CONTROL register to disable analog capture.


## Additional MsoFpga Functions Identified

| ELF VA     | Ghidra VA  | Function | Notes |
|------------|------------|----------|-------|
| 0x02d650e0 | 0x02e650e0 | DisableAnalogChannels | Writes reg 0x40 = 0 |
| 0x02d65280 | 0x02e65280 | DisableAnalogTrigger | Writes reg 0x70 = 0x0C |
| 0x02d65420 | 0x02e65420 | SamplesToTicks | (samples+7) >> 3 |
| 0x02d654a0 | 0x02e654a0 | SetAnalogTriggerConfiguration | Main trigger config |
| 0x02d661f0 | 0x02e661f0 | SetScopeConfigurationId | Writes to reg table at DAT_0089ad80 |
| 0x02d662c0 | 0x02e662c0 | WriteRegister_0x194 | Writes reg 0x194 (unknown purpose) |
| 0x02d66380 | 0x02e66380 | SetSmartCableDigitalConfig | Calculates SmartCable addr, calls InternalSetDigitalConfiguration |
| 0x02d663f0 | 0x02e663f0 | InternalSetDigitalConfiguration | Per-channel glitch filter + enable |
| 0x02d66590 | 0x02e66590 | SetCommitBankControl | Writes reg 0x1C0 with enable bits |
| 0x02d666a0 | 0x02e666a0 | SetEnabledSmartCableDigitalChannels | Per-port channel enables in reg 0x1C0 |
| 0x02d66870 | 0x02e66870 | SetDigitalDerivedConfig | Calculates DigitalDerived addr, calls InternalSetDigitalConfiguration |
| 0x02d668e0 | 0x02e668e0 | SetTriggerTickSize | Writes reg 0x94 = SamplesToTicks(samples) |
| 0x02d66a10 | 0x02e66a10 | SetDigitalDerivedVoltageThresholds | Writes reg from DAT_0089ad8a table + secondary write |
| 0x02d66b70 | 0x02e66b70 | SetScopeChannelConfig | Writes reg from DAT_0089ad92 table |
| 0x02d66c60 | 0x02e66c60 | SetDigitalThresholdEnable | Writes reg 0x104 = 1 |
| 0x02d66d30 | 0x02e66d30 | ReadFpgaVersion | Reads regs 200 (0xC8) and 204 (0xCC) |
| 0x02d66d60 | 0x02e66d60 | SetSoftwareTrigger | Writes regs 0x98/0x9C |
| 0x02d66f30 | 0x02e66f30 | WriteSpi | Writes reg 0x04 (SPI bridge) |
| 0x02d671c0 | 0x02e671c0 | ReadSpi_7BitAddress | Read via reg 0x04 (7-bit address) |
| 0x02d67230 | 0x02e67230 | ReadSpi_15BitAddress | Read via reg 0x04 (15-bit address) |


## Scope Instrument Register Group (TASK-0204)

### Register 0x0194 (SCOPE_CHANNEL_CONFIG master)

Function `WriteRegister_0x194` at ELF 0x02d662c0 writes register 0x0194
with a single uint32 parameter. Called from `ProcessPendingConfiguration`
in `oscilloscope_instrument.cpp` -- the value is `*(uint32_t *)config_data`
(offset 0x00 of the oscilloscope pending configuration struct). The internal
bit layout of this uint32 is not yet decoded.

### Register array 0x0198-0x01A8 (per-channel scope config, 5 channels)

Function `SetScopeChannelConfig` at ELF 0x02d66b70 writes to registers
looked up from table DAT_0089ad92: {0x0198, 0x019C, 0x01A0, 0x01A4, 0x01A8}.
Index is 0-4 (bounds-checked with `array::at`). Value is `param & 0xFF`.

Packed by `AddInstrumentInternal` in mso_device.cpp using `SetField<uint8_t>`:
- bits [7:5] = InstrumentIndex (MsoInstrumentIndex, 3 bits, assertion: < 8)
- bits [4:0] = InstrumentType (sub-instrument type, 5 bits)

### Register array 0x0180-0x0190 (configuration IDs, 5 channels)

Function `SetScopeConfigurationId` at ELF 0x02d661f0 writes to registers
looked up from table DAT_0089ad80: {0x0180, 0x0184, 0x0188, 0x018C, 0x0190}.

### Register array 0x01B0-0x01BC (voltage thresholds, 4 channels)

Function `SetDigitalDerivedVoltageThresholds` at ELF 0x02d66a10 writes to
registers looked up from table DAT_0089ad8a: {0x01B0, 0x01B4, 0x01B8, 0x01BC}.


## Confidence Assessment

| Finding | Confidence | Source |
|---------|-----------|--------|
| TRIGGER_CONTROL (0x70) full bit layout | **Confirmed** | Decompiled SetAnalogTriggerConfiguration packing expression |
| ChannelSource in bits [15:8] | **Confirmed** | Decompiled: `(uint)*pbVar3 << 8` where pbVar3[0] = mChannelSource |
| GlitchFilterEnabled in bits [1:0] | **Confirmed** | Decompiled: `pbVar3[10] & 3` and SetField assertion |
| TriggerMode 3 = Disabled | **Confirmed** | DisableAnalogTrigger writes 0x0C (bits [3:2] = 3) |
| TriggerMode 0=Normal, 1=Auto, 2=Single | **Confirmed** | ProcessPendingConfiguration applies +1 offset: UI {0,1,2} -> FPGA {1,2,3}; value 0 is default |
| TriggerType 0=Edge, 1=Pulse | **Confirmed** | DigitalTriggerNode switch with exception for any other value |
| TriggerType 2-15 reserved | **Confirmed** | FUN_01be5210 throws exception for type != 0 and != 1 |
| GlitchFilterEnabled modes 1-3 | **Unknown** | 2-bit field confirmed; specific mode behaviors are FPGA-internal |
| InternalSetDigitalConfiguration bit layout | **Confirmed** | Decompiled: enabled<<20 | pulseWidth |
| SamplesToTicks = (samples+7)>>3 | **Confirmed** | Standalone function decompiled |
| Hysteresis reg 0x7C layout | **Confirmed** | Decompiled: rising[15:0] | falling[31:16] |
| Logic Pro has no HW triggers | **Confirmed** | FpgaDeviceFeatures has no trigger registers |
| Register 0x194 exists | **Confirmed** | Decompiled function writes it; internal bit layout TBD |
| Scope register tables (0x0180-0x01A8) | **Confirmed** | DAT_0089ad80/ad8a/ad92 memory reads match decompiled index calculations |
| Per-channel scope config byte layout | **Confirmed** | SetField assertions in AddInstrumentInternal: index[7:5] | type[4:0] |
| Voltage threshold table (0x01B0-0x01BC) | **Confirmed** | DAT_0089ad8a memory read; 4 entries at stride 4 |


## Key Addresses (ELF VAs)

| Address    | Ghidra VA  | Function |
|------------|------------|----------|
| 0x02d650e0 | 0x02e650e0 | DisableAnalogChannels |
| 0x02d65280 | 0x02e65280 | DisableAnalogTrigger |
| 0x02d65420 | 0x02e65420 | SamplesToTicks |
| 0x02d654a0 | 0x02e654a0 | SetAnalogTriggerConfiguration |
| 0x02d661f0 | 0x02e661f0 | SetScopeConfigurationId |
| 0x02d662c0 | 0x02e662c0 | WriteRegister_0x194 |
| 0x02d66380 | 0x02e66380 | SetSmartCableDigitalConfig |
| 0x02d663f0 | 0x02e663f0 | InternalSetDigitalConfiguration |
| 0x02d66590 | 0x02e66590 | SetCommitBankControl |
| 0x02d666a0 | 0x02e666a0 | SetEnabledSmartCableDigitalChannels |
| 0x02d66870 | 0x02e66870 | SetDigitalDerivedConfig |
| 0x02d668e0 | 0x02e668e0 | SetTriggerTickSize |
| 0x02d66a10 | 0x02e66a10 | SetDigitalDerivedVoltageThresholds |
| 0x02d66b70 | 0x02e66b70 | SetScopeChannelConfig |
| 0x02d66c60 | 0x02e66c60 | SetDigitalThresholdEnable |
| 0x02d66d30 | 0x02e66d30 | ReadFpgaVersion |
| 0x02d66d60 | 0x02e66d60 | SetSoftwareTrigger |
| 0x02d66f30 | 0x02e66f30 | WriteSpi |
| 0x02d671c0 | 0x02e671c0 | ReadSpi_7BitAddress |
| 0x02d67230 | 0x02e67230 | ReadSpi_15BitAddress |
| 0x01ae1720 | 0x01be1720 | DigitalTriggerNode::CheckTrigger (trigger type switch) |
| 0x01ae1960 | 0x01be1960 | EdgeTrigger_Detect |
| 0x01ae23c0 | 0x01be23c0 | PulseTrigger_Detect |
| 0x01ae5210 | 0x01be5210 | InvalidTriggerType_Throw (exception for type != 0,1) |
| 0x02d4a280 | 0x02e4a280 | ProcessPendingConfiguration (oscilloscope_instrument.cpp) |
