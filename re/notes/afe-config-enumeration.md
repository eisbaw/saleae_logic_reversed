# AFE Config Enumeration and HMCAD1100 Init Sequence

Analysis date: 2026-03-13
Binary: libgraph_server_shared.so (Saleae Logic 2 v2.4.40)
Method: Ghidra decompilation of ForConfigNumber, GetConfigNumber,
EncodeVgaConfig, AfeConfig_RawHardware format function, and
AllConfigs data table extraction.
Source: `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/oscilloscope_data.cpp`
VGA source: `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/component/vga_lmh6518.cpp`


## Summary

The Saleae Logic Pro and MSO devices use a fixed table of 66 AFE (Analog
Front-End) hardware configurations, indexed 0-65. Each configuration
specifies the attenuator, VGA preamp, and VGA ladder settings. The DAC
offset voltage is NOT part of the static config table -- it is computed
dynamically per-channel based on the desired voltage range.


## AfeConfig_RawHardware Struct Layout

From the format function `FUN_02e4f0c0` (ELF VA `0x2d4f0c0`) which uses:
```
AfeConfig_RawHardware(attn1={}, attn2={}, dac={:.6f}, preamp={}, ladder={}, filter={}, pwoer={})
```

**Confirmed struct layout** (24 bytes, stride 0x18):

| Offset | Size | Field    | Type     | Description                              |
|--------|------|----------|----------|------------------------------------------|
| 0x00   | 1    | attn1    | bool     | Attenuator 1: 0=bypass, 1=10x            |
| 0x01   | 1    | attn2    | bool     | Attenuator 2: 0=bypass, 1=100x           |
| 0x02   | 2    | preamp   | uint16   | LMH6518 preamp: 0=LG, 1=HG              |
| 0x04   | 2    | power    | uint16   | LMH6518 power mode (always 1)            |
| 0x06   | 2    | filter   | uint16   | LMH6518 filter/bandwidth (always 4)      |
| 0x08   | 1    | ladder   | int8     | LMH6518 ladder attenuation (0 to -20 dB) |
| 0x09   | 7    | padding  | -        | Unused/alignment                         |
| 0x10   | 8    | dac      | double   | DAC voltage (always 0.0 in static table) |

**Note on typo**: The original source has `pwoer` (typo of "power") in the
format string. This is present in the shipped binary.


## AllConfigs Table (66 entries)

Table location: Ghidra VA `0x0089a448`, ELF VA `0x0079a448`
Source: `oscilloscope_data.cpp`
Constant: `MsoConstants::TotalAfeConfigurations = 66` (hex `0x42`)

The table has a systematic structure: 3 attenuator combinations x 2 preamp
modes x 11 ladder values = 66 configurations.

All entries have `power=1`, `filter=4`, `dac=0.0` (these are constant).

| Index Range | attn1 | attn2 | preamp | ladder range  | Description         |
|-------------|-------|-------|--------|---------------|---------------------|
| 0-10        | 0     | 0     | HG (1) | 0 to -20 dB   | No atten, high gain |
| 11-21       | 0     | 0     | LG (0) | 0 to -20 dB   | No atten, low gain  |
| 22-32       | 1     | 0     | HG (1) | 0 to -20 dB   | 10x atten, high gain|
| 33-43       | 1     | 0     | LG (0) | 0 to -20 dB   | 10x atten, low gain |
| 44-54       | 1     | 1     | HG (1) | 0 to -20 dB   | 10x+100x, high gain |
| 55-65       | 1     | 1     | LG (0) | 0 to -20 dB   | 10x+100x, low gain  |

### Config Number Formula

Given the table structure, the config number can be computed as:

```
attn_group = (attn1 == 0 && attn2 == 0) ? 0
           : (attn1 == 1 && attn2 == 0) ? 1
           : 2   // attn1=1, attn2=1

config_number = attn_group * 22 + (preamp == HG ? 0 : 11) + (-ladder / 2)
```


## ForConfigNumber Function

Decompiled from `FUN_02e5b610` (ELF VA `0x2d5b610`, 143 bytes):

