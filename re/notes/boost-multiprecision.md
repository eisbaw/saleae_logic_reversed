# Boost.Multiprecision Usage in libgraph_server_shared.so

## Summary

Boost.Multiprecision is used exclusively for **128-bit integer arithmetic** on
timestamps represented in nanoseconds and picoseconds. It is NOT used for
arbitrary-precision math, cryptography, or any exotic computation. The only
assertion string from boost headers is `divide.hpp`, and it appears in a single
`eval_modulus` instantiation.

Rust's native `u128`/`i128` types are a direct replacement. No bignum crate is
needed.

## Evidence

### String evidence

Only one boost::multiprecision string survives in the binary:

```
/workspace/monorepo/build/graph-io/linux-x64/release/vcpkg_installed/
  x64-linux-libcxx-release/share/rapidjson/../../include/
  boost/multiprecision/cpp_int/divide.hpp
```

The vcpkg include path is routed through rapidjson's share directory (a vcpkg
packaging quirk, not meaningful).

### 128-bit compiler intrinsics

The binary uses these `__int128` compiler intrinsics (all statically linked
through PLT thunks):

| Intrinsic        | Purpose                     | Call sites |
|------------------|-----------------------------|------------|
| `__udivti3`      | Unsigned 128-bit division   | 18         |
| `__umodti3`      | Unsigned 128-bit modulus    | 2          |
| `__divti3`       | Signed 128-bit division     | 1          |
| `__modti3`       | Signed 128-bit modulus      | 1          |
| `__fixunsdfti`   | double -> unsigned 128-bit  | 24         |
| `__floatuntidf`  | unsigned 128-bit -> double  | 1          |

Total: 35 unique functions use 128-bit operations.

### Categorization of all 128-bit users

The 35 functions fall into 4 categories:

#### 1. Integer-to-string formatting (libc++/fmt)

Functions at ELF 0x1a0f900, 0x1a0fca0, 0x1a1cf80, 0x1a1dea0.

These convert 128-bit integers to decimal strings. They use `__udivti3` with
divisors 10000 and 100, writing digit pairs from a precomputed lookup table at
`DAT_004892f9` (the string
`"000102030405...9899"`). This is standard `{fmt}` or `std::to_string` for
`__int128` types.

**Callers**: general formatting utilities, not specific to device I/O.
**Rust equivalent**: `u128.to_string()` -- trivial.

#### 2. Filesystem timestamp (libc++)

Function `__last_write_time` at ELF 0x1b0ac50.

This is `std::filesystem::__last_write_time()` from libc++. It converts
`_FilesystemClock` time_points (128-bit nanosecond duration) to `timespec` for
`utimensat()`. Uses `__divti3` and `__modti3` to split 128-bit nanoseconds into
seconds + remainder.

```cpp
// Reconstructed logic:
seconds = nanosecond_timestamp / 1'000'000'000;
nsec    = nanosecond_timestamp % 1'000'000'000;
```

**Relevance to Saleae driver**: None. This is an internal libc++ implementation
detail for file modification times.

#### 3. GraphTime / SaleaeTime arithmetic

Functions at ELF 0x2db2bc0 (the FUN_02eb2bc0 `fmod`-like function) and
0x2db34e0 (`eval_modulus`).

This is the **only actual boost::multiprecision code path**. The function at
0x2db2bc0 converts timestamps between floating-point seconds and 128-bit
picosecond integers, then computes a modulus using boost's `eval_modulus`.

**Key constants** (resolved from RIP-relative loads):

| Address  | Value                | Meaning                      |
|----------|----------------------|------------------------------|
| 0x34d7f8 | 1,000,000,000.0      | Nanoseconds per second       |
| 0x34e210 | 1,000,000,000,000.0  | Picoseconds per second       |
| 0x34b970 | -0.0 (sign bit mask) | For absolute value via XOR   |
| 0x34e070 | 2^63                 | For unsigned 128-bit convert |

**Reconstructed algorithm** (0x2db2bc0):

