# FPGA Register Map Analysis

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0031


## Summary

The host communicates with the FPGA through a register interface tunneled via
the FX3 microcontroller. Registers have **16-bit addresses** and **32-bit
values**, transferred in **big-endian** byte order over USB bulk EP1 OUT/IN.

The register interface is implemented in `Saleae::MsoFpga` (source file:
`/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/component/mso_fpga.cpp`)
and used by `FpgaDeviceFeatures::WriteRegisters` / `ReadRegisters`.


## Extraction Method

1. Located the format strings `"FPGA WriteRegister 0x{:04x} = 0x{:08x} ({})"`
   (at file offset 0x0041fd7b) and `"FPGA ReadRegister 0x{:04X} = 0x{:08X}"`
   (at file offset 0x003e6a79) in the binary.

2. Scanned the .text section for RIP-relative LEA instructions referencing
   these format strings. Found **57 WriteRegister log sites** and **1
   ReadRegister log site**, all in the VA range 0x2d62000-0x2d68000
   (the MsoFpga code region).

3. For each log site, looked backward for `movw $imm16` instructions that
   set up the register address on the stack. Found **49 distinct movw
   instructions** with values < 0x200.

4. Cross-referenced with nearby string references (function labels, assertion
   messages, log messages) to determine what function each register write
   belongs to.

5. Disassembled key functions to understand register address computation
   formulas (e.g., CalculateDigitalConfigurationAddress).


## USB Transport Protocol

The ReadRegister wrapper function (at VA 0x2d62690) reveals the wire protocol:

```
Write (register address to FPGA):
  1. Take 16-bit register address
  2. Byte-swap via ROL $8, %si (big-endian conversion)
  3. Send 2-byte request via EP1 OUT bulk write
  4. Receive 4-byte response via EP1 IN bulk read

Read response:
  1. 4-byte big-endian value returned in response
  2. Byte-swapped back to host endianness
```

The WriteRegister path is similar but sends 6 bytes: 2 bytes address + 4 bytes
value, both big-endian.


## Function Signatures

```cpp
virtual std::vector<U8> FpgaDeviceFeatures::ReadRegisters(
    std::vector<U8> &addrs, const S8 *label)

virtual void FpgaDeviceFeatures::WriteRegisters(
    std::vector<std::pair<U8, U8>> &addr_val_pairs, const S8 *label)

void FpgaDeviceFeatures::WriteRegisterBit(
    U8 reg, U8 bit, U8 val, const S8 *label)
```

Note: Despite the U8 types in the function signatures, the log format shows
16-bit addresses and 32-bit values. The U8 pairs may be serialized into a
larger buffer, or the function signatures may use U8 as a typedef for
something larger. The actual wire protocol uses 2-byte addresses and 4-byte
values based on disassembly of the ReadRegister function.


## Complete Register Map

### SPI / Status / JESD Control (0x0004-0x0008)

| Address | Name | Functions | Description |
|---------|------|-----------|-------------|
| 0x0004 | SPI_DATA | WriteSpi, ReadSpi_7BitAddress, ReadSpi_15BitAddress | SPI data register for ADC communication. Bit 23 is R/W flag; upper 8 bits (24-31) must be zero for writes. |
| 0x0008 | STATUS_DDR_JESD_CONTROL | WaitForDdrReady, ResetJesdPhyTransceivers, StartJesdBerTest, StopJesdBerTest, GetJesdBerTestResults | Multi-purpose status/control: bits [4:3] = DDR locked+calibrated, also used for JESD PHY transceiver reset and BER test control. |

### GPIF Control (0x0020-0x0024)

| Address | Name | Functions | Description |
|---------|------|-----------|-------------|
| 0x0020 | GPIF_ENABLE | EnableGpif | Enables the GPIF (General Programmable Interface) between FX3 and FPGA. |
| 0x0024 | GPIF_FLUSH_TIMEOUT | EnableGpif | GPIF flush timeout in 100MHz clock ticks. Logged as: "Flush timeout set to {}ns (every {} ticks @ 100Mhz)". |

### Test Data Configuration (0x0030-0x0034)

