# FPGA Register Interface

Status: High confidence (static analysis of libgraph_server_shared.so v2.4.40)
Corroboration: Disassembly of 57 WriteRegister and 1 ReadRegister call sites,
format string analysis, assertion string cross-reference, function signature
matching, instruction-level extraction of immediate values
Awaiting: USB capture validation, Ghidra decompilation for bit-field layouts


## Overview

After FPGA bitstream programming, the host communicates with the FPGA via a
register interface tunneled through the Cypress FX3 microcontroller. The
registers control capture configuration, trigger setup, ADC communication,
JESD204B link management, and digital channel settings.


## Wire Protocol

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

### API

```cpp
virtual std::vector<U8> FpgaDeviceFeatures::ReadRegisters(
    std::vector<U8> &addrs, const S8 *label)

virtual void FpgaDeviceFeatures::WriteRegisters(
    std::vector<std::pair<U8, U8>> &addr_val_pairs, const S8 *label)

void FpgaDeviceFeatures::WriteRegisterBit(
    U8 reg, U8 bit, U8 val, const S8 *label)
```


## Register Map

39 register addresses extracted from immediate values in 49 `movw`
instructions within the MsoFpga code region (VA 0x2d62000-0x2d68000).

### System Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0004 | SPI_DATA | SPI bridge to ADC. Bits [22:0] = address+data, bit 23 = R/W, bits [31:24] must be zero for writes. |
| 0x0008 | STATUS_DDR_JESD | DDR ready status (bits [4:3] = locked+calibrated), JESD PHY control, BER test control. |
| 0x0020 | GPIF_ENABLE | Enables GPIF data interface between FX3 and FPGA. |
| 0x0024 | GPIF_FLUSH_TIMEOUT | GPIF flush timeout in 100MHz clock ticks. |
| 0x00E0 | INIT_CONFIG | Post-programming initialization register. |
| 0x00FC | JESD_BER_TEST | JESD204B Bit Error Rate test control. |

### Capture Configuration Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0040 | CAPTURE_CONTROL | Master capture control. Written during init, analog config, and disable. |
| 0x0044 | SCOPE_CONFIG_ID_0 | Scope configuration ID register 0. |
| 0x0048 | SCOPE_CONFIG_ID_1 | Scope configuration ID register 1. |
| 0x004C | SCOPE_CONFIG_ID_2 | Scope configuration ID register 2. |
| 0x0050 | SCOPE_CONFIG_ID_3 | Scope configuration ID register 3. |

### Analog Channel Configuration Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0054 | ANALOG_CH_CONFIG_0 | Analog channel configuration 0. |
| 0x0058 | ANALOG_CH_CONFIG_1 | Analog channel configuration 1. |
| 0x005C | ANALOG_CH_CONFIG_2 | Analog channel configuration 2. |
| 0x0060 | ANALOG_CH_CONFIG_3 | Analog channel configuration 3. |
| 0x0064 | ANALOG_CH_CONFIG_4 | Analog channel configuration 4. |
| 0x0068 | ANALOG_CH_CONFIG_5 | Analog channel configuration 5. |

### Trigger Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0070 | TRIGGER_CONTROL | Trigger enable/mode. Packed fields: TriggerType, TriggerMode, DownsampleMode. |
| 0x0074 | TRIGGER_NORMAL_HOLDOFF | Normal-mode holdoff (must be multiple of 8 samples). |
| 0x0078 | TRIGGER_NORMAL_TIMEOUT | Normal-mode timeout (must be multiple of 8 samples). |
| 0x007C | TRIGGER_AUTO_CONFIG | Auto-mode holdoff/timeout configuration. |
| 0x0080 | TRIGGER_HOLDOFF_LO | Holdoff sample count, low 32 bits (in FPGA ticks). |
| 0x0084 | TRIGGER_HOLDOFF_HI | Holdoff sample count, high 32 bits. |
| 0x0088 | TRIGGER_TIMEOUT_LO | Timeout sample count, low 32 bits (in FPGA ticks). |
| 0x008C | TRIGGER_TIMEOUT_HI | Timeout sample count, high 32 bits. |
| 0x0090 | TRIGGER_INITIAL | Initial holdoff and threshold configuration. |
| 0x0094 | TRIGGER_TICK_SIZE | Sample-to-tick conversion factor. |
| 0x0098 | SW_TRIGGER_LO | Software trigger timestamp, low 32 bits. |
| 0x009C | SW_TRIGGER_HI | Software trigger timestamp, high 32 bits. |

