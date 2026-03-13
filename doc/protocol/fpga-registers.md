# FPGA Register Interface

Status: High confidence (static analysis of libgraph_server_shared.so v2.4.40)
Corroboration: Ghidra decompilation of 59 functions, format string analysis,
assertion string cross-reference, function signature matching, SetField template
instantiation tracing
Awaiting: USB capture validation for register value verification


## Overview

After FPGA bitstream programming, the host communicates with the FPGA via a
register interface tunneled through the Cypress FX3 microcontroller.

Two distinct register interfaces exist:

1. **MsoFpga**: 16-bit addresses, 32-bit values. Direct 6-byte USB bulk
   transfers. Used by MSO/oscilloscope device path. 39 register addresses.

2. **FpgaDeviceFeatures**: 8-bit addresses, 8-bit values. Wrapped in EP1
   opcode commands (0x80 write, 0x81 read). Used by Logic 4/8 and Logic Pro
   8/16 device path. 21 register addresses.


## MsoFpga Wire Protocol

- **Address width**: 16 bits
- **Value width**: 32 bits
- **Byte order**: Big-endian on wire (byte-swapped via x86 ROL instruction)
- **Transport**: USB bulk transfers on EP1 OUT (write) and EP1 IN (read)

### Register Read

```
Host -> Device (EP1 OUT):  [addr_hi] [addr_lo]          (2 bytes)
Device -> Host (EP1 IN):   [val_3] [val_2] [val_1] [val_0]  (4 bytes, big-endian)
```

### Register Write

```
Host -> Device (EP1 OUT):  [addr_hi] [addr_lo] [val_3] [val_2] [val_1] [val_0]  (6 bytes)
```

### Logging Format

```
FPGA WriteRegister 0x{:04x} = 0x{:08x} ({label})
FPGA ReadRegister 0x{:04X} = 0x{:08X}
```


## FpgaDeviceFeatures Wire Protocol

- **Address width**: 8 bits
- **Value width**: 8 bits
- **Transport**: USB bulk EP1 OUT with opcode prefix

### Register Write (Opcode 0x80)

```
Host -> Device (EP1 OUT):  [0x80] [count] [addr0] [val0] [addr1] [val1] ...
Max 31 register pairs per batch.
Base class: fire-and-forget (no response check).
ProFpgaDeviceFeatures: reads back confirmation byte after write.
```

### Register Read (Opcode 0x81)

```
Host -> Device (EP1 OUT):  [0x81] [count] [addr0] [addr1] ...
Device -> Host (EP1 IN):   [val0] [val1] ...  (count bytes)
Max 63 registers per batch.
```

### API

```cpp
// Virtual methods (vtable slots [2]-[5])
virtual void WriteRegister(U8 addr, U8 val);
virtual void WriteRegisters(std::vector<std::pair<U8, U8>> &pairs, const S8 *label);
virtual U8 ReadRegister(U8 addr, const S8 *label);
virtual std::vector<U8> ReadRegisters(std::vector<U8> &addrs, const S8 *label);

// Non-virtual
void WriteRegisterBit(U8 reg, U8 bit, U8 val, const S8 *label);
void WriteConfigurationData(const U8 *data, U32 len);  // opcode 0x7F
```


## MsoFpga Register Map

### System Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0004 | SPI_DATA | SPI bridge to ADC. Bits [22:0] = address+data, bit 23 = R/W, bit 24 = SpiWireConfig, bit 26 = SpiClockEdge. |
| 0x0008 | STATUS_DDR_JESD | DDR ready (bits [4:3] = locked+calibrated), JESD PHY control. |
| 0x0020 | GPIF_ENABLE | Enables GPIF data interface between FX3 and FPGA. |
| 0x0024 | GPIF_FLUSH_TIMEOUT | GPIF flush timeout in 100MHz clock ticks. |
| 0x00E0 | INIT_CONFIG | Post-programming initialization register. |
| 0x00FC | JESD_BER_TEST | JESD204B Bit Error Rate test control. |

### Capture Configuration Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0040 | CAPTURE_CONTROL | Master capture control. Bit 4 = capture mode, bits [6:5] = DownsampleMode. |
| 0x0044 | SCOPE_CONFIG_ID_0 | Scope configuration ID register 0. |
| 0x0048 | SCOPE_CONFIG_ID_1 | Scope configuration ID register 1. |
| 0x004C | SCOPE_CONFIG_ID_2 | Scope configuration ID register 2. |
| 0x0050 | SCOPE_CONFIG_ID_3 | Scope configuration ID register 3. |