| Address | Name | Functions | Description |
|---------|------|-----------|-------------|
| 0x0030 | TEST_DATA_CONFIG_0 | SetTestDataConfiguration | Test data pattern/object enable and size. Mutual exclusion: cannot have both test pattern and data object enabled. |
| 0x0034 | TEST_DATA_CONFIG_1 | SetTestDataConfiguration | Test data frequency. |

### Capture and Analog Channel Configuration (0x0040-0x0068)

| Address | Name | Functions | Description |
|---------|------|-----------|-------------|
| 0x0040 | CAPTURE_CONTROL | init, SetAnalogChannelConfiguration, DisableAnalogChannels | Main capture control register. Written during post-programming init, analog channel config, and when disabling analog channels. Appears to be the "master" configuration register. |
| 0x0044 | SCOPE_CONFIG_ID_0 | SetConfigurationIds | Configuration ID for scope. Four IDs are logged: "Setting scope configuration IDs: {} {} {} {}". |
| 0x0048 | SCOPE_CONFIG_ID_1 | SetConfigurationIds | Configuration ID 1. |
| 0x004C | SCOPE_CONFIG_ID_2 | SetConfigurationIds | Configuration ID 2. |
| 0x0050 | SCOPE_CONFIG_ID_3 | SetConfigurationIds | Configuration ID 3. |
| 0x0054 | ANALOG_CH_CONFIG_0 | SetAnalogChannelConfiguration | Analog channel configuration registers. Six registers (0x54-0x68) suggest configuration for up to 6 parameters or channel-specific settings. |
| 0x0058 | ANALOG_CH_CONFIG_1 | SetAnalogChannelConfiguration | |
| 0x005C | ANALOG_CH_CONFIG_2 | SetAnalogChannelConfiguration | |
| 0x0060 | ANALOG_CH_CONFIG_3 | SetAnalogChannelConfiguration | |
| 0x0064 | ANALOG_CH_CONFIG_4 | SetAnalogChannelConfiguration | |
| 0x0068 | ANALOG_CH_CONFIG_5 | SetAnalogChannelConfiguration | |

### Trigger Configuration (0x0070-0x009C)

| Address | Name | Functions | Description |
|---------|------|-----------|-------------|
| 0x0070 | TRIGGER_CONTROL | SetAnalogTriggerConfiguration, DisableAnalogTrigger | Trigger enable/disable and mode. Contains packed fields: TriggerType (via SetField), TriggerMode (via SetField). Written twice in SetAnalogTriggerConfiguration (initial setup and final commit). |
| 0x0074 | TRIGGER_CONFIG_0 | SetAnalogTriggerConfiguration | Normal-mode holdoff configuration. Holdoff must be a multiple of 8 samples (warned if not). |
| 0x0078 | TRIGGER_CONFIG_1 | SetAnalogTriggerConfiguration | Normal-mode timeout configuration. Timeout must be a multiple of 8 samples. |
| 0x007C | TRIGGER_CONFIG_2 | SetAnalogTriggerConfiguration | Auto-mode holdoff/timeout configuration. |
| 0x0080 | TRIGGER_HOLDOFF_LO | SetAnalogTriggerConfiguration | Holdoff sample count, low 32 bits. Values are converted from sample counts to FPGA clock ticks via SamplesToTicks. |
| 0x0084 | TRIGGER_HOLDOFF_HI | SetAnalogTriggerConfiguration | Holdoff sample count, high 32 bits. |
| 0x0088 | TRIGGER_TIMEOUT_LO | SetAnalogTriggerConfiguration | Timeout sample count, low 32 bits. |
| 0x008C | TRIGGER_TIMEOUT_HI | SetAnalogTriggerConfiguration | Timeout sample count, high 32 bits. |
| 0x0090 | TRIGGER_CONFIG_3 | SetAnalogTriggerConfiguration | Initial holdoff and threshold configuration. |
| 0x0094 | TRIGGER_TIMESTAMP_SIZE | SamplesToTicks | Timestamp/sample size register. Written near trigger holdoff computation; converts sample-domain values to tick-domain. |
| 0x0098 | SW_TRIGGER_LO | SetSoftwareTrigger | Software trigger timestamp, low 32 bits. Logged: "Setting software trigger at timestamp {}". |
| 0x009C | SW_TRIGGER_HI | SetSoftwareTrigger | Software trigger timestamp, high 32 bits. |

