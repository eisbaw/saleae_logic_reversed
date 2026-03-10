# FPGA Register Map Analysis

Analysis date: 2026-03-10
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Tasks: TASK-0031, TASK-0068


## Summary

The host communicates with the FPGA through two distinct register interfaces:

1. **MsoFpga (16-bit address, 32-bit value)** -- Used by the MSO/oscilloscope
   device path. Register writes go directly as 6-byte USB packets over EP1.
   39 register addresses documented.

2. **FpgaDeviceFeatures (8-bit address, 8-bit value)** -- Used by the Logic 4/8
   and Logic Pro 8/16 device path. Register writes are wrapped in EP1 opcode
   0x80 (write) and 0x81 (read) commands. Max 31 registers per batch.
   21 register addresses documented.

Both paths ultimately reach the FPGA through the FX3 microcontroller over
USB bulk EP1 OUT/IN.


## Extraction Method

1. Located format strings `"FPGA WriteRegister 0x{:04x} = 0x{:08x} ({})"`
   and `"FPGA ReadRegister 0x{:04X} = 0x{:08X}"` in the binary.

2. Found 59 WriteRegister log sites and 1 ReadRegister log site via Ghidra
   xref analysis.

3. Extracted register addresses from `movw $imm16` immediates and decompiled
   function bodies.

4. Decompiled key functions using Ghidra headless (DecompileFpgaRegisters.java):
   59 functions total, including all MsoFpga register writers, FpgaDeviceFeatures
   vtable methods, Logic2FpgaDevice capture configuration, and SetField
   instantiations for bit-field layout.

5. Cross-referenced assertion strings (__PRETTY_FUNCTION__) to identify functions.


## MsoFpga Register Interface (16-bit address)

### Wire Protocol

- **Address width**: 16 bits
- **Value width**: 32 bits
- **Byte order**: Big-endian on wire (byte-swapped via x86 ROL instruction)
- **Transport**: USB bulk EP1 OUT (write) and EP1 IN (read)

```
Write: [addr_hi] [addr_lo] [val_3] [val_2] [val_1] [val_0]  (6 bytes)
Read:  [addr_hi] [addr_lo]  -> response: [val_3] [val_2] [val_1] [val_0]
```

Source file:
`/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/component/mso_fpga.cpp`


### Complete Register Map

#### SPI / Status / JESD Control (0x0004-0x0008)

| Address | Name | Description |
|---------|------|-------------|
| 0x0004 | SPI_DATA | SPI bridge to ADC. See bit-field layout below. |
| 0x0008 | STATUS_DDR_JESD | DDR/JESD status and control. See bit-field layout below. |

#### GPIF Control (0x0020-0x0024)

| Address | Name | Description |
|---------|------|-------------|
| 0x0020 | GPIF_ENABLE | Enables GPIF data interface between FX3 and FPGA. |
| 0x0024 | GPIF_FLUSH_TIMEOUT | GPIF flush timeout in 100MHz clock ticks. |

#### Test Data Configuration (0x0030-0x0034)

| Address | Name | Description |
|---------|------|-------------|
| 0x0030 | TEST_DATA_SIZE | Test data pattern size. Written with `config.mSize`. |
| 0x0034 | TEST_DATA_FREQ_CTL | Packed: `mDataObjectEnabled << 20 | mTestPatternEnabled << 21 | mFrequency[19:0]`. |

**TEST_DATA_FREQ_CTL (0x0034) bit fields**:
```
bits [19:0]:  Frequency (max 0xFFFFF, 20 bits)
bit  20:      DataObject enabled
bit  21:      TestPattern enabled
bits [31:22]: Unused
```

Constraint: Only one of TestPattern and DataObject may be enabled at a time.

#### Capture Control (0x0040)

| Address | Name | Description |
|---------|------|-------------|
| 0x0040 | CAPTURE_CONTROL | Master capture control. See bit-field layout below. |

