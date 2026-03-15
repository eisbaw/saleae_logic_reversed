# Capture Register Formulas: Decompilation vs Empirical Validation

Analysis date: 2026-03-15
Binary: libgraph_server_shared.so (Saleae Logic 2 v2.4.40)
Function: `SetCaptureParameters` (ELF 0x1c24ca0, Ghidra 0x1d24ca0)
Device: Logic Pro 8, HW rev 1.2, device type 6
Clocks: digital=500MHz (offset 0x194), analog=50MHz (offset 0x198)


## Device Constants (Logic Pro 8)

| Constant | Value | Source |
|----------|-------|--------|
| Digital FPGA clock | 500 MHz | DeviceSettings offset 0x194 |
| Analog FPGA clock | 50 MHz | DeviceSettings offset 0x198 |
| GPIF bus width | 32 (0x20) | Pro device type |
| ADC bits per sample | 12 | AD9637 |
| analog_bytes | 8 | (gpif=32, adc=12) table in DeviceCalculateMixRate |
| samples_per_group | 3 | (gpif=32, adc=12) table in DeviceCalculateMixRate |
| Channel count | 8 | Logic Pro 8 |


## Empirical Configurations (from pcap)

For quick reference throughout this document:

| # | Mode | Digital Rate | Analog Rate | Dig Ch | Ana Ch | USB |
|---|------|-------------|-------------|--------|--------|-----|
| 1 | Mixed | 6.25 MHz | 1.5625 MHz | 8 | 8 | 2.0 |
| 2 | Digital | 1 MHz | -- | 8 | 0 | 3.0 |
| 3 | Analog | -- | 1.5625 MHz | 0 | 2 | 3.0 |
| 4 | Digital | 50 MHz | -- | 2 | 0 | 3.0 |
| 5 | Mixed | 50 MHz | 12.5 MHz | 4 | 2 | 3.0 |
| 6 | Mixed | 250 MHz | 6.25 MHz | 4 | 2 | 3.0 |

NOTE: Config 1 was originally labeled "12.5 MHz" but back-computation from
register values proves the actual rates are 6.25 MHz digital / 1.5625 MHz
analog. The register values `r0x0b=0x10` (divider 16) and `r0x0a=0x20`
(decimation 32) give: 500M/16/5=6.25M digital, 50M/32=1.5625M analog.


## Register-by-Register Analysis

### r0x00 (CAPTURE_INIT) - Always 0x00

Decompiled: Written as 0x00 during configuration. Set to 0x01 to start capture,
0x00 to stop. Also used for JESD cascade values (0x1E, 0x1C, 0x18, 0x08, 0x00).

All 6 configs: 0x00. **MATCH.**


### r0x06 (DIGITAL channel enable bitmask)

Decompiled: Iterates channels, checks `FUN_01d47510` (offset +0x14 == 0) to
determine if channel's digital is enabled. Builds bitmask with `1 << channel_index`.

Formula: `bitmask of enabled digital channels`

| Config | Expected | Pcap | Match |
|--------|----------|------|-------|
| 1 | 0xFF (8 dig) | 0xFF | YES |
| 2 | 0xFF (8 dig) | 0xFF | YES |
| 3 | 0x00 (0 dig) | 0x00 | YES |
| 4 | 0x03 (2 dig) | 0x03 | YES |
| 5 | 0x0F (4 dig) | 0x0F | YES |
| 6 | 0x0F (4 dig) | 0x0F | YES |

**CRITICAL: r0x06 is DIGITAL, not analog.** The pcap proves this: config 4
(digital-only 2ch) has r0x06=0x03, r0x08=0x00. Our code and register map
incorrectly label r0x06 as analog.


### r0x08 (ANALOG channel enable bitmask)

Decompiled: Iterates channels, checks `FUN_01d47500` (offset +0x18 == 0) to
determine if channel's analog is enabled. Builds bitmask.

Formula: `bitmask of enabled analog channels`

