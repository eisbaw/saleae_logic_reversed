# ADC Initialization -- HMCAD1100, AD9637, and ADS528x Register Maps

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Method: String extraction from binary, cross-referenced with public ADC
datasheets (HMCAD1100 SBAS547C, AD9637 data sheet)


## Summary

Three ADC families are used across Saleae Logic devices. The ADC type is
determined at runtime by board revision, not by device model. All ADC
register access goes through the FPGA SPI bridge at FPGA register 0x0004.

| ADC | Bit Depth | Channels | Devices (Xilinx Rev) | Interface | SPI Addr Width |
|-----|-----------|----------|----------------------|-----------|----------------|
| HMCAD1100 | 8-bit | Quad (1/2/4ch modes) | Logic 4, Logic 8 | Parallel LVDS | 7-bit |
| AD9637 | 12-bit | Dual | Logic Pro 8, Logic Pro 16 | Parallel LVDS | 15-bit (13-bit addr) |
| ADS5282/ADS5287 | 12-bit | Octal | All (Lattice ECP5 Rev) | JESD204B serial | N/A (JESD) |


## ADC Selection Logic

The ADC type is stored in the `RevisionDetail` struct, which is parsed
from the board revision data:

```
RevisionDetail {
    AdcType: enum { HMCAD1100, AD9637 }
    UsesNewCryptoInterface: bool
    // ... other fields
}
```

String evidence:
- `"mBoardRevision.mAdcType == RevisionDetail::AdcType::AD9637"` (assertion)
- `"Unsupported board revision for Ad9637"` (error)
- `"ParseBoardRevision"` (function name)
- `"bool RevisionDetail::UsesNewCryptoInterface()"` (assertion)

For Lattice ECP5 hardware revisions, the ADC is always ADS528x -- the
FPGA bitstream itself determines the ADC interface (all Lattice bitstreams
contain "ADS528x" in their design name).

### Selection Flow

```
ParseBoardRevision
  -> RevisionDetail.mAdcType
     -> HMCAD1100: calls InitHmcad1100(Hmcad1100Settings)
     -> AD9637:    calls SetAd9637Enabled(true), then WriteAd9637Register() sequence
     -> ADS528x:   implicit in Lattice ECP5 bitstream; calls SetAds528xEnabled, JESD init
```


## FPGA SPI Bridge (Register 0x0004)

All ADC register access for Xilinx FPGA devices goes through a single
FPGA register that implements an SPI master:

**FPGA Register**: 0x0004 (SPI_DATA)

**Write format** (FPGA register write = SPI transaction to ADC):
```
Bits [22:0]:   SPI address + data (format depends on SpiAddressDataFormat)
Bit  23:       R/W flag (must be 0 for writes)
Bits [31:24]:  Must be zero
```

**SPI Configuration** (set via SetField before SPI transactions):
- `SpiClockEdge`: Clock polarity (rising/falling edge)
- `SpiWireConfiguration`: 3-wire or 4-wire SPI
- `SpiAddressDataFormat`: How address and data bits are packed in [22:0]

**Read functions**:
- `MsoFpga::ReadSpi_7BitAddress(uint8_t addr, SpiClockEdge, SpiWireConfig)`
  - Returns `uint16_t` -- used for HMCAD1100 (7-bit register addresses)
  - Assertion: bit 7 of address must be zero
- `MsoFpga::ReadSpi_15BitAddress(uint16_t addr, SpiClockEdge, SpiWireConfig)`
  - Returns `uint8_t` -- used for AD9637 (13-bit register addresses within 15-bit field)
  - Assertion: bit 15 of address must be zero

**Write function**:
- `MsoFpga::WriteSpi(uint32_t data, SpiClockEdge, SpiWireConfig)`
  - Writes the 32-bit value to FPGA register 0x0004
  - Error: `"Upper 8 bits of data cannot be used in WriteSpi"`
  - The caller constructs the 32-bit value with address + data packed per SpiAddressDataFormat