**CAPTURE_CONTROL (0x0040) bit fields** (from SetAnalogChannelConfiguration):
```
bit  4:       Capture mode (from config byte 0)
bits [6:5]:   DownsampleMode (2 bits, via SetField<DownsampleMode>)
bits [3:0]:   Preserved from previous value (masked with 0x8F)
bits [31:7]:  Other configuration (preserved/set elsewhere)
```

The field assembly is:
`value = (config.mCaptureMode << 4) | (DownsampleMode & 3) << 5 | (prev_value & 0x8F)`

#### Scope Configuration IDs (0x0044-0x0050)

| Address | Name | Description |
|---------|------|-------------|
| 0x0044 | SCOPE_CONFIG_ID_0 | Configuration ID 0. |
| 0x0048 | SCOPE_CONFIG_ID_1 | Configuration ID 1. |
| 0x004C | SCOPE_CONFIG_ID_2 | Configuration ID 2. |
| 0x0050 | SCOPE_CONFIG_ID_3 | Configuration ID 3. |

Logged as: "Setting scope configuration IDs: {} {} {} {}"

#### Analog Channel Configuration (0x0054-0x0068)

| Address | Name | Struct Offset | Description |
|---------|------|---------------|-------------|
| 0x0054 | ANALOG_SAMPLE_DIVIDER | +0x04 | Sample rate divider. `config.mSamplesPerSecondDivider`. |
| 0x0058 | ANALOG_PRE_TRIGGER | +0x10 | Negated pre-trigger length: `-(config.mPreTriggerLength)`. |
| 0x005C | ANALOG_POST_TRIGGER | +0x18 | Post-trigger length: `config.mPostTriggerLength`. |
| 0x0060 | ANALOG_COMPUTED_RATE | computed | `round(DAT_0044e1c8 / divider)` -- derived from 0x0054 value. |
| 0x0064 | ANALOG_TRIGGER_HI | computed | Upper bits: `(pre_hi & 0xFFFF) | (post_hi & 0xFFFF) << 16`. |
| 0x0068 | ANALOG_SAMPLES_PER_CHUNK | +0x20 | Samples per chunk: `config.mSamplesPerChunk`. |

Constraints (from assertions):
- `sample_length >= MsoConstants::MinTriggerBufferSamples` (8)
- `sample_length <= MaxTriggerBufferSamples_Xilinx` (0x3FFFC000) or `_Altera` (0x2FFFC000)
- `-config.mPreTriggerLength >= MinTriggerBufferIndex_Xilinx/Altera`
- `config.mPostTriggerLength <= MsoConstants::MaxTriggerBufferIndex` (0xBFFF)
- `config.mSamplesPerChunk` must be in range (Xilinx: 0x100-0x1E00, Altera: 0x40-0x1780)
- `SamplesPerPacketMultiple_WithStats` alignment required

The FPGA variant (Xilinx vs Altera) is detected via `FUN_02e62780()`: return value
`& 0xFE == 0` means Altera, otherwise Xilinx.

#### Trigger Configuration (0x0070-0x009C)

| Address | Name | Description |
|---------|------|-------------|
| 0x0070 | TRIGGER_CONTROL | Trigger control with packed bit fields. See layout below. |
| 0x0074 | TRIGGER_NORMAL_HOLDOFF | Normal-mode holdoff: `SamplesToTicks(normal_holdoff_samples)`. |
| 0x0078 | TRIGGER_NORMAL_TIMEOUT | Normal-mode timeout: `SamplesToTicks(normal_timeout_samples)`. |
| 0x007C | TRIGGER_AUTO_CONFIG | Auto-mode config: `hysteresis_falling | threshold << 16`. |
| 0x0080 | TRIGGER_INITIAL_HOLDOFF_LO | Initial holdoff low 32: `SamplesToTicks(initial_holdoff)`. |
| 0x0084 | TRIGGER_AUTO_HOLDOFF | Auto-mode holdoff: `SamplesToTicks(auto_holdoff_samples)`. |
| 0x0088 | TRIGGER_AUTO_TIMEOUT | Auto-mode timeout: `SamplesToTicks(auto_timeout_samples)`. |
| 0x008C | TRIGGER_HOLDOFF_TIMEOUT_HI | Packed high bits: `timeout_hi[11:0] << 16 | holdoff_hi[11:0]`. |
| 0x0090 | TRIGGER_INITIAL_HOLDOFF_HI | Initial holdoff high: `(holdoff >> 35) & 0x1FFF` (13 bits). |
| 0x0094 | TRIGGER_TICK_SIZE | SamplesToTicks conversion factor (written near holdoff computation). |
| 0x0098 | SW_TRIGGER_LO | Software trigger timestamp, low 32 bits. |
| 0x009C | SW_TRIGGER_HI | Software trigger timestamp, high 32 bits. |