| Config | Expected | Pcap | Match |
|--------|----------|------|-------|
| 1 | 0xFF (8 ana) | 0xFF | YES |
| 2 | 0x00 (0 ana) | 0x00 | YES |
| 3 | 0x03 (2 ana) | 0x03 | YES |
| 4 | 0x00 (0 ana) | 0x00 | YES |
| 5 | 0x03 (2 ana) | 0x03 | YES |
| 6 | 0x03 (2 ana) | 0x03 | YES |

**CRITICAL: r0x08 is ANALOG, not digital.**


### r0x0a (ANALOG_DECIMATION)

Decompiled algorithm:
```
raw = 50_000_000 / analog_physical_rate   (integer division)
if raw in {1, 2, 3}:
    raw -= 1
result = raw & 0xFF
```

The subtraction for values 1-3 handles the FPGA's off-by-one for low
decimation factors. For digital-only (no analog), result is 0.

| Config | Rate | 50M/rate | Adjust | Expected | Pcap | Match |
|--------|------|----------|--------|----------|------|-------|
| 1 | 1.5625M | 32 | no | 0x20 | 0x20 | YES |
| 2 | -- | 0 | -- | 0x00 | 0x00 | YES |
| 3 | 1.5625M | 32 | no | 0x20 | 0x20 | YES |
| 4 | -- | 0 | -- | 0x00 | 0x00 | YES |
| 5 | 12.5M | 4 | ->3 | 0x04* | 0x04 | YES |
| 6 | 6.25M | 8 | no | 0x08 | 0x08 | YES |

*Config 5: 50M/12.5M = 4. Since 4 is NOT in {1,2,3}, no subtraction. Result = 4.

Note: The subtraction rule matters for configs with analog rates of 50M (raw=1),
25M (raw=2), or 16.67M (raw=3). None of our 6 configs trigger it.


### r0x0b:r0x0c (DIGITAL_DIVIDER)

Decompiled algorithm (Pro devices, device type 6):
```
raw_divider = 500_000_000 / digital_rate   (integer division)

# Pro special case: dividers 1 -> 0
if raw_divider == 1:
    raw_divider = 0

# Pro: divide by 5 (the /5 step)
raw_divider = raw_divider / 5

r0x0b = raw_divider & 0xFF
r0x0c = (raw_divider >> 8) & 0xFF
```

The /5 step uses compiler magic: `imul $0xcccd; shr $0x12` = unsigned divide by 5.
The effective formula is `100M / digitalRate` for most rates.

| Config | Rate | 500M/rate | /5 | Expected | Pcap r0x0b | Pcap r0x0c | Match |
|--------|------|-----------|-----|----------|------------|------------|-------|
| 1 | 6.25M | 80 | 16 | 0x10 | 0x10 | 0x00 | YES |
| 2 | 1M | 500 | 100 | 0x64 | 0x64 | 0x00 | YES |
| 3 | -- | 0 | 0 | 0x00 | 0x00 | 0x00 | YES |
| 4 | 50M | 10 | 2 | 0x02 | 0x02 | 0x00 | YES |
| 5 | 50M | 10 | 2 | 0x02 | 0x02 | 0x00 | YES |
| 6 | 250M | 2 | 0* | 0x00 | 0x00 | 0x00 | YES |

*Config 6: 500M/250M = 2. Special case: raw==1 maps to 0 (but 2 does not).
Then 2/5 = 0 (integer truncation). Result = 0.


### r0x0d:r0x0e (MIX_RATIO via DeviceCalculateMixRate)

Called as:
```
DeviceCalculateMixRate(
    numDigital,          // digital_multiplier
    numAnalog,           // analog_multiplier
    digitalRate,         // digital_sample_rate (Hz)
    analogRate,          // analog_sample_rate (Hz)
    32,                  // gpif_width (Pro)
    12,                  // adc_bits (AD9637)
    &out_dig_mixed, &out_ana_mixed, &out_dig_quotient, &out_ana_quotient
)
```

For Pro 8: analog_bytes=8, samples_per_group=3.