**Logging**:
- `"ADC Read 0x{:x} = 0x{:x}"` -- logs register address and read value
- `"ADC Write 0x{:x} = 0x{:x}"` -- logs register address and written value

String offsets (file offsets in binary):
- "ADC Read 0x{:x} = 0x{:x}" at 0x418078
- "ADC Write 0x{:x} = 0x{:x}" at 0x43acc1


## HMCAD1100 (Logic 4 / Logic 8, Xilinx Revisions)

### Overview

The HMCAD1100 is an Analog Devices (formerly Hittite Microwave) high-speed
8-bit ADC with configurable channel modes. It uses a 7-bit SPI register
address space.

- **Part**: HMCAD1100 (TI/Hittite)
- **Resolution**: 8 bits
- **Modes**: 1-channel (1 GSPS), 2-channel (500 MSPS), 4-channel (250 MSPS)
- **SPI**: 7-bit address, 16-bit data
- **Datasheet**: SBAS547C (Texas Instruments)
- **Binary reference**: `"/hittiteAdcGainRegisterValue"` (Hittite is the pre-acquisition brand)

### Initialization Function

```cpp
void Logic2FpgaDevice::InitHmcad1100(Hmcad1100Settings)
```

String offset: 0x363003 (assertion function name)
Source file: `/workspace/monorepo/graph-io/devices/logic_interface/src/Logic2FpgaDevice.cpp`

This function performs the full HMCAD1100 initialization sequence by
writing a series of SPI registers through the FPGA SPI bridge (register
0x0004). The settings struct is populated by `Hmcad1100Settings::SetDefault(DeviceType)`.

### Per-Device Defaults

```cpp
void Hmcad1100Settings::SetDefault(::DeviceType)
```

String offset: 0x38291d (assertion function name)

This function sets HMCAD1100 register values appropriate for the specific
device type (Logic 4 vs Logic 8). The differences likely include:
- Channel mode (Logic 4 uses 4-channel mode, Logic 8 may use 4 or 8 channel)
- Gain settings
- Clock configuration

### HMCAD1100 Register Map (from datasheet SBAS547C)

The HMCAD1100 uses 7-bit SPI register addresses with 16-bit data words.
Registers accessed through `ReadSpi_7BitAddress`:

| Addr | Name | Description | Relevant to Saleae |
|------|------|-------------|--------------------|
| 0x00 | CHIP_ID | Read-only, returns 0x45 for HMCAD1100 | Checked during init |
| 0x03 | POWER_CTRL | Standby, power-down modes | Set during enable/disable |
| 0x0F | SOFT_RESET | Write 0x01 to reset all registers | First step of init |
| 0x11 | LVDS_OUTPUT_MODE | Output data format (offset binary, twos complement) | Set for FPGA interface |
| 0x12 | LVDS_OUTPUT_PHASE | Output clock phase | Set for FPGA timing |
| 0x24 | INPUT_1CH | 1-channel mode input mux selection | Mode-dependent |
| 0x25 | INPUT_2CH | 2-channel mode input mux selection | Mode-dependent |
| 0x26 | INPUT_4CH | 4-channel mode input mux selection | Mode-dependent |
| 0x27 | CHANNEL_EN | Per-channel enable bits | Set based on active channels |
| 0x2A | COARSE_GAIN_CH1 | Coarse gain, channel 1 | Per-channel calibration |
| 0x2B | COARSE_GAIN_CH2 | Coarse gain, channel 2 | Per-channel calibration |
| 0x2C | COARSE_GAIN_CH3 | Coarse gain, channel 3 | Per-channel calibration |
| 0x2D | COARSE_GAIN_CH4 | Coarse gain, channel 4 | Per-channel calibration |
| 0x30 | FINE_GAIN_CH1 | Fine gain adjust, channel 1 | Calibration-derived |
| 0x31 | FINE_GAIN_CH2 | Fine gain adjust, channel 2 | Calibration-derived |
| 0x32 | FINE_GAIN_CH3 | Fine gain adjust, channel 3 | Calibration-derived |
| 0x33 | FINE_GAIN_CH4 | Fine gain adjust, channel 4 | Calibration-derived |
| 0x34 | CHANNEL_MODE | 1ch/2ch/4ch selection | Set based on active channels |
| 0x3A | TEST_PATTERN | Test pattern mode control | Used by SetTestPattenHmcad1100 |
| 0x3B | TEST_PAT_LSB | Test pattern value, low byte | Test pattern |
| 0x3C | TEST_PAT_MSB | Test pattern value, high byte | Test pattern |
| 0x42 | CLK_DIVIDE | Clock divider ratio | Sample rate dependent |
| 0x45 | FULL_SCALE | Full-scale adjust | Gain calibration |
| 0x50 | JITTER_CTRL | Jitter control | Performance tuning |
| 0x51-0x53 | CALIBRATION | Calibration registers | Foreground calibration |
| 0x55 | RESOLUTION | Resolution select | 8-bit mode |

