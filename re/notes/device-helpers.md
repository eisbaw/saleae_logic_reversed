# DeviceHelpers.cpp: Complete Function Catalog

Analysis date: 2026-03-10
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0076
Source path: `/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceHelpers.cpp`
Method: String xref tracing, objdump disassembly, Ghidra headless decompilation


## Summary

DeviceHelpers.cpp is a small utility module containing two tightly coupled
functions for computing mixed-mode capture parameters. The file contains
exactly **2 named functions** (confirmed by assertion string xrefs), plus
one assertion-handler fragment that Ghidra splits out as a separate function
due to compiler code layout.

The module is responsible for:
1. Computing digital/analog interleave ratios for mixed-mode captures
2. Computing total samples collected from a byte count

These functions are called by Logic2FpgaDevice.cpp, DeviceSettings.cpp,
and logic_device_node.cpp -- the key callers in the capture setup path.


## Function Catalog

### DeviceCalculateMixRate

| Property | Value |
|----------|-------|
| ELF VA | 0x1c19fa0 |
| Ghidra VA | 0x1d19fa0 |
| Size | 317 bytes (0x1c19fa0 - 0x1c1a0dd) |
| Source Line | Starts around line 1, assertions at lines 13 and 49 |

**Signature** (from assertion string):
```cpp
void DeviceCalculateMixRate(
    U32 digital_multiplier,     // param_1 (edi) - per-sample digital count
    U32 analog_multiplier,      // param_2 (esi) - per-sample analog count
    U32 digital_sample_rate,    // param_3 (edx) - digital SR in Hz
    U32 analog_sample_rate,     // param_4 (ecx) - analog SR in Hz
    U32 gpif_width,             // param_5 (r8d) - GPIF bus width: 16 or 32
    U32 adc_bits_per_sample,    // param_6 (r9d) - ADC bit depth: 10 or 12
    U32 &out_digital_mixed,     // param_7 (stack) - output: digital samples per cycle
    U32 &out_analog_mixed,      // param_8 (stack) - output: analog samples per cycle
    U32 &out_digital_quotient,  // param_9 (stack) - output: digital ratio
    U32 &out_analog_quotient    // param_10 (stack) - output: analog ratio
)
```

**Algorithm:**

1. **Validation**: Assert `(digital_sample_rate | analog_sample_rate) != 0`
   ("At least one sample rate must be set", line 13)

2. **Determine ADC packing parameters** based on GPIF width and ADC bits:

   | GPIF Width | ADC Bits | analog_bytes | samples_per_group | Meaning |
   |------------|----------|--------------|-------------------|---------|
   | 16 (0x10)  | 10       | 8            | 5                 | 10-bit ADC, 5 bytes per 8 samples |
   | 16 (0x10)  | 12       | 4            | 3                 | 12-bit ADC, 3 bytes per 4 samples |
   | 32 (0x20)  | 12       | 8            | 3                 | 12-bit ADC, 3 bytes per 8 samples |
   | other      | other    | --           | --                | Assert fail: "No valid analog and gpif width combo given" (line 49) |

3. **Three operating modes:**

   a. **Analog-only** (`analog_sample_rate == 0`):
      - `out_digital_mixed = digital_multiplier`
      - `out_analog_mixed = 0`
      - `out_digital_quotient = 1`
      - `out_analog_quotient = 0`

   b. **Digital-only** (`digital_sample_rate == 0`):
      - `out_analog_mixed = samples_per_group * analog_multiplier`
      - `out_digital_mixed = 0`
      - `out_analog_quotient = 1`
      - `out_digital_quotient = 0`

   c. **Mixed mode** (both rates nonzero):
      ```
      digital_reduced = digital_sample_rate / gpif_width
      analog_reduced  = analog_sample_rate / analog_bytes
      g = Gcd(digital_reduced, analog_reduced)
      digital_quotient = digital_reduced / g
      analog_quotient  = analog_reduced / g

      out_digital_mixed    = digital_multiplier * digital_quotient
      out_analog_mixed     = samples_per_group * analog_multiplier * analog_quotient
      out_digital_quotient = digital_quotient
      out_analog_quotient  = analog_quotient
      ```

**Callers** (5 direct CALL sites):