**SamplesToTicks conversion**: `ticks = (samples + 7) >> 3` (divide by 8, round up).
Holdoff/timeout values must be multiples of 8 samples (warning logged if not).

**TRIGGER_CONTROL (0x0070) bit fields** (from decompiled SetAnalogTriggerConfiguration):
```
bits [7:0]:   Enable/config byte: threshold[7:0] << 0 (TriggerThreshold low byte)
bits [7:4]:   Actually reinterpreted: see packed formula below
bits [9:8]:   TriggerMode (2 bits) -- Saleae::MsoFpga::TriggerMode_impl::TriggerMode
bit  [1:0]:   From SetField<DownsampleMode> check area
bits [31:16]: TriggerThreshold (16 bits, signed)
```

The full packed value from decompilation:
```
value = (config.mDownsampleMode & 3) << 6    // not confirmed, see assertion check area
      | (config.mTriggerType & 0xF) << 4     // TriggerType: 4 bits [7:4]
      | (config.mTriggerMode & 3) << 2       // TriggerMode: 2 bits [3:2]
      | (config.mTriggerThreshold << 16)      // Threshold: 16 bits [31:16]
      | (config.mEnable)                      // Enable: bit [0] or similar
```

SetField bounds checks (from assertion strings):
- TriggerType: `(value & 0xF0) == 0` -> max 4 bits (0-15)
- TriggerMode: `(value & 0xFC) == 0` -> max 2 bits (0-3)
- DownsampleMode (in 0x0040, not 0x0070): `(value & 0xFC) == 0` -> max 2 bits

**TRIGGER_AUTO_CONFIG (0x007C) bit fields**:
```
bits [15:0]:  Hysteresis rising value (from config+6)
bits [31:16]: Hysteresis falling value (from config+4)
```

**TRIGGER_HOLDOFF_TIMEOUT_HI (0x008C) bit fields**:
```
bits [11:0]:  Holdoff high (12 bits, from holdoff >> 35)
bits [27:16]: Timeout high (12 bits, from timeout >> 35)
bits [31:28]: Unused
```

Constraint: holdoff and timeout high values must be < 0x1000 (12 bits).

#### Initialization (0x00E0)

| Address | Name | Description |
|---------|------|-------------|
| 0x00E0 | INIT_CONFIG | Post-programming initialization register. |

#### JESD BER Test (0x00FC)

| Address | Name | Description |
|---------|------|-------------|
| 0x00FC | JESD_BER_TEST | JESD204B Bit Error Rate test control. |

#### Digital Voltage Threshold (0x0104)

| Address | Name | Description |
|---------|------|-------------|
| 0x0104 | DIGITAL_VOLTAGE_THRESHOLD | Sets digital logic voltage threshold. |

Referenced by `SetDigitalDerivedVoltageThresholds` with an additional
secondary write via a different mechanism (possibly I2C DAC).

#### Smart Cable Digital Configuration (0x0120-0x016F)

Address formula: `address = 0x0120 + port * 16 + sub_channel * 4`

| Address Range | Description |
|---------------|-------------|
| 0x0120-0x012C | Port 0, channels 0-3 |
| 0x0130-0x013C | Port 1, channels 0-3 |
| 0x0140-0x014C | Port 2, channels 0-3 |
| 0x0150-0x015C | Port 3, channels 0-3 |
| 0x0160-0x016C | Port 4, channels 0-3 |

Constraint: `port * 16 + sub_channel * 4 < 0x50` (80 bytes, 20 registers max).
Bound check: `address < DigitalConfiguration_SmartCable_END`.