### HMCAD1100 Initialization Sequence (Hypothesis)

Based on the datasheet and the binary evidence, the init sequence likely
proceeds as follows:

```
1. Soft reset: write 0x01 to reg 0x0F
2. Wait for reset complete
3. Set channel mode: write mode to reg 0x34
   - Logic 4: 4-channel mode (4 analog inputs)
   - Logic 8: 4-channel mode (8 inputs via external mux or 4 analog)
4. Set input mux: write to reg 0x24/0x25/0x26 (mode-dependent)
5. Set channel enables: write to reg 0x27
6. Set LVDS output mode: write to reg 0x11 (data format for FPGA)
7. Set LVDS output phase: write to reg 0x12 (timing alignment)
8. Set coarse gains: write to regs 0x2A-0x2D (from calibration data)
9. Set fine gains: write to regs 0x30-0x33 (from calibration data)
10. Set full-scale adjust: write to reg 0x45 (from /hittiteAdcGainRegisterValue)
11. Start foreground calibration: write to regs 0x51-0x53
12. Wait for calibration: "Waiting for ADC calibration to finish"
13. Verify calibration: check for "ADC failed to finish foreground calibration"
```

### HMCAD1100 Test Pattern

```cpp
void Logic2FpgaDevice::SetTestPattenHmcad1100(Hmcad1100Settings::AdcTestPattern)
```

String offset: 0x3be1a8

The test pattern mode writes to register 0x3A to enable/disable test
patterns. Available test pattern types from the `AdcTestPattern` enum:
- Normal operation (test pattern disabled)
- Various fixed patterns (for ADC-FPGA interface validation)

Error string: `"Invalid adc pattern mode"` -- indicates an enum range check.

Environment variable: `SALEAE_ANALOG_TEST_PATTERN` -- when set, enables
ADC test pattern mode for diagnostics.

### HMCAD1100 Data Validation

The binary validates that ADC data fits within 8 bits:
- `"invalid ADC data, exceedes 8 bits."` (sic -- typo in binary)
- This check occurs in the data pipeline after ADC samples are received

Despite being an 8-bit ADC, the data is stored in `int16_t` containers.
The JS bundle uses `{min: -2048, max: 2047}` for Logic 8, suggesting
the native code normalizes 8-bit values to 12-bit range (left-shift by 4).


## AD9637 (Logic Pro 8 / Logic Pro 16, Xilinx Revisions)

### Overview

The AD9637 is an Analog Devices dual 12-bit, 40/80 MSPS ADC with LVDS
output. It uses a 13-bit SPI register address space.