Algorithm:
- Digital-only (analogRate==0): dig_quotient=1, ana_quotient=0
  - r0x0d = 0, r0x0e = numDigital * 1
- Analog-only (digitalRate==0): dig_quotient=0, ana_quotient=1
  - r0x0d = 3 * numAnalog * 1, r0x0e = 0
- Mixed:
  - dig_reduced = digitalRate / 32
  - ana_reduced = analogRate / 8
  - g = GCD(dig_reduced, ana_reduced)
  - dig_quotient = dig_reduced / g
  - ana_quotient = ana_reduced / g
  - r0x0d = 3 * numAnalog * ana_quotient
  - r0x0e = numDigital * dig_quotient

Register assignment: **r0x0d = analog mix value, r0x0e = digital mix value**.

| Config | Mode | Computation | r0x0d | r0x0e | Pcap 0d | Pcap 0e | Match |
|--------|------|-------------|-------|-------|---------|---------|-------|
| 1 | Mixed | dr=6.25M/32=195312, ar=1.5625M/8=195312, g=195312, dq=1, aq=1 | 3*8*1=24 | 8*1=8 | 0x18 | 0x08 | YES |
| 2 | DigOnly | dq=1, aq=0 | 0 | 8*1=8 | 0x00 | 0x08 | YES |
| 3 | AnaOnly | dq=0, aq=1 | 3*2*1=6 | 0 | 0x06 | 0x00 | YES |
| 4 | DigOnly | dq=1, aq=0 | 0 | 2*1=2 | 0x00 | 0x02 | YES |
| 5 | Mixed | dr=50M/32=1562500, ar=12.5M/8=1562500, g=1562500, dq=1, aq=1 | 3*2*1=6 | 4*1=4 | 0x06 | 0x04 | YES |
| 6 | Mixed | dr=250M/32=7812500, ar=6.25M/8=781250, g=781250, dq=10, aq=1 | 3*2*1=6 | 4*10=40 | 0x06 | 0x28 | YES |


### r0x12 (MODE / CAPTURE_CONFIG_2)

Decompiled: Determined by ADC type string matching via `FUN_01d4bee0` and
`FUN_01d4c510`. The ADC type configurations are indexed 0-2:

| Index | ADC Type String | Meaning | Register Value |
|-------|----------------|---------|----------------|
| 0 | (analog-only match) | Analog-only | 1 << 0 = 0x01 |
| 1 | (USB2 mixed match) | USB 2.0 mixed | 1 << 1 = 0x02 |
| 2 | (USB3 digital/mixed) | USB 3.0 digital or mixed | 1 << 2 = 0x04 |

Formula: `1 << adcTypeIndex`

Selection logic (reconstructed from pcap patterns):
- Analog-only (no digital channels) -> index 0 -> 0x01
- USB 2.0 + mixed -> index 1 -> 0x02
- USB 3.0 + (digital-only OR mixed) -> index 2 -> 0x04

| Config | Mode | USB | Expected | Pcap | Match |
|--------|------|-----|----------|------|-------|
| 1 | Mixed | 2.0 | 0x02 | 0x02 | YES |
| 2 | Digital | 3.0 | 0x04 | 0x04 | YES |
| 3 | Analog | 3.0 | 0x01 | 0x01 | YES |
| 4 | Digital | 3.0 | 0x04 | 0x04 | YES |
| 5 | Mixed | 3.0 | 0x04 | 0x04 | YES |
| 6 | Mixed | 3.0 | 0x04 | 0x04 | YES |


### r0x13 (CAPTURE_CONTROL)

Decompiled:
```
if board_revision >= 2:
    result = 0x01
    if usb_speed == USB3:
        result |= 0x02    # ignore-errors bit
else:
    result = 0x00
```

All 6 configs are from board rev 1.2 (integer revision 1), so r0x13 = 0x00.

| Config | Expected | Pcap | Match |
|--------|----------|------|-------|
| All 6 | 0x00 | 0x00 | YES |


### r0x14 (DECIMATION_FACTOR, formerly mislabeled "SCOPE_CFG" / "gpif_bus_width")