Smart cables are MSO-only. Max ports: `MsoConstants::MaxSmartCablePortCount`.

Each register has the same format as DigitalDerived (see below).

#### Per-Channel Digital Configuration (0x0170-0x017C)

Address formula: `address = 0x0170 + channel_index * 4` (channels 0-3)

| Address | Name | Description |
|---------|------|-------------|
| 0x0170 | DIGITAL_CH_CONFIG[0] | Channel 0 digital config. |
| 0x0174 | DIGITAL_CH_CONFIG[1] | Channel 1 digital config. |
| 0x0178 | DIGITAL_CH_CONFIG[2] | Channel 2 digital config. |
| 0x017C | DIGITAL_CH_CONFIG[3] | Channel 3 digital config. |

Bound check: `address < DigitalConfiguration_DigitalDerived_END`.

**Digital channel config bit fields** (from InternalSetDigitalConfiguration):
```
bits [19:0]:  PulseWidth (glitch filter width)
bit  20:      Enabled (channel enable)
bits [31:21]: Unused
```

The packed value: `value = (config.mEnabled & 1) << 20 | config.mPulseWidth`

Glitch filter constraints:
- `PulseWidth >= MsoConstants::MinGlitchFilterLength` (5)
- `PulseWidth <= MsoConstants::MaxGlitchFilterLength` (0x3D08C + 5 = 250000)
- `PulseWidth == 5 || (PulseWidth % 10) == 0`

#### Commit / Bank Control (0x01C0-0x01C4)

| Address | Name | Description |
|---------|------|-------------|
| 0x01C0 | COMMIT_BANK_CONTROL | Configuration commit and smart cable channel enable bits. |
| 0x01C4 | DIGITAL_TEST_PATTERN | Digital test pattern configuration. |

**COMMIT_BANK_CONTROL (0x01C0) bit fields** (from SetEnabledSmartCableDigitalChannels):
```
bits [19:0]:  Smart cable channel enable bits (5 ports x 4 channels = 20 bits)
              Bit position = port * 4 + channel
bits [31:20]: Other commit/bank configuration
```

Constraint: `msb < 20` (max 20 channel enable bits).
The function clears bits for the target port, then sets individual channel bits
using SetField<bool> at positions port*4+0..port*4+3.


### Register Address Ranges Summary

```
0x0004-0x0008  SPI / Status / JESD control
0x0020-0x0024  GPIF (FX3 data interface) control
0x0030-0x0034  Test data generation
0x0040         Capture control (mode, downsample)
0x0044-0x0050  Scope configuration IDs (4 registers)
0x0054-0x0068  Analog channel configuration (6 registers)
0x0070-0x009C  Trigger configuration (12 registers)
0x00E0         Post-programming initialization
0x00FC         JESD BER test
0x0104         Digital voltage threshold
0x0120-0x016F  Smart cable digital config (5 ports x 4 ch, MSO only)
0x0170-0x017C  Per-channel digital config (4 channels, stride 4)
0x01C0-0x01C4  Commit/bank control, digital test pattern
```

Total: 39 unique 16-bit register addresses confirmed from MsoFpga code region.


## FpgaDeviceFeatures Register Interface (8-bit address)

### Wire Protocol

- **Address width**: 8 bits
- **Value width**: 8 bits
- **Transport**: USB bulk EP1 OUT with opcode prefix
- **Opcodes**: 0x80 (WriteRegisters), 0x81 (ReadRegisters)

```
WriteRegisters (opcode 0x80):
  EP1 OUT: [0x80] [count] [addr0] [val0] [addr1] [val1] ...
  Max 31 register pairs per batch.
  Fire-and-forget (no response check) for base class.
  ProFpgaDeviceFeatures: adds response verification (reads back confirmation).

ReadRegister (opcode 0x81):
  EP1 OUT: [0x81] [count=1] [addr]
  EP1 IN:  [value]  (1 byte)

ReadRegisters (opcode 0x81, batch):
  EP1 OUT: [0x81] [count] [addr0] [addr1] ...
  EP1 IN:  [val0] [val1] ...  (count bytes)
  Max 63 registers per batch.
```