### Analog Channel Configuration Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0054 | ANALOG_SAMPLE_DIVIDER | Sample rate divider from config struct. |
| 0x0058 | ANALOG_PRE_TRIGGER | Negated pre-trigger length. |
| 0x005C | ANALOG_POST_TRIGGER | Post-trigger length. |
| 0x0060 | ANALOG_COMPUTED_RATE | Computed: `round(clock_rate / divider)`. |
| 0x0064 | ANALOG_TRIGGER_HI | Upper bits: `(pre_hi & 0xFFFF) | (post_hi & 0xFFFF) << 16`. |
| 0x0068 | ANALOG_SAMPLES_PER_CHUNK | Samples per chunk from config struct. |

FPGA variant constraints:
- Xilinx: SamplesPerChunk 0x100-0x1E00, MaxTriggerBuffer 0x3FFFC000
- Altera: SamplesPerChunk 0x40-0x1780, MaxTriggerBuffer 0x2FFFC000

### Trigger Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0070 | TRIGGER_CONTROL | Packed: threshold[31:16], channelSource[15:8], TriggerType[7:4], TriggerMode[3:2], glitchFilterEnabled[1:0]. |
| 0x0074 | TRIGGER_NORMAL_HOLDOFF | Normal-mode holdoff in ticks: `(samples+7) >> 3`. |
| 0x0078 | TRIGGER_NORMAL_TIMEOUT | Normal-mode timeout in ticks. |
| 0x007C | TRIGGER_HYSTERESIS | Hysteresis: `rising[15:0] | falling[31:16]`. |
| 0x0080 | TRIGGER_INITIAL_HOLDOFF_LO | Initial holdoff, low 32 bits (in ticks). |
| 0x0084 | TRIGGER_AUTO_HOLDOFF | Auto-mode holdoff in ticks. |
| 0x0088 | TRIGGER_AUTO_TIMEOUT | Auto-mode timeout in ticks. |
| 0x008C | TRIGGER_HOLDOFF_TIMEOUT_HI | Packed: `timeout_hi[11:0] << 16 | holdoff_hi[11:0]`. |
| 0x0090 | TRIGGER_INITIAL_HOLDOFF_HI | Initial holdoff high: 13 bits. |
| 0x0094 | TRIGGER_TICK_SIZE | Sample-to-tick conversion factor. |
| 0x0098 | SW_TRIGGER_LO | Software trigger timestamp, low 32 bits. |
| 0x009C | SW_TRIGGER_HI | Software trigger timestamp, high 32 bits. |

SamplesToTicks conversion: `ticks = (samples + 7) >> 3` (samples/8, rounded up).
Holdoff/timeout values must be multiples of 8 samples.

### Test Data Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0030 | TEST_DATA_SIZE | Test data size value. |
| 0x0034 | TEST_DATA_FREQ_CTL | `mDataObjectEnabled<<20 | mTestPatternEnabled<<21 | freq[19:0]`. |

### Digital Channel Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0104 | DIGITAL_THRESHOLD | Digital logic voltage threshold. |
| 0x0170 | DIGITAL_CH_CONFIG[0] | Channel 0: `enabled<<20 | pulseWidth[19:0]`. |
| 0x0174 | DIGITAL_CH_CONFIG[1] | Channel 1 digital config. |
| 0x0178 | DIGITAL_CH_CONFIG[2] | Channel 2 digital config. |
| 0x017C | DIGITAL_CH_CONFIG[3] | Channel 3 digital config. |

Address formula: `address = 0x0170 + channel_index * 4` (channels 0-3).
Glitch filter: pulse width must be 5 or a multiple of 10 (range 5 to ~250000).

### Smart Cable Digital Registers (MSO only)

Address formula: `address = 0x0120 + port * 16 + sub_channel * 4`

| Address Range | Port | Description |
|---------------|------|-------------|
| 0x0120-0x012C | 0 | Port 0, channels 0-3 |
| 0x0130-0x013C | 1 | Port 1, channels 0-3 |
| 0x0140-0x014C | 2 | Port 2, channels 0-3 |
| 0x0150-0x015C | 3 | Port 3, channels 0-3 |
| 0x0160-0x016C | 4 | Port 4, channels 0-3 |

Same bit layout as DIGITAL_CH_CONFIG: `enabled<<20 | pulseWidth[19:0]`.

### Scope Channel Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0194 | SCOPE_CHANNEL_CONFIG | Scope channel configuration. Written by function at ELF 0x02d662c0 with a single uint32 parameter. Exact bit fields unknown -- possibly scope-mode channel select or test pattern config. See `re/notes/fpga-glitch-filter.md`. |