### Initialization (0x00E0)

| Address | Name | Functions | Description |
|---------|------|-----------|-------------|
| 0x00E0 | INIT_CONFIG | CommonDeviceInit (post-FPGA-programming) | Written during post-programming initialization sequence. Part of the init triple: 0x01C0 -> 0x0040 -> 0x00E0. |

### JESD BER Test (0x00FC)

| Address | Name | Functions | Description |
|---------|------|-----------|-------------|
| 0x00FC | JESD_BER_TEST | StartJesdBerTest, StopJesdBerTest | JESD204B Bit Error Rate test control. Separate from the main JESD status in register 0x0008. |

### Digital Voltage Threshold (0x0104)

| Address | Name | Functions | Description |
|---------|------|-----------|-------------|
| 0x0104 | DIGITAL_VOLTAGE_THRESHOLD | SetDigitalDerivedVoltageThresholds | Sets the digital logic voltage threshold for all digital channels. Logic Pro feature. The nearby code also writes via "Write 0x{:x} to 0x{:x}" format (separate from the FPGA register log). |

### Per-Channel Digital Configuration (0x0170-0x017C)

| Address | Name | Functions | Description |
|---------|------|-----------|-------------|
| 0x0170 | DIGITAL_CH_CONFIG[0] | InternalSetDigitalConfiguration | Per-channel digital configuration. Address formula: `0x0170 + channel_index * 4`. Contains glitch filter settings (pulse width must be 5 or multiple of 10). |
| 0x0174 | DIGITAL_CH_CONFIG[1] | InternalSetDigitalConfiguration | |
| 0x0178 | DIGITAL_CH_CONFIG[2] | InternalSetDigitalConfiguration | |
| 0x017C | DIGITAL_CH_CONFIG[3] | InternalSetDigitalConfiguration | |

Bounded by assertion: `address < DigitalConfiguration_DigitalDerived_END`

### Smart Cable Digital Configuration (0x0180+, MSO only)

| Address | Name | Functions | Description |
|---------|------|-----------|-------------|
| 0x0180+ | SMART_CABLE_CH_CONFIG | SetEnabledSmartCableDigitalChannels, CalculateDigitalConfigurationAddress_SmartCable | Per-port, per-channel digital config for smart cable channels. Observed value 0x0194 in the code. |

Bounded by assertion: `address < DigitalConfiguration_SmartCable_END`

Smart cables are MSO-only and out of scope for Logic Pro RE.

### Commit / Bank Control (0x01C0-0x01C4)

| Address | Name | Functions | Description |
|---------|------|-----------|-------------|
| 0x01C0 | COMMIT_BANK_CONTROL | init, InternalSetDigitalConfiguration | Written during init (first register in the init sequence), and after setting digital channel configuration. Likely a configuration commit or PLL/bank enable register. |
| 0x01C4 | DIGITAL_TEST_PATTERN | SetDigitalTestPattern | Digital test pattern configuration. |


## Register Bit Fields (Partially Decoded)

### Register 0x0008 (STATUS_DDR_JESD_CONTROL)

```
Bit 4: DDR locked status
Bit 3: DDR calibrated status

WaitForDdrReady reads register 0x0008 and checks:
  (value & 0x18) == 0x18   (both bits 3 and 4 set = DDR ready)

During JESD init, the register is read, OR'd with 0x10 (bit 4), and written
back. This suggests bit 4 can also be used as a control bit.
```

### Register 0x0004 (SPI_DATA)