Source file:
`/workspace/monorepo/graph-io/devices/logic_interface/src/Logic2FpgaDevice.cpp`

### Vtable Layout

```
[0] ~FpgaDeviceFeatures() [D1 complete destructor]
[1] ~FpgaDeviceFeatures() [D0 deleting destructor]
[2] WriteRegister(U8 addr, U8 val)
[3] WriteRegisters(vector<pair<U8,U8>>&, const S8*)
[4] ReadRegister(U8 addr, const S8*) -> U8
[5] ReadRegisters(vector<U8>&, const S8*) -> vector<U8>
[6] DownloadBitstream(const U8*, U64, callback, double)
[7] ResetFpga()
```

ProFpgaDeviceFeatures overrides slots [1] and [3] (adds response verification).

### Non-Virtual Methods

```
WriteRegisterBit(U8 reg, U8 bit, U8 val, const S8*)  -- read-modify-write helper
WriteConfigurationData(const U8*, U32)                 -- EP1 opcode 0x7F (bitstream data)
```

### Complete Register Map

The 8-bit registers are used by Logic2FpgaDevice::SetCaptureParameters and
Logic2FpgaDevice::WriteAd9637Register.

#### ADC SPI Registers (via WriteAd9637Register)

WriteAd9637Register builds a vector of (addr, value) pairs to write ADC SPI
registers through the FPGA's SPI bridge:

| Reg | Name | Description |
|-----|------|-------------|
| 0x03 | ADC_SPI_ADDR | ADC SPI address: `adc_addr[15:8]`. Lower bits in next reg. |
| 0x04 | ADC_SPI_DATA_LO | ADC SPI data low byte: `adc_data[7:0]`. |
| 0x05 | ADC_SPI_DATA_HI | ADC SPI data high bits: `adc_data[15:8]`. |
| 0x16 | ADC_SPI_ADDR_EXT | Extended ADC address: `adc_addr[12:8] | 0x16`. |

The AD9637 is the ADC used in Logic Pro 8/16 (Xilinx-based).
Data constraint: `adc_data < 0x100` (8-bit ADC register values).

#### Capture Configuration Registers (via SetCaptureParameters)

| Reg | Name | Description |
|-----|------|-------------|
| 0x00 | CAPTURE_INIT | Initial register (written as 0). |
| 0x06 | ANALOG_CH_ENABLE_LO | Analog channel enable mask, low byte. |
| 0x07 | ANALOG_CH_ENABLE_HI | Analog channel enable mask, high byte (16ch devices). |
| 0x08 | DIGITAL_CH_ENABLE_LO | Digital channel enable mask, low byte. |
| 0x09 | DIGITAL_CH_ENABLE_HI | Digital channel enable mask, high byte (16ch devices). |
| 0x0A | ANALOG_DECIMATION | Analog decimation factor, low byte. |
| 0x0B | DIGITAL_DIVIDER_LO | Digital sample rate divider, low byte. |
| 0x0C | DIGITAL_DIVIDER_HI | Digital sample rate divider, high byte. |
| 0x0D | MIX_RATIO_A | Analog/digital mix ratio A. |
| 0x0E | MIX_RATIO_B | Analog/digital mix ratio B. |
| 0x13 | CAPTURE_CONTROL | Capture control flags byte. |
| 0x14 | DECIMATION_FACTOR | Computed decimation factor (max 0xFF). |
| 0x1A | ADC_CONFIG | ADC configuration (Pro devices, JESD mode). |
| 0x1B | ANALOG_DEC_UPPER | Upper analog decimation (board revision 1 only). |

**CAPTURE_CONTROL (reg 0x13) bit fields**:
```
bit 0:  Capture enabled (revision 2 = always 1)
bit 1:  Ignore FPGA errors flag
bits [7:2]: Reserved
```

**MIX_RATIO constraints**: `(mix_ratio_a | mix_ratio_b) < 0x100` (8-bit values).