```c
AfeConfig_RawHardware* ForConfigNumber(AfeConfig_RawHardware* result, uint number)
{
    if (number > 0x42) {
        // assertion: "number >= 0 && number <= MsoConstants::TotalAfeConfigurations"
        abort();
    }
    if (number != 0x42) {
        // Index into AllConfigs array (stride 0x18 = 24 bytes)
        result[2] = AllConfigs[number * 0x18 + 0x10];  // dac (double)
        result[1] = AllConfigs[number * 0x18 + 0x08];  // ladder + padding
        result[0] = AllConfigs[number * 0x18 + 0x00];  // attn1, attn2, preamp, power, filter
        return result;
    }
    // array::at -- out of bounds (number == 66 is past-the-end)
    throw std::out_of_range("array::at");
}
```

Key facts:
- Valid config numbers: 0 to 65 (inclusive)
- Config number 66 (0x42) triggers an out-of-range exception
- The assertion string confirms the constant name: `MsoConstants::TotalAfeConfigurations`


## GetConfigNumber Function

Decompiled from `FUN_02e5b560` (ELF VA `0x2d5b560`, 168 bytes):

```c
int GetConfigNumber(const AfeConfig_RawHardware* this)
{
    int index = 0;
    long offset = 0;
    while (this[0] != AllConfigs[offset] ||
           this[1] != AllConfigs[offset+1] ||
           this[8] != AllConfigs[offset+8] ||
           this[2..3] != AllConfigs[offset+2..3]) {
        offset += 0x18;
        index++;
        if (offset == 0x630) {  // 0x630 = 66 * 0x18 = end of table
            // assertion: "it != AllConfigs.end()"
            abort();
        }
    }
    return index;
}
```

Key facts:
- Linear scan through AllConfigs comparing fields
- Compares: attn1 (byte 0), attn2 (byte 1), ladder (byte 8), preamp (bytes 2-3)
- Does NOT compare dac, power, or filter (those are constant across all entries)
- Total table size: `0x630` = 1584 bytes = 66 entries * 24 bytes


## EncodeVgaConfig Function (LMH6518 SPI Register Format)

Decompiled from `FUN_02e6a000` (ELF VA `0x2d6a000`, 295 bytes):

```c
uint16_t EncodeVgaConfig(const VgaConfig* config)
{
    int8_t ladder = config->ladder;  // at offset 6 (short, field [3])

    if (ladder > 0) {
        assert_fail("config.mLadderAttenuation <= 0");
    }
    if (ladder < -20) {
        assert_fail("config.mLadderAttenuation >= -20");
    }
    if (ladder & 1) {  // odd check
        assert_fail("config.mLadderAttenuation % 2 == 0");
    }

    // Successful encoding:
    uint16_t result = 0;
    result |= ((uint16_t)config->filter & 7) << 6;  // bits [8:6] = filter
    result |= (config->preamp != 0) ? (1 << 4) : 0; // bit 4 = preamp (HG=1, LG=0)
    result |= (config->power != 0) ? (1 << 10) : 0; // bit 10 = power mode
    result |= (uint16_t)((-ladder) >> 1);             // bits [3:0] = ladder/2

    return result;
}
```

### LMH6518 SPI Register Bit Layout (uint16_t)

| Bits   | Field       | Values                               |
|--------|-------------|--------------------------------------|
| [3:0]  | Ladder      | 0-10 (maps to 0 to -20 dB in 2dB steps) |
| [4]    | Preamp      | 0=Low Gain, 1=High Gain              |
| [5]    | (unused?)   |                                      |
| [8:6]  | Filter      | Bandwidth filter select (typically 4) |
| [9]    | (unused?)   |                                      |
| [10]   | Power       | 0=low power, 1=full power            |
| [15:11]| (unused?)   |                                      |

This matches the LMH6518 datasheet SPI control register format:
- The LMH6518 is an SPI-controlled VGA with a single 16-bit control word
- Bits [3:0] = attenuation (0 to -20 dB in 2 dB steps, encoded as 0-10)
- Bit [4] = preamp mode (0=LG/-6.5dB, 1=HG/+13.5dB per datasheet)
- Bits [8:6] = filter bandwidth select
- Bit [10] = power mode