- **Part**: AD9637 (Analog Devices)
- **Resolution**: 12 bits
- **Channels**: 2 per chip (multiple chips per Pro device)
- **Max sample rate**: 80 MSPS
- **SPI**: 13-bit address (within 15-bit field), 8-bit data
- **Interface**: LVDS to FPGA

### Initialization Functions

```cpp
void Logic2FpgaDevice::SetAd9637Enabled(bool)
void Logic2FpgaDevice::WriteAd9637Register(U16 addr, U16 data)
```

String offsets:
- SetAd9637Enabled: 0x3b09b1
- WriteAd9637Register: 0x42772d

**Preconditions**:
- `mBoardRevision.mAdcType == RevisionDetail::AdcType::AD9637` (asserted)
- `"Unsupported board revision for Ad9637"` (error if wrong revision)

### AD9637 Register Map (from datasheet)

The AD9637 uses 13-bit SPI register addresses with 8-bit data. Register
access uses `ReadSpi_15BitAddress` (15-bit field with 13 address bits).

| Addr | Name | Description | Relevant to Saleae |
|------|------|-------------|--------------------|
| 0x0000 | SPI_PORT_CFG | SPI configuration (MSB/LSB first, soft reset) | Init sequence |
| 0x0001 | CHIP_ID | Read-only, should return 0x49 | Vendor ID check |
| 0x0002 | CHIP_GRADE | Speed grade | Device identification |
| 0x0005 | DEVICE_INDEX | Channel A/B selection for config writes | Multi-channel config |
| 0x0008 | POWER_MODES | Normal, standby, power-down, reset | Enable/disable |
| 0x0009 | GLOBAL_CLOCK | Duty cycle stabilizer, clock divide | Sample rate config |
| 0x000B | CLK_DIVIDE | Clock divider ratio | Sample rate config |
| 0x000D | TEST_MODE | Output test patterns | ADC test pattern |
| 0x000E | BIST | Built-in self-test | Diagnostics |
| 0x0010 | OFFSET_ADJUST | Offset adjustment | Calibration |
| 0x0014 | OUTPUT_MODE | CMOS/LVDS, data format, invert | FPGA interface |
| 0x0015 | OUTPUT_ADJUST | Drive strength, output coding | FPGA interface |
| 0x0016 | OUTPUT_PHASE | Clock-data phase relationship | FPGA timing |
| 0x0017 | OUTPUT_DELAY | Output delay | FPGA timing |
| 0x0018 | VREF | Internal voltage reference | ADC accuracy |
| 0x0019-0x001C | USER_PATTERNS | User test pattern values | Test mode |
| 0x0021 | SERIAL_CTRL | Serial port control | SPI config |
| 0x0022 | SERIAL_STATUS | Serial port status | Diagnostics |
| 0x0100 | SAMPLE_RATE | Sample rate register | Rate config |

### AD9637 Vendor ID Check

The binary includes ADC vendor ID verification:
- `"ADC Vendor ID: {:04x}"` -- logs the vendor ID read from chip
- `"Invalid ADC vendor id: {}{}"` -- error when vendor ID mismatch
- The vendor ID is read from register 0x0001 (CHIP_ID)
- Expected value: 0x49 for AD9637

### AD9637 Initialization Sequence (Hypothesis)

```
1. Assert board revision has AdcType::AD9637
2. SetAd9637Enabled(true)
3. Soft reset: write to reg 0x0000 (bit 5 = 1 for reset)
4. Read Chip ID: read reg 0x0001, verify = 0x49
5. Log: "ADC Vendor ID: {:04x}"
6. Set device index: write reg 0x0005 (select channel A, B, or both)
7. Set output mode: write reg 0x0014 (LVDS, twos complement)
8. Set output drive: write reg 0x0015
9. Set output phase: write reg 0x0016
10. Set clock config: write reg 0x0009, 0x000B
11. Set VREF: write reg 0x0018
12. Start foreground calibration
13. Wait: "Waiting for ADC calibration to finish"
14. Verify: check for "ADC failed to finish foreground calibration"
```