```cpp
// Input: double seconds_value, struct { uint64_t unix_seconds; double fractional_ns; } *time_ref
// Returns: double (modulus result as seconds)

double compute_time_modulus(double period_seconds, TimeRef* ref) {
    // Convert time reference to 128-bit nanosecond count
    int128_t seconds_ns = (int128_t)ref->unix_seconds * 1'000'000'000;
    int128_t frac_ns = (int128_t)(ref->fractional_ns * 1e9);  // sub-second to ns
    int128_t dividend = seconds_ns + frac_ns;

    // Convert period to 128-bit picosecond count
    int128_t divisor = (int128_t)(period_seconds * 1e12);

    // Compute modulus (boost::multiprecision::eval_modulus)
    int128_t remainder = dividend % divisor;  // via eval_modulus at 0x2db34e0

    // Convert back to seconds
    return (double)remainder / 1e12;
}
```

**Callers** (3 functions):
- ELF 0x2d6faf0: `SimulatorSignalCsvRepeating::FindTrigger` -- MSO simulator
  signal trigger finding. Computes where in a repeating CSV waveform a trigger
  event occurs by taking the current time modulo the signal period.
- ELF 0x2d70090: Another SimulatorSignalCsvRepeating method (trigger search
  variant).
- ELF 0x2d70820: Another SimulatorSignalCsvRepeating method.

Source file (from assertion): `sim_signal_csv_repeating.cpp`

**Relevance to Saleae driver**: Low. This is the MSO oscilloscope simulator
signal implementation, not the capture pipeline. However, the `GraphTime` type
IS used throughout the graph system.

#### 4. RocksDB sample storage / graph data pipeline

20 functions in ELF range 0x1c89c40 - 0x1c994b0 use `__fixunsdfti` (double to
128-bit conversion). These all follow the same pattern:

```cpp
// Pseudocode pattern found in all 20 functions:
uint64_t unix_seconds = get_seconds(time_ref);
double frac = get_fractional(time_ref) * 1e9;  // to nanoseconds
int128_t seconds_ns = (int128_t)unix_seconds * 1'000'000'000;
int128_t frac_ns = (int128_t)abs(frac);
if (frac < 0) frac_ns = -frac_ns;
int128_t total_ns = seconds_ns + frac_ns;
```

These convert `GraphTime` timestamps to 128-bit nanosecond values for:
- Writing to RocksDB (sample data storage keys)
- Sample index-to-time conversion
- Time-to-sample-index conversion
- Time interval computation

**Callers**: graph data nodes, measurement engine, waveform persistence.

**Source assertion at 0x2d6faf0**:
```
sim_signal_csv_repeating.cpp line 0xf2 (242)
```

## GraphTime / SaleaeTime type

From Python binding docstrings and string analysis:

- `GraphTime` is the C++ internal type
- `SaleaeTime` is the Python-exposed type
- `SaleaeTimeDelta` represents time differences
- `SaleaeTimeInterval` represents time ranges
- `GraphTime(unixMs=...)` -- constructed from Unix milliseconds
- ISO 8601 output "with picosecond precision"
- `/minimumPulseWidthPicoseconds` -- used in trigger configuration

### Internal representation (reconstructed)

```
struct GraphTime {
    uint64_t  seconds;        // Unix epoch seconds (64-bit)
    double    fractional_ns;  // Sub-second component in nanoseconds
};
```

When precision is needed, this is converted to a 128-bit nanosecond count:
`total_ns = (i128)seconds * 1e9 + (i128)(fractional_ns)`

The dual representation (64-bit seconds + double nanoseconds) keeps most
operations fast (double arithmetic) while allowing exact 128-bit computation
when needed for modulus, comparison, or serialization.

## Implications for Rust implementation

### No bignum crate needed

All 128-bit operations use fixed 128-bit precision via `__int128`. This maps
directly to Rust's native `i128`/`u128` types, which compile to the same
`__udivti3`/`__umodti3` intrinsics.

### Recommended Rust type