| Caller ELF VA | Source File | Context |
|---------------|-------------|---------|
| 0x1c1a169 | DeviceHelpers.cpp | Internal: called by DeviceCalculateSamplesCollected |
| 0x1be7ce7 | logic_device_node.cpp | Capture setup in device node |
| 0x1c24fb9 | Logic2FpgaDevice.cpp | FPGA device capture config |
| 0x1c26c80 | Logic2FpgaDevice.cpp | FPGA device capture config (different path) |
| 0x1c4a6f9 | DeviceSettings.cpp | Device settings computation |


### DeviceCalculateSamplesCollected

| Property | Value |
|----------|-------|
| ELF VA | 0x1c1a130 |
| Ghidra VA | 0x1d1a130 |
| Size | 329 bytes (0x1c1a130 - 0x1c1a279) |
| Source Line | Assertion at line 136 |

**Signature** (from assertion string):
```cpp
U64 DeviceCalculateSamplesCollected(
    U32 digital_multiplier,     // param_1 (edi)
    U32 analog_multiplier,      // param_2 (esi)
    U32 digital_sample_rate,    // param_3 (edx)
    U32 analog_sample_rate,     // param_4 (ecx)
    U32 gpif_width,             // param_5 (r8d) - 16 or 32
    U32 adc_bits_per_sample,    // param_6 (r9d) - 10 or 12
    U64 total_bytes             // param_7 (stack) - total bytes transferred
) -> U64                        // total samples collected
```

**Algorithm:**

1. Call `DeviceCalculateMixRate` with the same first 6 parameters to get
   `dig_mixed`, `ana_mixed`, `dig_quotient`, `ana_quotient`.

2. Compute bytes per interleave cycle:
   ```
   bytes_per_cycle = (gpif_width / 8) * (dig_mixed + ana_mixed)
   ```

3. Compute number of complete interleave cycles:
   ```
   num_cycles = total_bytes / bytes_per_cycle
   ```
   (Uses 32-bit fast path when total_bytes < 2^32.)

4. Compute digital sample count:
   ```
   bytes_per_digital_sample = (gpif_width == 32) ? 8 : 4
   digital_samples = dig_quotient * bytes_per_digital_sample * num_cycles
   ```

5. If analog is enabled (`digital_sample_rate != 0`):
   ```
   analog_samples = num_cycles * ana_quotient * gpif_width
   ```
   If both digital and analog are enabled (`analog_multiplier != 0`):
   ```
   lcm_rate = Lcm(digital_sample_rate, analog_sample_rate)
   analog_samples *= lcm_rate / digital_sample_rate
   ```
   Assert consistency: `analog_samples == digital_samples * (lcm_rate / analog_sample_rate)`
   ("alg problem", line 136)

6. Return the final sample count.

**Callers**: 0 direct CALL sites.

This function has zero external callers. It is either:
- Dead code retained by the linker
- Called through a function pointer or template mechanism
- Inlined at call sites (unlikely given its 329-byte size)

The most likely explanation is that the function exists as a compiled
out-of-line copy but is effectively unused in the current binary. Its
logic is useful for understanding the relationship between byte counts
and sample counts in mixed-mode captures, even if it's not actively
called at runtime.


### Assertion Fragment (compiler-separated)

| Property | Value |
|----------|-------|
| ELF VA | 0x1c1a0dd |
| Ghidra VA | 0x1d1a0dd |
| Size | 80 bytes |

Ghidra identified this as a separate function, but it is actually the
cold path (assertion failure) for DeviceCalculateMixRate, separated by
the compiler for better instruction cache utilization. It constructs
assertion parameters and calls `FUN_02eca300` (the assertion handler).

It handles two assertion paths:
- Line 13: "At least one sample rate must be set"
- Line 49: "No valid analog and gpif width combo given"


## Assertion Metadata

Both functions use the same assertion handler at ELF VA 0x2dca300 with
the standard Saleae assertion format.

### Source Line Numbers

| Function | Line | Assertion Message |
|----------|------|-------------------|
| DeviceCalculateMixRate | 13 (0x0d) | "At least one sample rate must be set" |
| DeviceCalculateMixRate | 49 (0x31) | "No valid analog and gpif width combo given" |
| DeviceCalculateSamplesCollected | 136 (0x88) | "alg problem" |

The source file is 74 characters: `/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceHelpers.cpp`