```
WriteSpi:
  Assertion: "Upper 8 bits of data cannot be used in WriteSpi"
  Assertion: "( data & 0xFF000000 ) == 0"  -- bits [31:24] must be zero
  Assertion: "R/W cannot be set when writing to SPI"
  Assertion: "( data & ( 1 << 23 ) ) == 0"  -- bit 23 must be zero for write

ReadSpi_7BitAddress:
  Assertion: "Upper bit of address cannot be used"
  Assertion: "( address & 0x80 ) == 0"  -- 7-bit address, bit 7 must be zero

ReadSpi_15BitAddress:
  Assertion: "( address & 0x8000 ) == 0"  -- 15-bit address, bit 15 must be zero

The SPI register packs address + data + control bits:
  Bits [22:16] or [22:8]: SPI address (7-bit or 15-bit)
  Bits [7:0] or [15:0]:   SPI data
  Bit 23:                  R/W flag (1=read, 0=write)
  Bits [31:24]:            Must be zero

Additional packed fields via SetField:
  - SpiClockEdge
  - SpiWireConfiguration
  - SpiAddressDataFormat
```

### Register 0x0070 (TRIGGER_CONTROL)

```
Contains packed bit fields set via Saleae::Utilities::SetField:
  - TriggerType (from Saleae::MsoFpga::TriggerType_impl::TriggerType enum)
  - TriggerMode (from Saleae::MsoFpga::TriggerMode_impl::TriggerMode enum)
  - DownsampleMode (from Saleae::DownsampleMode_impl::DownsampleMode enum)
  - unsigned char fields (generic configuration bits)

Assertions in SetAnalogTriggerConfiguration:
  - mTriggerHysteresisFalling >= mTriggerThreshold
  - mTriggerHysteresisRising <= mTriggerThreshold
  - isfinite(mTriggerThreshold)
```

### Digital Channel Config (0x0170+)

```
InternalSetDigitalConfiguration receives:
  - uint16_t address (computed by CalculateDigitalConfigurationAddress)
  - DigitalChannelConfiguration struct containing:
    - mPulseWidth:  glitch filter pulse width (must be 5 or multiple of 10)
    - mEnabled:     channel enable bit

Additional bits via SetField:
  - bool fields (enable/disable bits)
  - unsigned char fields
```


## Initialization Sequence

Post-FPGA-programming register write sequence:

```
1. Write 0x01C0 (COMMIT_BANK_CONTROL) -- initial bank/commit config
2. Write 0x0040 (CAPTURE_CONTROL) -- initial capture config
3. Write 0x00E0 (INIT_CONFIG) -- init-specific config
4. Write 0x0024 (GPIF_FLUSH_TIMEOUT) -- set GPIF flush timeout
5. Write 0x0020 (GPIF_ENABLE) -- enable GPIF data interface
6. [Lattice devices]: ResetJesdPhyTransceivers
7. [Lattice devices]: WaitForDdrReady (polls register 0x0008)
```


## JESD204B Initialization (Lattice ECP5 devices)

```
ResetJesdPhyTransceivers:
  1. Writes to register 0x0008 (multiple times in a loop)
  2. Reads JESD status: "FPGA JESD lock=0x{:x} synced=0x{:x} error=0x{:x}"
  3. Extended status: "power_good={} init_done={} sync_err={}, lane_0-3_pll={}, lane_4-7_pll={}, smpl_pll={}"
  4. Retries until lock or timeout: "Locked JESD after {} ms, {} retries"
  5. JesdStatus struct: link_up, sync_status, realigned, aligned, spll_locked, cpll_locked

WaitForDdrReady:
  1. Reads register 0x0008 in a polling loop
  2. Checks (value & 0x18) == 0x18 (both DDR locked and calibrated)
  3. Sleeps 1ms between polls
  4. "Locked and calibrated DDR after {} ms"
```


## ADC SPI Interface

The FPGA provides an SPI bridge to the ADC via register 0x0004:

```
ADC Write 0x{:x} = 0x{:x}   -- write to ADC SPI register
ADC Read 0x{:x} = 0x{:x}    -- read from ADC SPI register
ADC Vendor ID: {:04x}        -- read ADC vendor ID register

SPI modes:
  - 7-bit address:  ReadSpi_7BitAddress(addr, clock_edge, wire_config)
  - 15-bit address: ReadSpi_15BitAddress(addr, clock_edge, wire_config)
  - Write:          WriteSpi(data, clock_edge, wire_config)

SPI configuration enums:
  - SpiClockEdge:           rising/falling edge
  - SpiWireConfiguration:   3-wire/4-wire SPI
  - SpiAddressDataFormat:   address+data bit packing
```