This is NOT a GPIF bus width register. It is a decimation/bandwidth factor
computed from total data throughput.

Decompiled algorithm (via `FUN_01d4bbb0`):

```
T = numDigital * digitalRate + numAnalog * analogRate * bitsPerSample

where bitsPerSample = 12 (AD9637)

base = T / 32                    # 32 = GPIF bus width
factor = (base + 99_999_999) / base   # ceiling division: ceil(100M / (T/32))
                                       # equivalently: ceil(3_200_000_000 / T)
result = min(0xFF, factor)
```

The `(base + 99_999_999) / base` pattern is `ceil(100_000_000 / base)`, which
equals `ceil(3_200_000_000 / T)` after substituting `base = T/32`.

| Config | numDig | digRate | numAna | anaRate | T | T/32 | ceil(100M/(T/32)) | min(0xFF) | Pcap | Match |
|--------|--------|---------|--------|---------|---|------|-------------------|-----------|------|-------|
| 1 | 8 | 6.25M | 8 | 1.5625M | 200M | 6250000 | 16 | 0x10 | 0x10 | YES |
| 2 | 8 | 1M | 0 | -- | 8M | 250000 | 400->255 | 0xFF | 0xFF | YES |
| 3 | 0 | -- | 2 | 1.5625M | 37.5M | 1171875 | 86 | 0x56 | 0x56 | YES |
| 4 | 2 | 50M | 0 | -- | 100M | 3125000 | 32 | 0x20 | 0x20 | YES |
| 5 | 4 | 50M | 2 | 12.5M | 500M | 15625000 | 7* | 0x07 | 0x07 | YES |
| 6 | 4 | 250M | 2 | 6.25M | 1150M | 35937500 | 3* | 0x03 | 0x03 | YES |

*Config 5: T = 4*50M + 2*12.5M*12 = 200M + 300M = 500M. T/32=15625000.
ceil(100M/15625000) = ceil(6.4) = 7. Correct.

*Config 6: T = 4*250M + 2*6.25M*12 = 1000M + 150M = 1150M. T/32=35937500.
ceil(100M/35937500) = ceil(2.78) = 3. Correct.


### r0x1a (ADC_CONFIG)

Decompiled: Based on the raw digital divider value BEFORE the /5 step.

```
raw_divider = 500_000_000 / digitalRate

if raw_divider == 2:
    adc_config = 1
elif raw_divider == 4:
    adc_config = 2
else:
    adc_config = 0
```

Only triggers for digitalRate = 250MHz (raw=2) or 125MHz (raw=4).

| Config | Digital Rate | 500M/rate | Expected | Pcap | Match |
|--------|-------------|-----------|----------|------|-------|
| 1 | 6.25M | 80 | 0x00 | 0x00 | YES |
| 2 | 1M | 500 | 0x00 | 0x00 | YES |
| 3 | -- | -- | 0x00 | 0x00 | YES |
| 4 | 50M | 10 | 0x00 | 0x00 | YES |
| 5 | 50M | 10 | 0x00 | 0x00 | YES |
| 6 | 250M | 2 | 0x01 | 0x01 | YES |


### r0x1b (ANALOG_DEC_UPPER)

Decompiled: Only written for board revision 1.
```
value = (analog_decimation >> 8) & 0xFF
if value > 1: value = 1
```

For all 6 configs, analog_decimation fits in 8 bits, so r0x1b = 0x00.
This register is not included in the pcap WriteRegisters payload for our
configs (it is only present when board_revision == 1 AND the decimation
overflows 8 bits). Since all our decimation values are <= 0x64, the upper
byte is always 0. The register is written but the pcap format bundles it
with the other registers.


## Validation Summary