### VgaConfig Struct Layout

From EncodeVgaConfig parameter access patterns:

| Offset | Field   | Type    | Description                        |
|--------|---------|---------|------------------------------------|
| 0      | preamp  | int16   | Preamp mode: 0=LG, nonzero=HG     |
| 2      | power   | int16   | Power mode: 0=low, nonzero=full    |
| 4      | filter  | int16   | Filter/bandwidth select (0-7)      |
| 6      | ladder  | int8    | Ladder attenuation (0 to -20, even)|

Wait -- from the EncodeVgaConfig decompilation:
```c
config->filter  = param_1[2]   // short at offset 4 (index 2 in short array)
config->preamp  = *param_1     // short at offset 0
config->power   = param_1[1]   // short at offset 2
config->ladder  = param_1[3]   // byte at offset 6
```

So VgaConfig is:
| Offset | Field   | Size | Description |
|--------|---------|------|-------------|
| 0      | preamp  | 2    | 0=LG, 1=HG |
| 2      | power   | 2    | Power mode  |
| 4      | filter  | 2    | Filter/BW   |
| 6      | ladder  | 2    | int8 stored in short |


## HMCAD1100 Full Init Sequence (CommonDeviceInit)

From `FUN_01d21fb0` (ELF VA `0x1c21fb0`, 2260 bytes), the
`Logic2FpgaDevice::CommonDeviceInit()` function handles FPGA post-
programming init for all Logic devices. The ADC init path is selected by
a `mAdcType` field at `param_1 + 0x218`:

```c
int adc_type = *(int*)(this + 0x218);
if (adc_type == 0) {
    // HMCAD1100 (Xilinx Spartan-6)
    Hmcad1100Settings::SetDefault(device_type);
    InitHmcad1100(settings);
    FUN_01d22af0(this);  // Device2_1 init (I2C 0xC0)
}
else if (adc_type == 1) {
    // AD9637 (Xilinx)
    FUN_01d20d90(this, 0x200000001, 0);
}
else if (adc_type == 2) {
    // ADS528x (Lattice ECP5 / JESD204B)
    // FPGA r0x00 = 1, wait 1ms
    // FPGA r0x0F = 0
    // SetAds528xDrive(...)
    // FPGA r0x2A = 0x1111
    // FPGA r0x2B = 0x1111
    // FPGA r0x42 = 0x8040
    // FPGA r0x46 = 0x8204
    // wait 10ms
    // FUN_01d22af0(this)  // Device2_1 init
}
```

### HMCAD1100 Init (adc_type == 0)

The `InitHmcad1100` function (`FUN_01d20ab0`, ELF VA `0x1c20ab0`) writes
HMCAD1100 registers via the FPGA SPI bridge (`FUN_01d207c0` = WriteHmcadRegister):

```
WriteHmcadRegister(this, 0x25, test_pattern_25)   // test pattern reg
WriteHmcadRegister(this, 0x45, test_pattern_45)   // full-scale adjust
WriteHmcadRegister(this, 0x11, coarse_gain_mode)  // LVDS output mode
WriteHmcadRegister(this, 0x12, fine_gain_mode)     // LVDS output phase
WriteHmcadRegister(this, 0x2A, gain_val)           // coarse gain ch1
WriteHmcadRegister(this, 0x2B, gain_val)           // coarse gain ch2
WriteHmcadRegister(this, 0x46, clock_config)       // clock config
WriteHmcadRegister(this, 0x50, jitter_ctrl)        // jitter control
WriteHmcadRegister(this, 0x55, resolution)         // resolution select
WriteHmcadRegister(this, 0x56, calibration)        // calibration
```

The coarse gain value (register 0x2A, 0x2B) is replicated across nibbles:
```c
coarse_gain = (settings.coarse << 8) | (settings.coarse << 4) | settings.coarse;
```

### Hmcad1100Settings::SetDefault(DeviceType)