**DECIMATION_FACTOR (reg 0x14)**: Computed as
`min(0xFF, (base_rate / total_channel_rate + padding) / (base_rate / total_channel_rate))`
where padding depends on whether JESD mode is active (40M vs 100M).

**ADC_CONFIG (reg 0x1A)**: Written only when `local_17c != 0` (JESD/Pro mode).
```
Value encoding:
  1 = standard mode
  0 = 4x interleaved mode (local_188 = 1)
  2*N = other interleave (local_168 * 2)
```

**ANALOG_DEC_UPPER (reg 0x1B)**: Written only for board revision 1
(`*(param_1 + 0x21c) == 1`). Upper bit of analog decimation.
Constraint: `analog_decimation_upper <= 1`.

#### PLL and DCM Lock Registers (read via FpgaDeviceFeatures)

| Reg | Name | Description |
|-----|------|-------------|
| 0x40 | PLL_STATUS | PLL/DCM lock status register. |
| 0x4A | PLL_STATUS_2 | Secondary PLL status (read alongside 0x40). |

**PLL_STATUS (reg 0x40) bit fields** (from SetAd9637Enabled):
```
bit 5:  Main DCM locked
bit 6:  Slow DCM locked
```

Both DCMs must lock after ADC enable. If Main DCM does not lock, throws
"Main DCM didn't lock". If Slow DCM does not lock, throws "Slow DCM didn't lock".