### Test Data Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0030 | TEST_DATA_0 | Test data pattern enable, size. |
| 0x0034 | TEST_DATA_1 | Test data frequency. |

### Digital Channel Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x0104 | DIGITAL_THRESHOLD | Digital logic voltage threshold. |
| 0x0170 | DIGITAL_CH_CONFIG[0] | Channel 0 digital config (glitch filter, enable). |
| 0x0174 | DIGITAL_CH_CONFIG[1] | Channel 1 digital config. |
| 0x0178 | DIGITAL_CH_CONFIG[2] | Channel 2 digital config. |
| 0x017C | DIGITAL_CH_CONFIG[3] | Channel 3 digital config. |

Address formula: `address = 0x0170 + channel_index * 4` (channels 0-3).

### Control Registers

| Address | Name | Description |
|---------|------|-------------|
| 0x01C0 | COMMIT_CONTROL | Configuration commit / bank control. Written during init and after digital config changes. |
| 0x01C4 | DIGITAL_TEST_PATTERN | Digital test pattern configuration. |


## Post-Programming Initialization Sequence

After FPGA bitstream upload completes, the host performs:

```
1. Write reg 0x01C0  (COMMIT_CONTROL)
2. Write reg 0x0040  (CAPTURE_CONTROL)
3. Write reg 0x00E0  (INIT_CONFIG)
4. Write reg 0x0024  (GPIF_FLUSH_TIMEOUT)
5. Write reg 0x0020  (GPIF_ENABLE)
6. [Lattice ECP5]: ResetJesdPhyTransceivers  (writes reg 0x0008 in loop)
7. [Lattice ECP5]: WaitForDdrReady           (polls reg 0x0008 for bits [4:3])
```


## DDR Ready Check

```
Read register 0x0008:
  DDR_LOCKED      = bit 4
  DDR_CALIBRATED  = bit 3
  DDR_READY       = (value & 0x18) == 0x18

Polling loop with 1ms sleep, logged as:
  "Locked and calibrated DDR after {} ms"
```


## SPI Bridge (Register 0x0004)

The FPGA provides an SPI master to communicate with the on-board ADC:

```
WriteSpi format:
  Bits [22:0]:  SPI address + data (format depends on SpiAddressDataFormat)
  Bit  23:      R/W flag (must be 0 for write)
  Bits [31:24]: Must be zero

ReadSpi_7BitAddress:
  7-bit SPI address (bit 7 must be zero)

ReadSpi_15BitAddress:
  15-bit SPI address (bit 15 must be zero)

Configuration via SetField:
  - SpiClockEdge:        clock polarity
  - SpiWireConfiguration: 3-wire or 4-wire SPI
  - SpiAddressDataFormat: how address and data bits are packed
```


## Digital Channel Configuration

Per-channel configuration at addresses 0x0170-0x017C includes:

- Glitch filter: pulse width must be 5 or a multiple of 10
- Channel enable bits
- Other configuration (exact bit layout needs decompilation)

Address computation:
```cpp
uint16_t CalculateDigitalConfigurationAddress_DigitalDerived(int channel) {
    assert(channel < 4);
    return 0x0170 + channel * 4;
}
```


## Register Address Ranges

```
0x0004-0x0008  SPI / Status / JESD control
0x0020-0x0024  GPIF (FX3 data interface) control
0x0030-0x0034  Test data generation
0x0040-0x0068  Capture control, scope config IDs, analog channel config
0x0070-0x009C  Trigger configuration and software trigger
0x00E0         Post-programming initialization
0x00FC         JESD BER test
0x0104         Digital voltage threshold
0x0170-0x017C  Per-channel digital config (4 channels, stride 4)
0x01C0-0x01C4  Commit/bank control, digital test pattern
```