From `FUN_01d1f900` (ELF VA `0x1c1f900`):

| DeviceType | Mode (field[0]) | Channel (field[1]) | Gain (field[6]) |
|------------|-----------------|---------------------|-----------------|
| 5 (Pro 8?) | 5               | 6                   | 2               |
| 6 or 7     | 4               | 7                   | 0               |

Field assignments:
```c
if (device_type == 5) {
    settings[0] = 5;   // mode
    settings[1] = 6;   // channel config
    settings[6] = 2;   // gain mode
} else if ((device_type & 0xFFFFFFFE) == 6) {  // 6 or 7
    settings[0] = 4;   // mode
    settings[1] = 7;   // channel config
    settings[6] = 0;   // gain mode
}
// Common initialization:
settings[2..3] = 0x100000000  // two u32 fields: {0, 1}
settings[4] = 1               // (as u16)
settings[5] = 0
settings[8..9] = 0            // zero two u32 fields
```

### ADS528x Init (adc_type == 2)

The ADS528x path writes these FPGA registers:
```
r0x00 = 1          (FPGA power-on / enable)
sleep(1ms)
r0x0F = 0          (clear reset)
SetAds528xDrive()  (configure drive strength from lookup table)
r0x2A = 0x1111     (coarse gain for all channels)
r0x2B = 0x1111     (coarse gain for all channels)
r0x42 = 0x8040     (clock config)
r0x46 = 0x8204     (additional clock config)
sleep(10ms)
```

### FPGA Verification (All ADC Types)

After ADC init, all paths converge on FPGA verification:
```
WriteReg(0x7F, 0xAA)    // write test value to FPGA readback register
WriteReg(0x7F, 0xAA)    // write again (redundancy)
ReadReg(0x7F) == 0xAA?  // verify FPGA responds
```

If readback fails:
- Log: "FPGA readback got {:#x} instead of 0xAA"
- Log: "FPGA readback failed for {device_name}"
- Throw: "Fpga initialization failed"

### Post-Init Channel Configuration (Logic Pro 8/16)

For Pro devices (device_type & 0xFFFFFFFE == 6):
```
WriteReg(0x15, 0)  // clear channel config register
```

Then based on adc_type:
- adc_type 0: `WriteReg(0x0F, 0x100)` -- enable ADC outputs
- adc_type 1: `FUN_01d23330(this, 0)` -- AD9637-specific setup
- adc_type 2: `WriteReg(0x0F, 0x100)` -- enable ADC outputs


## Coarse Gain Lookup (FUN_01d1f9b0)

The function at ELF VA `0x1c1f9b0` computes a coarse gain register
value from a double (the requested gain in dB). It:

1. Builds a table of 64 (0x40) gain values using the formula:
   `gain[i] = i * K1 * K2 + K3` (where K1, K2, K3 are FP constants)
2. Performs binary search to find the closest entry to the requested gain
3. Returns the index as the register value (uint)

This maps a continuous gain request to the nearest HMCAD1100 coarse gain
register value (0-63).


## SetAds528xDrive (FUN_01d20f60)

The function at ELF VA `0x1c20f60` configures ADS528x drive strength:

1. Builds a red-black tree of valid drive current values (in mA)
2. Builds a red-black tree of valid termination resistance values (in ohms)
3. Looks up the requested drive/termination combination
4. If found: writes HMCAD1100 registers r0x11 and r0x12 with the encoded values
5. If not found: logs available options and the closest match

Drive current and termination values are loaded from data tables at:
- Drive currents: `DAT_00572bf0` through `DAT_00572c68` (8 entries)
- Additional set: `DAT_00572c70` through `DAT_00572ca8` (4 entries)

The register encoding:
```c
r0x11 = (drive_3bits << 8) | (drive_3bits << 4) | drive_3bits
r0x12 = (term_3bits << 8) | (term_3bits << 4) | (enable_bit << 14) | term_3bits
```


## Confidence Assessment