The PLL lock check function (FUN_01d22af0) also checks system PLL and PCLK PLL,
and logs: "PLL(s) failed to lock after PLL reset! system PLL: {}, PCLK PLL: {}.
status register: {:#x}"

#### WriteAd9637Register Protocol

The AD9637 ADC is programmed through the FPGA's 8-bit register interface.
Each ADC register write requires 4 register writes in sequence:

```
Step 1: Write reg 0x03 = (adc_addr >> 8) | 0x03    -- ADC address high byte
Step 2: Write reg 0x16 = (adc_addr & 0x1F00) >> 8  -- Extended address bits
Step 3: Write reg 0x04 = adc_data                   -- ADC data
Step 4: Write reg 0x05 = 0                          -- ADC data high (zero for 8-bit)
```

This is different from the MsoFpga SPI bridge (reg 0x0004) which uses a single
32-bit register write. The FpgaDeviceFeatures path uses multiple 8-bit writes
through the 0x80 opcode batch mechanism.


## SPI Bridge Details (MsoFpga Register 0x0004)

**SPI_DATA (0x0004) bit fields** (from WriteSpi decompilation):
```
bits [22:0]:  SPI address + data (packed format depends on SpiAddressDataFormat)
bit  23:      R/W flag (0 = write, 1 = read)
bit  24:      SpiWireConfiguration (0 = 4-wire, 1 = 3-wire)
bits [25]:    Unused
bit  26:      SpiClockEdge (0 = rising, 1 = falling)
bits [31:27]: Must be zero
```

WriteSpi packed value construction:
```
value = data[22:0] | (SpiClockEdge & 1) << 26 | (SpiWireConfig & 1) << 24
```

ReadSpi_7BitAddress: Sets bit 23 (R/W = read), address in bits [22:16].
  Constraint: `(address & 0x80) == 0` (7-bit address).

ReadSpi_15BitAddress: Sets bit 23 (R/W = read), address in bits [22:8].
  Constraint: `(address & 0x8000) == 0` (15-bit address).

SPI modes (from SetField assertions):
- SpiClockEdge: 1 bit (0 or 1)
- SpiWireConfiguration: 1 bit (0 or 1)
- SpiAddressDataFormat: multi-bit (controls address/data bit packing)


## STATUS_DDR_JESD_CONTROL (MsoFpga Register 0x0008) Details

**Bit fields** (from WaitForDdrReady and ResetJesdPhyTransceivers decompilation):
```
bit  3:  DDR calibrated status
bit  4:  DDR locked status

WaitForDdrReady: (value & 0x18) == 0x18  (both bits 3 and 4 set = DDR ready)
```

ResetJesdPhyTransceivers sequence:
1. Asserts `channel_count == 2 || channel_count == 4`
2. Sets mask: `0xFF` for 4 channels, `0x5A` for 2 channels
3. Writes register 0x0008 with value 2 in a loop
4. Reads register 0x0008 to check JESD status
5. Retries with 1ms sleep until lock or timeout (2 seconds)

JESD status fields (from log format):
```
For Xilinx FPGA:
  link_up:     bit 0
  sync_status: byte at bits [15:8]
  realigned:   byte at bits [23:16]  (specific bits extracted)
  aligned:     byte at bits [23:16]  (different bit)
  spll_locked: bit 4
  cpll_locked: bit 5

For Altera FPGA:
  synced:      extracted from bits [23:16] & bits [15:8]
  locked:      computed from above
```


## Initialization Sequence

Post-FPGA-programming register write sequence (MsoFpga):
```
1. Write 0x01C0 (COMMIT_BANK_CONTROL) -- initial bank/commit config
2. Write 0x0040 (CAPTURE_CONTROL) -- initial capture config
3. Write 0x00E0 (INIT_CONFIG) -- init-specific config
4. Write 0x0024 (GPIF_FLUSH_TIMEOUT) -- set GPIF flush timeout
5. Write 0x0020 (GPIF_ENABLE) -- enable GPIF data interface
6. [Lattice devices]: ResetJesdPhyTransceivers (writes reg 0x0008 in loop)
7. [Lattice devices]: WaitForDdrReady (polls reg 0x0008 for bits [4:3])
```


## ADC Types and SPI Interfaces

| ADC | Devices | SPI Path | Init Function |
|-----|---------|----------|---------------|
| AD9637 | Logic Pro 8/16 (Xilinx) | FpgaDeviceFeatures regs 0x03-0x05,0x16 | WriteAd9637Register, SetAd9637Enabled |
| ADS528x | Lattice-based devices | MsoFpga SPI bridge (reg 0x0004) | SetAds528xEnabled, SetAds528xDrive |
| HMCAD1100 | Logic 8 (Xilinx, non-Pro) | FpgaDeviceFeatures | InitHmcad1100 |


## SetField Instantiations

The `Saleae::Utilities::SetField<T,S>(target, value, bit_pos, bit_width)` template
function is used to pack bit fields into registers. All observed instantiations:

| Target Type | Value Type | Used For |
|-------------|-----------|----------|
| unsigned int | DownsampleMode | CAPTURE_CONTROL reg 0x0040, bits [6:5] |
| unsigned int | TriggerType | TRIGGER_CONTROL reg 0x0070, bits [7:4] |
| unsigned int | TriggerMode | TRIGGER_CONTROL reg 0x0070, bits [3:2] |
| unsigned int | SpiClockEdge | SPI_DATA reg 0x0004, bit 26 |
| unsigned int | SpiWireConfiguration | SPI_DATA reg 0x0004, bit 24 |
| unsigned int | SpiAddressDataFormat | SPI_DATA reg 0x0004, bit packing mode |
| unsigned int | unsigned int | TEST_DATA_FREQ_CTL reg 0x0034, frequency field |
| unsigned int | unsigned short | Various 16-bit fields |
| unsigned int | unsigned char | TRIGGER_CONTROL reg 0x0070, sub-fields |
| unsigned int | unsigned long | TRIGGER_HOLDOFF_TIMEOUT_HI reg 0x008C |
| unsigned int | bool | COMMIT_BANK_CONTROL reg 0x01C0, channel enable bits |
| unsigned char | bool | Digital channel InternalSetDigitalConfiguration enable |
| unsigned char | unsigned char | Various byte-level fields |


## Confidence Assessment

| Finding | Confidence | Source |
|---------|-----------|--------|
| MsoFpga register addresses (39 addresses) | **Confirmed** | Direct from 49 movw immediates + decompilation |
| FpgaDeviceFeatures register addresses (14 addresses) | **Confirmed** | Decompilation of SetCaptureParameters |
| DigitalDerived address = 0x170 + ch*4 | **Confirmed** | Decompiled CalculateDigitalConfigurationAddress |
| SmartCable address = 0x120 + port*16 + sub_ch*4 | **Confirmed** | Decompiled CalculateDigitalConfigurationAddress_SmartCable |
| Digital channel config: enabled<<20 | pulseWidth | **Confirmed** | Decompiled InternalSetDigitalConfiguration |
| Trigger register bit layouts | **Confirmed** | Decompiled SetAnalogTriggerConfiguration + SetField assertions |
| SamplesToTicks = (samples+7)>>3 | **Confirmed** | Decompiled trigger holdoff/timeout computation |
| SPI_DATA bit layout (bit 23=R/W, 24=wire, 26=clock) | **Confirmed** | Decompiled WriteSpi with assertion checks |
| TEST_DATA_FREQ_CTL layout | **Confirmed** | Decompiled SetTestDataConfiguration |
| COMMIT_BANK_CONTROL channel enable bits | **Confirmed** | Decompiled SetEnabledSmartCableDigitalChannels |
| WriteAd9637Register multi-step protocol | **Confirmed** | Decompiled WriteAd9637Register |
| SetCaptureParameters register list | **Confirmed** | Decompiled Logic2FpgaDevice::SetCaptureParameters |
| PLL_STATUS bit 5=Main DCM, bit 6=Slow DCM | **Confirmed** | Decompiled SetAd9637Enabled |
| CAPTURE_CONTROL bit 4=mode, [6:5]=downsample | **High** | Decompiled SetAnalogChannelConfiguration, some bit ambiguity |
| Analog channel reg semantics (0x54-0x68) | **High** | Decompiled with struct offset correlation |
| Register name assignments | **Hypothesis** | Inferred from function context, not from binary symbols |


## Resolved Open Questions (from TASK-0031)

1. **Analog channel register mapping (0x0054-0x0068)**: RESOLVED. These are NOT
   per-channel registers. They are per-parameter: sample divider, pre-trigger,
   post-trigger, computed rate, trigger high bits, and samples-per-chunk.

2. **Trigger register field layout (0x0070-0x009C)**: RESOLVED. Full bit-field
   layout extracted from decompiled SetAnalogTriggerConfiguration. TriggerType
   is 4 bits at [7:4], TriggerMode is 2 bits at [3:2], threshold is 16 bits
   at [31:16].

3. **Register 0x01C0 dual use**: RESOLVED. It is genuinely dual-purpose:
   - During init: bank/commit configuration
   - During smart cable config: per-channel enable bits at positions port*4+ch

4. **Smart cable register range**: RESOLVED. Formula is `0x120 + port*16 + ch*4`,
   range 0x0120-0x016F (not 0x0180-0x01BF as previously hypothesized).

5. **Computed register addresses**: RESOLVED. The loop in ResetJesdPhyTransceivers
   always writes value 2 to register 0x0008 (not computed addresses).


## Remaining Open Questions

1. **Register 0x00E0 (INIT_CONFIG)**: What specific value is written during init?
   Need decompilation of the init function to determine.

2. **CAPTURE_CONTROL (0x0040) full bit layout**: The SetAnalogChannelConfiguration
   only modifies bits [6:4]. What are bits [3:0], [7], and [31:8]?

3. **SCOPE_CONFIG_ID registers (0x0044-0x0050)**: What values are written? Need
   decompilation of SetConfigurationIds.

4. **Register 0x0094 (TRIGGER_TICK_SIZE)**: What value is written? This appears to
   be a clock rate configuration for the sample-to-tick conversion.

5. **FpgaDeviceFeatures registers above 0x1B**: Are there undiscovered 8-bit
   registers beyond 0x1B? SetCaptureParameters only covers the known set.

6. **DIGITAL_VOLTAGE_THRESHOLD (0x0104)**: What is the secondary write mechanism?
   The code also writes via "Write 0x{:x} to 0x{:x}" format, possibly I2C DAC.

7. **HMCAD1100 initialization registers**: InitHmcad1100 uses a different register
   set than AD9637. Need decompilation to document HMCAD1100-specific registers.