The function name strings confirm the exact C++ signatures:
- `"void DeviceCalculateMixRate(U32, U32, U32, U32, U32, U32, U32 &, U32 &, U32 &, U32 &)"` (85 chars)
- `"U64 DeviceCalculateSamplesCollected(U32, U32, U32, U32, U32, U32, U64)"` (70 chars)


## Module Boundaries

Functions surrounding DeviceHelpers.cpp in the binary:

| ELF VA | Size | Description | Source File |
|--------|------|-------------|-------------|
| 0x1c19800 | 218 | std::map erase (tree node delete) | STL (not DeviceHelpers) |
| 0x1c198e0 | 383 | std::vector push_back (reallocation) | STL (not DeviceHelpers) |
| 0x1c19ab0 | 15 | shared_ptr destructor | STL (not DeviceHelpers) |
| 0x1c19ac0 | 28 | shared_ptr destructor + delete | STL (not DeviceHelpers) |
| 0x1c19ae0 | 10 | Virtual dispatch thunk | STL/infra (not DeviceHelpers) |
| 0x1c19b00 | 112 | Object constructor helper | STL/infra (not DeviceHelpers) |
| 0x1c19ba0 | 1 | Empty function (noop) | Unknown |
| 0x1c19bb0 | 239 | std::map emplace (tree node insert) | STL (not DeviceHelpers) |
| 0x1c19cc0 | 300 | Logging/debug output function | Possibly LogicAnalyzerDevice |
| 0x1c19f90 | 15 | Path thunk: prepends "/id" | Possibly LogicAnalyzerDevice |
| **0x1c19fa0** | **317** | **DeviceCalculateMixRate** | **DeviceHelpers.cpp** |
| **0x1c1a0dd** | **80** | **Assertion cold path for MixRate** | **DeviceHelpers.cpp** |
| **0x1c1a130** | **329** | **DeviceCalculateSamplesCollected** | **DeviceHelpers.cpp** |
| 0x1c1a280 | 1 | Single ret (empty function) | Unknown |
| 0x1c1a290 | 191 | Logic16SampleRateSettings constructor | Logic16Device.cpp |
| 0x1c1a3a0 | 973 | Logic16 rate validation | Logic16Device.cpp |

The DeviceHelpers.cpp translation unit occupies a compact region from
0x1c19fa0 to 0x1c1a279 (approximately 729 bytes, including the separated
assertion fragment). It is bounded by STL infrastructure functions above
and Logic16Device.cpp functions below.


## Cross-Reference with Prior Work

| Prior Task | Relationship | Status |
|------------|-------------|--------|
| TASK-0069 | Disaggregator: DeviceCalculateMixRate documented | Confirmed and extended |
| TASK-0075 | ComputationUtilities: Gcd (0x1c47140) called by MixRate | Confirmed |
| TASK-0075 | ComputationUtilities: Lcm (0x1c47170) called by SamplesCollected | Confirmed |
| TASK-0090 | SampleRateOption: rates fed to these functions | Cross-referenced |
| TASK-0073 | Multi-device: GPIF width varies by device (16 or 32) | Confirmed |

The disaggregator-interleaving.md (TASK-0069) already documented
DeviceCalculateMixRate at a high level. This analysis provides:
- Complete parameter identification with register mapping
- The exact algorithm with all three modes (analog-only, digital-only, mixed)
- Full documentation of DeviceCalculateSamplesCollected (not previously covered)
- Complete caller analysis for both functions
- Module boundary mapping


## Relevance to Rust Implementation

### DeviceCalculateMixRate

This function computes how to interleave digital and analog data in the
FPGA data stream. For the Rust driver:

```rust
/// Compute the interleave ratio for mixed-mode captures.
///
/// Returns (digital_mixed, analog_mixed, digital_quotient, analog_quotient).
fn device_calculate_mix_rate(
    digital_multiplier: u32,
    analog_multiplier: u32,
    digital_sample_rate: u32,
    analog_sample_rate: u32,
    gpif_width: u32,       // 16 or 32
    adc_bits: u32,         // 10 or 12
) -> (u32, u32, u32, u32) {
    assert!(digital_sample_rate | analog_sample_rate != 0,
            "At least one sample rate must be set");

    let (analog_bytes, samples_per_group) = match (gpif_width, adc_bits) {
        (16, 10) => (8u32, 5u32),
        (16, 12) => (4, 3),
        (32, 12) => (8, 3),
        _ => panic!("No valid analog and gpif width combo given"),
    };

    match (analog_sample_rate, digital_sample_rate) {
        (0, _) => (digital_multiplier, 0, 1, 0),
        (_, 0) => (0, samples_per_group * analog_multiplier, 0, 1),
        _ => {
            let dig_reduced = digital_sample_rate / gpif_width;
            let ana_reduced = analog_sample_rate / analog_bytes;
            let g = gcd(dig_reduced, ana_reduced);
            let dq = dig_reduced / g;
            let aq = ana_reduced / g;
            (
                digital_multiplier * dq,
                samples_per_group * analog_multiplier * aq,
                dq,
                aq,
            )
        }
    }
}
```

### DeviceCalculateSamplesCollected

This function converts raw byte counts to sample counts. Even though it
has zero direct callers in the current binary, it documents the exact
relationship between bytes and samples in mixed-mode captures:

```rust
fn device_calculate_samples_collected(
    digital_multiplier: u32,
    analog_multiplier: u32,
    digital_sample_rate: u32,
    analog_sample_rate: u32,
    gpif_width: u32,
    adc_bits: u32,
    total_bytes: u64,
) -> u64 {
    let (dig_mixed, ana_mixed, dig_quot, ana_quot) =
        device_calculate_mix_rate(
            digital_multiplier, analog_multiplier,
            digital_sample_rate, analog_sample_rate,
            gpif_width, adc_bits,
        );

    let bytes_per_cycle = (gpif_width / 8) as u64 * (dig_mixed + ana_mixed) as u64;
    let num_cycles = total_bytes / bytes_per_cycle;

    let dig_sample_bytes = if gpif_width == 32 { 8u64 } else { 4 };
    let digital_samples = dig_quot as u64 * dig_sample_bytes * num_cycles;

    if digital_sample_rate == 0 {
        return digital_samples;
    }

    let mut analog_samples = num_cycles * ana_quot as u64 * gpif_width as u64;
    if analog_multiplier != 0 {
        let l = lcm(digital_sample_rate, analog_sample_rate);
        analog_samples *= l as u64 / digital_sample_rate as u64;
        debug_assert_eq!(
            analog_samples,
            digital_samples * (l as u64 / analog_sample_rate as u64),
            "alg problem"
        );
    }

    analog_samples
}
```


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| DeviceCalculateMixRate full signature | **Confirmed** | Assertion string with exact C++ signature |
| DeviceCalculateSamplesCollected full signature | **Confirmed** | Assertion string with exact C++ signature |
| GPIF/ADC packing parameters (8/5, 4/3, 8/3) | **Confirmed** | Disassembly: immediate comparisons and MOV values |
| GCD-based interleave ratio computation | **Confirmed** | Ghidra decompilation + cross-ref with Gcd at 0x1c47140 |
| LCM-based consistency check in SamplesCollected | **Confirmed** | Ghidra decompilation + cross-ref with Lcm at 0x1c47170 |
| Three modes (digital-only, analog-only, mixed) | **Confirmed** | Three separate code paths with distinct output patterns |
| Module has exactly 2 named functions | **Confirmed** | Only 2 xrefs to DeviceHelpers.cpp string in entire binary |
| SamplesCollected has 0 external callers | **Confirmed** | Full E8/E9 scan of .text section |
| Source line numbers (13, 49, 136) | **Confirmed** | Immediate values in assertion call setup |
| Callers of MixRate from 3 source files | **Confirmed** | E8 call scan + source path strings near callers |
| Functions before 0x1c19fa0 are NOT DeviceHelpers | **High** | Decompilation shows STL infrastructure patterns |
| Assertion fragment at 0x1c1a0dd is compiler-separated | **High** | Code flow from MixRate jumps into this block |

## Address Summary

| ELF VA | Ghidra VA | Function | Size |
|--------|-----------|----------|------|
| 0x1c19fa0 | 0x1d19fa0 | DeviceCalculateMixRate | 317 |
| 0x1c1a0dd | 0x1d1a0dd | Assertion fragment (MixRate cold path) | 80 |
| 0x1c1a130 | 0x1d1a130 | DeviceCalculateSamplesCollected | 329 |

Total attributed code: ~726 bytes across 3 function bodies.