### Control Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x01C0 | COMMIT_CONTROL | Config commit and smart cable enable bits[19:0]. |
| 0x01C4 | DIGITAL_TEST_PATTERN | Digital test pattern configuration. |


## FpgaDeviceFeatures Register Map

### Capture Configuration (SetCaptureParameters)

| Reg | Name | Description |
|-----|------|-------------|
| 0x00 | CAPTURE_INIT | Initial register (value 0). |
| 0x06 | ANALOG_CH_ENABLE_LO | Analog channel enable mask, low byte. |
| 0x07 | ANALOG_CH_ENABLE_HI | Analog channel enable mask, high byte (16ch). |
| 0x08 | DIGITAL_CH_ENABLE_LO | Digital channel enable mask, low byte. |
| 0x09 | DIGITAL_CH_ENABLE_HI | Digital channel enable mask, high byte (16ch). |
| 0x0A | ANALOG_DECIMATION | Analog decimation factor. |
| 0x0B | DIGITAL_DIVIDER_LO | Digital sample rate divider, low byte. |
| 0x0C | DIGITAL_DIVIDER_HI | Digital sample rate divider, high byte. |
| 0x0D | MIX_RATIO_A | Analog/digital mix ratio A. |
| 0x0E | MIX_RATIO_B | Analog/digital mix ratio B. |
| 0x13 | CAPTURE_CONTROL | bit 0=enabled, bit 1=ignore errors. |
| 0x14 | DECIMATION_FACTOR | Computed decimation factor (max 0xFF). |
| 0x1A | ADC_CONFIG | ADC configuration (Pro/JESD devices only). |
| 0x1B | ANALOG_DEC_UPPER | Upper analog decimation (board rev 1 only). |

### ADC SPI Bridge (WriteAd9637Register)

| Reg | Name | Description |
|-----|------|-------------|
| 0x03 | ADC_SPI_ADDR | ADC SPI address high byte. |
| 0x04 | ADC_SPI_DATA_LO | ADC SPI data low byte. |
| 0x05 | ADC_SPI_DATA_HI | ADC SPI data high byte. |
| 0x16 | ADC_SPI_ADDR_EXT | Extended ADC address bits. |

### PLL Status (read-only)

| Reg | Name | Description |
|-----|------|-------------|
| 0x40 | PLL_STATUS | bit 5=Main DCM locked, bit 6=Slow DCM locked. |
| 0x4A | PLL_STATUS_2 | Secondary PLL status. |


## Post-Programming Initialization Sequence

After FPGA bitstream upload completes:

```
1. Write reg 0x01C0  (COMMIT_CONTROL)
2. Write reg 0x0040  (CAPTURE_CONTROL)
3. Write reg 0x00E0  (INIT_CONFIG)
4. Write reg 0x0024  (GPIF_FLUSH_TIMEOUT)
5. Write reg 0x0020  (GPIF_ENABLE)
6. [Lattice ECP5]: ResetJesdPhyTransceivers  (writes reg 0x0008)
7. [Lattice ECP5]: WaitForDdrReady           (polls reg 0x0008 bits [4:3])
```


## DDR Ready Check

```
Read register 0x0008:
  DDR_LOCKED      = bit 4
  DDR_CALIBRATED  = bit 3
  DDR_READY       = (value & 0x18) == 0x18
```


## Register Address Ranges

```
MsoFpga (16-bit addresses, 32-bit values):
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
  0x0120-0x016F  Smart cable digital config (5 ports x 4 channels)
  0x0170-0x017C  Per-channel digital config (4 channels, stride 4)
  0x0194         Scope channel configuration
  0x01C0-0x01C4  Commit/bank control, digital test pattern

FpgaDeviceFeatures (8-bit addresses, 8-bit values):
  0x00           Capture init
  0x03-0x05      ADC SPI bridge (address, data)
  0x06-0x09      Channel enable masks (analog + digital, lo + hi)
  0x0A           Analog decimation
  0x0B-0x0C      Digital sample rate divider (lo + hi)
  0x0D-0x0E      Analog/digital mix ratios
  0x13           Capture control flags
  0x14           Decimation factor
  0x16           ADC SPI extended address
  0x1A           ADC configuration (Pro/JESD only)
  0x1B           Upper analog decimation (board rev 1 only)
  0x40           PLL/DCM lock status
  0x4A           Secondary PLL status
```