| Register | Formula | All 6 Match | Notes |
|----------|---------|-------------|-------|
| r0x00 | Always 0x00 | YES | |
| r0x06 | Digital channel bitmask | YES | SWAPPED in our code |
| r0x08 | Analog channel bitmask | YES | SWAPPED in our code |
| r0x0a | 50M / analogRate (with {1,2,3} adjust) | YES | |
| r0x0b:0c | (500M / digRate) / 5 (special cases) | YES | |
| r0x0d | 3 * numAnalog * ana_quotient | YES | |
| r0x0e | numDigital * dig_quotient | YES | |
| r0x12 | 1 << adcTypeIndex | YES | |
| r0x13 | rev>=2: 0x01+USB3 bit; else 0x00 | YES | |
| r0x14 | min(0xFF, ceil(3.2G / T)) | YES | |
| r0x1a | raw_div==2: 1, raw_div==4: 2, else: 0 | YES | |

**All 12 registers match all 6 empirical configurations exactly.**


## Discrepancies in Current Rust Implementation

### BUG 1: r0x06/r0x08 Register Address Swap (CRITICAL)

**File**: `crates/saleae-proto/src/fpga_regs.rs` lines 464-473
**File**: `crates/saleae-driver/src/capture.rs` struct comments

Current code:
```rust
pub const FEAT_ANALOG_CH_ENABLE_LO: u8 = 0x06;   // WRONG: 0x06 is DIGITAL
pub const FEAT_DIGITAL_CH_ENABLE_LO: u8 = 0x08;   // WRONG: 0x08 is ANALOG
```

Should be:
```rust
pub const FEAT_DIGITAL_CH_ENABLE_LO: u8 = 0x06;
pub const FEAT_ANALOG_CH_ENABLE_LO: u8 = 0x08;
```

Impact: In the reference pcap config (all 8+8), both are 0xFF so the bug is
invisible. Any config with different digital/analog channel counts will write
the masks to the wrong registers. This will cause incorrect channel selection
on the FPGA.

Evidence: Config 4 (digital-only 2ch) has r0x06=0x03, r0x08=0x00. If 0x06
were analog, the analog mask would be 0x03 in a digital-only capture, which
is nonsensical.

Also need to fix: `FEAT_ANALOG_CH_ENABLE_HI` (currently 0x07, should be 0x09)
and `FEAT_DIGITAL_CH_ENABLE_HI` (currently 0x09, should be 0x07).


### BUG 2: PRO_FPGA_MASTER_CLOCK_HZ Constant (MODERATE)

**File**: `crates/saleae-driver/src/capture.rs` line 966

Current: `PRO_FPGA_MASTER_CLOCK_HZ = 200_000_000`

The actual device stores 500 MHz at offset 0x194. The algorithm computes
`(500M / rate) / 5`. For the common case this yields the same register value
as `200M / rate` (which is `100M/rate` after the /5), but the intermediate
value matters for the special-case handling (raw_divider == 1 maps to 0).

Recommendation: Change to `500_000_000` and add the explicit /5 step. This
makes the code match the decompiled algorithm and handles edge cases correctly.
Specifically: for 500 MHz digital rate, 500M/500M=1, special-case to 0, /5=0.
With the current 200M constant: 200M/500M=0 (integer truncation), which gives
a different code path.


### BUG 3: ADC_CONFIG (r0x1a) Uses Wrong Input (MODERATE)

**File**: `crates/saleae-driver/src/capture.rs` lines 1171-1179

Current code checks `analog_decimation` values {0,1} -> 0 and {2} -> 1.

Actual algorithm checks the **raw digital divider** (500M/digitalRate) BEFORE
the /5 step:
- raw_divider == 2 (digitalRate=250MHz) -> 1
- raw_divider == 4 (digitalRate=125MHz) -> 2
- everything else -> 0

These are completely different inputs. The current code would give wrong values
for any high-speed digital rate config.


### BUG 4: r0x14 Formula Completely Wrong (CRITICAL)

**File**: `crates/saleae-driver/src/capture.rs` field `gpif_bus_width`

Current code stores a fixed GPIF bus width value (0x10 for 32-bit Pro, 0x08
for 16-bit). The actual register is a dynamically computed decimation factor
based on total capture bandwidth:

```
T = numDigital * digitalRate + numAnalog * analogRate * 12
result = min(255, ceil(3_200_000_000 / T))
```