| Finding                                    | Confidence  | Evidence                      |
|--------------------------------------------|-------------|-------------------------------|
| TotalAfeConfigurations = 66                | **Confirmed** | Assertion + table extraction  |
| AfeConfig struct layout (24 bytes)         | **Confirmed** | Format function + table data  |
| AllConfigs table contents                  | **Confirmed** | Direct memory extraction      |
| EncodeVgaConfig bit layout                 | **Confirmed** | Decompilation + LMH6518 match |
| Config ordering (attn x preamp x ladder)   | **Confirmed** | Table data                    |
| power always 1, filter always 4            | **Confirmed** | All 66 entries identical      |
| DAC not in static config                   | **Confirmed** | All 66 entries have dac=0.0   |
| ADC type enum: 0=HMCAD, 1=AD9637, 2=ADS528x | **Confirmed** | Switch statement in CommonDeviceInit |
| HMCAD1100 register sequence                | **Confirmed** | Decompilation                 |
| Hmcad1100Settings per DeviceType           | **Confirmed** | Decompilation                 |
| Coarse gain lookup binary search           | **Confirmed** | Decompilation                 |
| attn1 = 10x attenuator                     | **Hypothesis** | Naming from earlier analysis, needs hardware validation |
| attn2 = 100x attenuator                    | **Hypothesis** | Naming from earlier analysis, needs hardware validation |


## Impact on Rust Code

The existing `AfeChannelConfig` in `crates/saleae-driver/src/capture.rs` is
marked PLACEHOLDER and has several issues compared to the decompiled truth:

1. **Missing `filter` and `power` fields** -- though constant (filter=4, power=1),
   they should be present for correctness
2. **`gain` field is speculative** -- the actual VGA gain is encoded via
   `EncodeVgaConfig` which combines preamp, ladder, filter, and power into a
   single uint16_t. There is no separate "gain" field.
3. **`to_register_value()` is wrong** -- the real encoding is the LMH6518 SPI
   format from EncodeVgaConfig, not the speculative bit packing
4. **`ladder` should be `i8` not `u8`** -- values are 0 to -20
5. **`dac_offset` is a `double` voltage, not a `u16` code** -- the DAC voltage
   is computed separately and is not part of the static config

These corrections should be addressed in a future task. The current task
is focused on the RE analysis, not code changes.


## Ghidra Scripts Used

- `re/ghidra/scripts/DecompileAfeConfig.java` -- decompiles ForConfigNumber,
  EncodeVgaConfig, GetConfigNumber, AfeConfig_RawHardware format function,
  and the MSO device init function containing GetAdcRegisterConfiguration
- `re/ghidra/scripts/ExtractAfeConfigTable.java` -- extracts all 66 entries
  of the AllConfigs table from binary memory
- `re/ghidra/scripts/DecompileHmcadInit.java` -- (existing) decompiles
  InitHmcad1100 and related functions
- `re/ghidra/scripts/DecompileHmcadHelpers.java` -- (existing) decompiles
  WriteHmcadRegister, coarse gain lookup, and SetAds528xDrive


## Open Questions

1. **GetAdcRegisterConfiguration table contents**: The MSO init function
   (FUN_02e36360) calls GetAdcRegisterConfiguration to get ADC register
   values, but the actual register table data is at `DAT_02fe5180` and
   `DAT_02fe5190` (two different tables for 1 GHz vs 1.6 GHz ADC clock).
   These contain gsl::span structs pointing to the actual register data.
   Extracting those tables would reveal the exact ADC init register sequence
   for MSO devices.

2. **LMH6518 filter bandwidth values**: Filter=4 is used for all static
   configs. What bandwidth does this correspond to? The LMH6518 datasheet
   lists filter options: 20/100/200/350/650/750 MHz plus a "full bandwidth"
   setting. Filter=4 likely maps to one of these.

3. **Power mode 1**: What does power=1 mean exactly? The LMH6518 has
   auxiliary power saving modes. Bit 10 = 1 likely means "full power".

4. **Attenuator mapping**: Is attn1=10x and attn2=100x, or vice versa?
   The `/attn10Transform` and `/attn100Transform` calibration paths suggest
   the naming, but hardware validation is needed.