### AD9637 ADC MISO Error

The binary contains an error about SPI read failures:
- `". Likely a general failure in reading from ADC MISO."` at offset 0x3e6a9f
- This indicates the SPI interface between FPGA and AD9637 can fail
  (loose connection, power issue, etc.)


## ADS528x (All Devices, Lattice ECP5 Revisions)

### Overview

The ADS5282/ADS5287 is a Texas Instruments octal 12-bit pipeline ADC
with JESD204B serial output interface.

- **Part**: ADS5282 or ADS5287 (TI)
- **Resolution**: 12 bits
- **Channels**: 8
- **Interface**: JESD204B serial (via Lattice ECP5 SERDES)
- **No SPI register access** -- configuration is handled differently
  via FPGA-side JESD link management

### Initialization Functions

```
SetAds528xEnabled  -- enable the ADC
SetAds528xDrive    -- configure drive strength
GetAdcRegisterConfiguration(MsoDeviceConstants) -- static register config table
```

### JESD204B Link Initialization

The ADS528x uses JESD204B serial interface, which requires a complex
link initialization sequence managed by the FPGA:

```
1. SetAds528xDrive -- configure physical layer drive strength
2. SetAds528xEnabled -- power on the ADC
3. ResetJesdPhyTransceivers -- reset the JESD PHY in the Lattice ECP5
4. Wait for JESD lock: "JESD reset started, waiting for lock..."
5. Monitor JESD status: "FPGA JESD lock=0x{:x} synced=0x{:x} error=0x{:x}"
6. Extended status: "power_good={} init_done={} sync_err={},
   lane_0-3_pll={}, lane_4-7_pll={}, smpl_pll={}"
7. Check lock result:
   - Success: "Locked JESD after {} ms, {} retries"
   - Failure: "JESD lock failed. Exiting."
   - Also: "Failed to reset JESD transceivers" / "Failed to reset JESD trasceivers" (sic)
8. Bitslip detection:
   - "bitslip detected after enabling Ads528x. ADC1: {} ADC2: {}. Attempt {} of {}"
   - "bitslip error detected after SERDES reset. attempt {}. ADC0: {}, ADC1: {}"
   - "ADC 1 bitslip detected" / "ADC 2 bitslip detected"
   - "Ads528x initialization - bitslip would not lock after multiple attempts."
   - Bitslip errors indicate lane alignment issues in the JESD204B interface
9. PLL and SERDES reset: "Lattice PLL and SERDES reset complete."
```

### JESD204B Status Register

The JESD status is read from FPGA register 0x0008 (STATUS_DDR_JESD):

```
JesdStatus {
    link_up: bool
    sync_status: u32
    realigned: bool
    aligned: bool
    spll_locked: bool    // System PLL
    cpll_locked: bool    // Channel PLL
}
```

Format string: `"JesdStatus(link_up={}, sync_status={}, realigned={}, aligned={}, spll_locked={}, cpll_locked={})"`

### JESD204B BER Test

Built-in bit error rate testing:
- `StartJesdBerTest`
- `GetJesdBerTestResults` / `"Getting JESD BER test results"`
- `StopJesdBerTest` / `"Stopping JESD BER test"`
- FPGA register 0x00FC controls BER test

### Bitslip Error Handling

The `BitslipError` exception type exists: `N6Saleae12BitslipErrorE`

Bitslip handling involves:
- `"resetting bitslip after slip, during shutdown, after bank shutoff"`
- `"resetting bitslip after slip, during shutdown."`
- Multiple retry attempts before declaring failure
- Two ADC chips (or banks) tracked independently: ADC0/ADC1 or ADC1/ADC2

### Lattice ECP5 Bitstream Design Names