ADC types accessed via SPI:
- AD9637: `WriteAd9637Register(U16, U16)` - Xilinx-based Pro devices
- ADS528x: `SetAds528xEnabled`, `SetAds528xDrive` - Lattice-based devices
- HMCAD1100: `InitHmcad1100(Hmcad1100Settings)` - Xilinx-based non-Pro devices


## Confidence Assessment

| Finding | Confidence | Sources |
|---------|-----------|---------|
| Register addresses extracted from movw immediates | **Confirmed** | Direct disassembly of 49 instruction sites |
| Register addresses are 16-bit, values 32-bit | **Confirmed** | ReadRegister function disassembly + format strings |
| Big-endian byte order on wire | **Confirmed** | ROL $8 byte-swap in ReadRegister/WriteRegister code |
| DigitalDerived address = 0x170 + ch*4 | **Confirmed** | LEA instruction in CalculateDigitalConfigurationAddress |
| DDR ready check: (reg 0x08 & 0x18) == 0x18 | **Confirmed** | AND + CMP instruction sequence in WaitForDdrReady |
| SPI register bit layout (bit 23 = R/W) | **Confirmed** | Assertion strings in WriteSpi/ReadSpi |
| Function attributions (which function writes which register) | **High** | String references near each xref site |
| Register name assignments (descriptive names) | **Hypothesis** | Inferred from function context, not from the binary |
| Smart cable register range (0x0180-0x01BF) | **Hypothesis** | Based on DigitalDerived ending at 0x0180 and one observed address 0x0194 |


## Open Questions

1. **Exact analog channel register mapping**: The six registers 0x0054-0x0068
   are written in SetAnalogChannelConfiguration, but are they per-channel
   (one per analog channel) or per-parameter (one per configuration field)?
   Need decompilation of SetAnalogChannelConfiguration to determine.

2. **Trigger register field layout**: Registers 0x0070-0x009C are used for
   trigger configuration with packed bit fields. The exact bit positions for
   TriggerType, TriggerMode, and DownsampleMode need Ghidra analysis of the
   SetField instantiations.

3. **Register 0x00E0 purpose**: Only written during init. What specific
   hardware configuration does it control?

4. **Register 0x01C0 dual use**: Written during both init and digital channel
   config. Is this a genuine dual-purpose register, or does it have different
   bit fields used in different contexts?

5. **JESD register bit layout in 0x0008**: The loop in ResetJesdPhyTransceivers
   writes computed values to register 0x0008 multiple times. Need decompilation
   to understand the full bit field layout and the JESD reset sequence.

6. **Missing registers**: There may be registers only accessed via
   WriteConfigurationData (bulk config writes during SetCaptureParameters)
   rather than individual WriteRegister calls. These would not appear in
   this analysis.

7. **Computed register addresses**: Several WriteRegister calls use register
   addresses computed at runtime (not from immediate constants). These include
   the loop in ResetJesdPhyTransceivers and some SetAnalogTriggerConfiguration
   paths. Full decompilation is needed to resolve these.


## New Leads

- The `SamplesToTicks` function near register 0x0094 converts sample counts to
  FPGA clock ticks. The conversion factor depends on `MsoDeviceConstants::clockRateHz`.
  This function is key to understanding the trigger timing registers.

- The `SetField` utility is a generic bit-field packer that takes (target_register,
  value, bit_position, bit_width). Tracing all SetField instantiations in the
  MsoFpga code would fully decode every register's bit layout.

- The `GetJesdBerTestResults`, `StartJesdBerTest`, and `StopJesdBerTest`
  functions all access registers 0x0008 and 0x00FC. These are diagnostic
  functions that could be useful for validating the JESD link quality.

- Register 0x0104 (DIGITAL_VOLTAGE_THRESHOLD) is written alongside a separate
  "Write 0x{:x} to 0x{:x}" log (different from the FPGA register log). This
  may indicate a second-level write through a different mechanism (possibly
  an I2C DAC for the threshold voltage).