This register tells the FPGA how to pace data output relative to USB bandwidth.
Getting it wrong will cause overflow or underflow of the FPGA data FIFO.


### BUG 5: Mix Ratio Formula Simplified (MODERATE)

**File**: `crates/saleae-driver/src/capture.rs` around line 1157

The current `compute_mix_ratios` function uses a simplified formula that
does not match `DeviceCalculateMixRate`. The correct algorithm requires:
1. Dividing digital rate by GPIF width (32)
2. Dividing analog rate by analog_bytes (8)
3. Computing GCD of these reduced values
4. Using the GCD-based quotients to scale channel counts
5. Applying `samples_per_group` (3) multiplier to analog side

The current code approximates this but uses different divisors and grouping.


### BUG 6: capture_config_2 (r0x12) Ignores USB Speed and Capture Mode (MINOR)

**File**: `crates/saleae-driver/src/capture.rs` lines 1196-1200

Current code: board_rev <= 1 -> 0x02, else -> 0x01.

Actual algorithm: Based on ADC type string matching that considers whether
the capture is analog-only (0x01), USB 2.0 mixed (0x02), or USB 3.0
digital/mixed (0x04). The board revision determines which ADC type
*configurations* are available, but the mode selection depends on the
actual capture parameters.


### BUG 7: Config 1 Rate Labels Wrong (DOCUMENTATION)

**File**: `crates/saleae-driver/src/capture.rs` line 1221

The "pcap reference" check uses 12.5 MHz, but the actual pcap rates are
6.25 MHz digital / 1.5625 MHz analog. The register values were correct
because 200M/16=12.5M happens to give the same divider as (500M/6.25M)/5=16,
but the rate label is wrong.


## Recommendations for Fixing compute_capture_registers()

Priority order:

1. **Fix register address swap** (BUG 1): Swap the constants in fpga_regs.rs.
   This is the most dangerous bug -- it will corrupt any non-symmetric capture.

2. **Fix r0x14 formula** (BUG 4): Replace the static GPIF bus width value with
   the bandwidth-based decimation factor formula. This affects FPGA data pacing.

3. **Fix clock constant and add /5 step** (BUG 2): Change to 500MHz and add
   the Pro /5 division with the raw_divider==1 special case.

4. **Fix ADC_CONFIG input** (BUG 3): Check raw digital divider instead of
   analog decimation. Only matters at 250MHz and 125MHz digital rates.

5. **Fix mix ratio formula** (BUG 5): Implement the full DeviceCalculateMixRate
   algorithm. The reference implementation is in `re/notes/device-helpers.md`.

6. **Fix MODE register** (BUG 6): Add USB speed and capture mode to the
   selection logic instead of using only board revision.

7. **Fix rate labels** (BUG 7): Update the pcap reference config check to use
   the correct rates (6.25 MHz digital, 1.5625 MHz analog).


## Open Questions

1. **SampleRateOption field values for pure digital/analog modes**: When a
   config is digital-only, what value does `analogPhysicalRate` have in the
   SampleRateOption? The formula works empirically with rate=0 for the missing
   mode, but we have not confirmed this from the binary's rate tables.

2. **r0x1b behavior for large decimation values**: We have not seen a config
   where analog_decimation > 255. The formula clips to 1, suggesting values
   above 256 are treated as 256 (decimation = 256 + upper*256?). Needs more
   pcap captures at very low analog rates.

3. **Board revision 2+ behavior**: All 6 configs are from rev 1.2 (integer 1).
   The r0x13 and r0x12 formulas for rev 2+ are decompiled but not empirically
   validated.

4. **ADC_CONFIG value 2**: The decompiled code shows raw_divider==4 produces
   adc_config=2, but we have no pcap at 125MHz digital to validate this.

5. **Non-Pro device formulas**: The /5 step, ADC type selection, and clock
   constants are all device-type-specific. This analysis covers only Logic Pro 8
   (device type 6). Logic 4 (type 3) and Logic 8 (type 4) use different clocks
   and GPIF widths.