Every Lattice ECP5 bitstream contains "ADS528x" in the design name:
- `Logic8_Top_ADS528x_impl1.ncd` -- Logic 8
- `Logic_Pro_Top_ADS528x_lse_impl1.ncd` -- Logic Pro 8
- `Logic_Pro16_Top_ADS528x_lse_impl1.ncd` -- Logic Pro 16

No Logic 4-specific Lattice bitstream was found.


## ADC Register Configuration Table

```cpp
gsl::span<const AdcRegisterValue> Saleae::(anonymous namespace)::
    GetAdcRegisterConfiguration(Saleae::MsoDeviceConstants)
```

String offset: 0x4116ea

This function returns a static table of ADC register address/value pairs
based on the `MsoDeviceConstants` for the device. The `AdcRegisterValue`
struct likely contains:
- Register address (U16)
- Register value (U16)

This table is used to configure the ADS528x ADC during initialization
on MSO/newer devices.


## ADC Gain Control

### Full-Scale Gain

```cpp
Saleae::Graph::LogicDevice::SetAdcFullScaleGain
HandleSetAdcFullScaleGain
```

String offsets:
- HandleSetAdcFullScaleGain: 0x38289f
- SetAdcFullScaleGain: 0x39dfbf

The `SetAdcFullScaleGain` action adjusts the ADC full-scale gain. For
HMCAD1100 devices, the gain is stored in the calibration data under the
key `/hittiteAdcGainRegisterValue` (offset 0x41f344).

Related calibration fields:
- `/fullScaleGain` -- full-scale gain setting
- `/fullScaleVoltageRanges` -- voltage ranges per full-scale setting

### IO Expander for ADC Control

The MSO/newer devices use I2C IO expanders to control the ADC and
related analog front-end components:

```
MsoDeviceInterface::BuildIoExpander_DcVgaAdcDac()
```

This constructs an I2C IO expander interface that controls:
- DC coupling relays
- VGA (variable gain amplifier) settings
- ADC power/mode pins
- DAC control

Log message: `"Configuring IO Expander for Dc/Vga/Adc/Dac"`


## ADC Temperature Monitoring

MSO/newer devices monitor ADC temperature:
```
MsoTemperature(afe={}C, adc={}C, fpga={}C, fan0={}C, fan1={}C)
```

The ADC temperature is separate from AFE and FPGA temperatures,
suggesting the ADC chip has its own temperature sensor.

ADC temperature field: `/adcCelsius`


## ADC Foreground Calibration

After ADC initialization, a foreground calibration procedure runs:

1. ADC registers are configured for calibration mode
2. Host waits: `"Waiting for ADC calibration to finish"`
3. ADC signals calibration complete (read-back of status register)
4. On success: proceeds to normal operation
5. On failure: `"ADC failed to finish foreground calibration"`

This calibration compensates for:
- ADC offset errors
- ADC gain errors
- ADC linearity (INL/DNL)
- Channel-to-channel mismatch (in multi-channel ADCs)

The calibration is triggered during device startup (CommonDeviceInit)
and potentially when sample rate or channel configuration changes.


## CommonDeviceInit Flow

The `Logic2FpgaDevice::CommonDeviceInit()` function is the entry point
for device initialization after FPGA programming. Mangled name evidence:

```
ZN16Logic2FpgaDevice16CommonDeviceInitEvE3$_1
```

The lambda `$_1` suggests CommonDeviceInit contains a callback
(likely for the ADC calibration wait). The callback signature
`FvbdE` = `void(bool, double)` suggests it receives a success flag
and a time/value parameter.

CommonDeviceInit likely:
1. Determines board revision and ADC type
2. Calls the appropriate ADC init function
3. Waits for ADC calibration with a timeout
4. Configures the AFE based on the ADC type


## Source Code Organization

From embedded `__FILE__` paths in the binary:

| Source File | Functions |
|-------------|-----------|
| `devices/logic_interface/src/Logic2FpgaDevice.cpp` | InitHmcad1100, SetAd9637Enabled, WriteAd9637Register, SetTestPattenHmcad1100, CommonDeviceInit, FakeReadThread, SetCaptureParameters |
| `devices/mso_device/src/mso_device/component/adc_12qj1600.cpp` | MSO ADC component (ADI 12QJ1600) |
| `devices/mso_device/src/mso_device/component/mso_fpga.cpp` | WriteSpi, ReadSpi_7BitAddress, ReadSpi_15BitAddress, ResetJesdPhyTransceivers |
| `devices/mso_device/src/mso_device/component/spi_flash.cpp` | SPI flash for MSO firmware |


## Cross-Reference with Existing Documentation

### FPGA Registers (doc/protocol/fpga-registers.md)

- Register 0x0004 (SPI_DATA): Used by WriteSpi/ReadSpi for all ADC register access
- Register 0x0008 (STATUS_DDR_JESD): JESD status for ADS528x, DDR ready status
- Register 0x00E0 (INIT_CONFIG): Post-programming init, precedes ADC init
- Register 0x0040 (CAPTURE_CONTROL): Master capture control
- Register 0x01C0 (COMMIT_CONTROL): Configuration commit
- Register 0x00FC (JESD_BER_TEST): BER test for JESD link validation

### Analog Capture Pipeline (re/notes/analog-capture-pipeline.md)

- ADC samples arrive as `VectorData<int16_t>` regardless of ADC bit width
- HMCAD1100 8-bit data is validated: `"invalid ADC data, exceedes 8 bits."`
- ADC-to-voltage transform: `voltage = adc_code * mScale + mOffset`
- Calibration data key: `/hittiteAdcGainRegisterValue` for HMCAD1100 gain

### Device Capabilities (doc/devices/device-capabilities.md)

| Device | ADC (Xilinx Rev) | ADC (Lattice Rev) | Bit Depth | ADC Range |
|--------|------------------|-------------------|-----------|-----------|
| Logic 4 | HMCAD1100 | ADS528x | 8/12-bit | 0-255 / -2048-2047 |
| Logic 8 | HMCAD1100 | ADS528x | 8/12-bit | -2048-2047 |
| Logic Pro 8 | AD9637 | ADS528x | 12-bit | -2048-2047 |
| Logic Pro 16 | AD9637 | ADS528x | 12-bit | -2048-2047 |


## String Evidence Summary

All ADC-related strings found in the binary, with file offsets:

### ADC Register Access
| Offset | String |
|--------|--------|
| 0x418078 | `ADC Read 0x{:x} = 0x{:x}` |
| 0x43acc1 | `ADC Write 0x{:x} = 0x{:x}` |

### HMCAD1100
| Offset | String |
|--------|--------|
| 0x363003 | `void Logic2FpgaDevice::InitHmcad1100(Hmcad1100Settings)` |
| 0x38291d | `void Hmcad1100Settings::SetDefault(::DeviceType)` |
| 0x3be1a8 | `void Logic2FpgaDevice::SetTestPattenHmcad1100(Hmcad1100Settings::AdcTestPattern)` |
| 0x3c4d8b | `Invalid adc pattern mode` |
| 0x41f344 | `/hittiteAdcGainRegisterValue` |

### AD9637
| Offset | String |
|--------|--------|
| 0x3b09b1 | `void Logic2FpgaDevice::SetAd9637Enabled(bool)` |
| 0x42772d | `void Logic2FpgaDevice::WriteAd9637Register(U16, U16)` |
| 0x3709c8 | `Unsupported board revision for Ad9637` |
| 0x4035e1 | `mBoardRevision.mAdcType == RevisionDetail::AdcType::AD9637` |
| 0x3d8109 | `Invalid ADC vendor id: {}{}` |
| 0x409f1c | `ADC Vendor ID: {:04x}` |
| 0x3e6a9f | `. Likely a general failure in reading from ADC MISO.` |