```rust
/// High-precision timestamp for the graph system.
/// Internally stored as 128-bit nanoseconds from Unix epoch.
///
/// Max representable time: 2^127 ns ~ 5.4e21 years (effectively unlimited)
/// Resolution: 1 nanosecond (sub-nanosecond via GraphTime's double component)
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GraphTime {
    /// Nanoseconds since Unix epoch
    nanos: i128,
}

impl GraphTime {
    pub fn from_unix_ms(ms: u64) -> Self {
        Self { nanos: (ms as i128) * 1_000_000 }
    }

    pub fn as_seconds_f64(&self) -> f64 {
        (self.nanos as f64) / 1_000_000_000.0
    }

    pub fn as_picoseconds(&self) -> i128 {
        self.nanos * 1_000  // ns to ps
    }
}
```

For the modulus operation (used in simulator signal wrapping):
```rust
let remainder_ps = timestamp_ps % period_ps;  // native i128 % i128
```

### What we do NOT need

- `num-bigint` or `rug` crate -- 128-bit is sufficient
- `boost::multiprecision` equivalent -- Rust has native 128-bit
- Arbitrary precision -- the binary never uses more than 128 bits
- The `eval_modulus` from boost is literally just `a % b` for 128-bit values;
  Rust does this natively

## Function address table

All addresses are ELF VAs (subtract 0x100000 from Ghidra VAs).

### Direct 128-bit users (L1)

| ELF VA     | Category              | Intrinsics used              |
|------------|-----------------------|------------------------------|
| 0x2db34e0  | eval_modulus (boost)  | __umodti3                    |
| 0x2db2bc0  | Time modulus calc     | __fixunsdfti, __floatuntidf  |
| 0x1a0f900  | int128 to string      | __udivti3                    |
| 0x1a0fca0  | int128 to string      | __udivti3                    |
| 0x1a1cf80  | int128 to string      | __udivti3                    |
| 0x1a1dea0  | int128 to string      | __udivti3                    |
| 0x1b0ac50  | __last_write_time     | __divti3, __modti3           |
| 0x1c89c40  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c8bbf0  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c8bdb0  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c8e7d0  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c8eef0  | RocksDB write w/time  | __fixunsdfti                 |
| 0x1c90950  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c93ec0  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c941f0  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c949e0  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c94e00  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c956d0  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c95ef0  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c966f0  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c96a00  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c96f40  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c97270  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c97a50  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c97e60  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c98640  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c989b0  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c991b0  | GraphTime -> ns       | __fixunsdfti                 |
| 0x1c994b0  | GraphTime -> ns       | __fixunsdfti                 |

### Callers of 128-bit functions (L2)

| ELF VA     | Identified as                                  |
|------------|-------------------------------------------------|
| 0x2d6faf0  | SimulatorSignalCsvRepeating (MSO simulator)     |
| 0x2d70090  | SimulatorSignalCsvRepeating::FindTrigger variant |
| 0x2d70820  | SimulatorSignalCsvRepeating variant              |
| 0x1b77e20  | Graph data pipeline                              |
| 0x1b7b120  | Graph data pipeline                              |
| 0x1b867c0  | Graph data pipeline                              |
| 0x1b8e650  | Graph data pipeline                              |
| 0x1c8ac30  | RocksDB write with mutex lock                   |
| 0x1c8e220  | Graph data write                                 |
| 0x1c93c40  | Measurement formatting                           |
| 0x1c93cd0  | Measurement formatting                           |
| 0x1c93d30  | Measurement formatting                           |
| 0x1c93dc0  | Measurement formatting                           |
| 0x1c93e90  | Measurement formatting                           |
| 0x1c96d10  | Measurement formatting                           |
| 0x1c96d90  | Measurement formatting                           |
| 0x1c96de0  | Measurement formatting                           |
| 0x1c96e50  | Measurement formatting                           |
| 0x1c96f10  | Measurement formatting                           |
| 0x1c97d60  | Measurement formatting                           |
| 0x1c94cf0  | Measurement formatting                           |