### ADS528x / JESD204B
| Offset | String |
|--------|--------|
| 0x3d8906 | `bitslip detected after enabling Ads528x. ADC1: {} ADC2: {}. Attempt {} of {}` |
| 0x3f525c | `Ads528x initialization - bitslip would not lock after multiple attempts.` |
| 0x3f52a5 | `SetAds528xDrive` |
| 0x42e2ae | `SetAds528xEnabled` |

### Calibration
| Offset | String |
|--------|--------|
| 0x3cb675 | `Waiting for ADC calibration to finish` |
| 0x3f49c3 | `ADC failed to finish foreground calibration` |
| 0x3c4409 | `ADC Locked` |
| 0x3fba5f | `Failed to lock ADC` |

### Configuration
| Offset | String |
|--------|--------|
| 0x3e67d9 | `Configuring ADC` |
| 0x43b80e | `set ADC range` |
| 0x3e68ba | `SALEAE_ANALOG_TEST_PATTERN` |
| 0x3bd6fc | `Configuring IO Expander for Dc/Vga/Adc/Dac` |
| 0x40aa59 | `CommonDeviceInit` |

### Board Revision
| Offset | String |
|--------|--------|
| 0x35c798 | `ParseBoardRevision` |
| 0x3d1d4c | `bool RevisionDetail::UsesNewCryptoInterface()` |


## Open Questions

1. **SpiAddressDataFormat enum values**: How exactly are the address and data
   bits packed in the 23-bit SPI data field? Different ADCs use different
   packing. HMCAD1100 uses 7-bit addr + 16-bit data, AD9637 uses 13-bit addr
   + 8-bit data. The enum likely has at least two values.

2. **HMCAD1100 channel mode selection**: How does Saleae decide between
   1-channel, 2-channel, and 4-channel modes? Likely based on the number
   of enabled analog channels and the requested sample rate.

3. **AD9637 multi-chip configuration**: Logic Pro 8 has 8 analog channels
   but AD9637 is a dual-channel ADC. How many AD9637 chips are used?
   At least 4 for Logic Pro 8 (8 channels / 2 per chip).

4. **ADS528x register programming**: The `GetAdcRegisterConfiguration`
   function returns a static table. What registers are in this table?
   Needs Ghidra decompilation to extract the table contents.

5. **Foreground calibration timing**: How long does foreground calibration
   take? The callback signature `void(bool, double)` suggests a timeout
   parameter. The polling loop with "Waiting for ADC calibration" suggests
   a maximum wait time.

6. **ADC data encoding**: Is the 12-bit AD9637 data left-justified
   (bits [15:4]) or right-justified (bits [11:0]) in the int16_t container?
   The fact that the JS bundle uses {min: -2048, max: 2047} suggests
   right-justified with sign extension (12-bit twos complement in 16-bit).

7. **HMCAD1100 8-bit to 12-bit normalization**: How exactly is the 8-bit
   HMCAD1100 data expanded to fill the int16_t? Logic 4 uses {min: 0,
   max: 255} (unsigned) while Logic 8 uses {min: -2048, max: 2047}
   (suggesting a 4-bit left shift and sign conversion for Logic 8).

8. **ADC 12QJ1600**: The MSO device uses an ADC part called "12QJ1600"
   (from the source file name `adc_12qj1600.cpp`). This appears to be
   an Analog Devices part. What is the full part number and register map?


## Ghidra Analysis Next Steps

1. Run `TraceAdcInit.py` to extract constants from the ADC init functions
2. Decompile `InitHmcad1100` to get the exact register write sequence
3. Decompile `WriteAd9637Register` to understand the SPI frame construction
4. Decompile `GetAdcRegisterConfiguration` to extract the ADS528x register table
5. Decompile `CommonDeviceInit` to trace the full ADC init call graph
6. Examine `ParseBoardRevision` to understand the AdcType selection logic
7. Look at `WriteSpi` to understand SpiAddressDataFormat enum values
