# ComputationUtilities.cpp: Complete Function Catalog

Analysis date: 2026-03-10
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0075
Source path: `/workspace/monorepo/graph-io/devices/logic_interface/src/ComputationUtilities.cpp`
Method: String xref tracing, objdump disassembly, Ghidra headless decompilation


## Summary

ComputationUtilities.cpp is a helper module containing small arithmetic and
selection utility functions used throughout the device layer. The file
contains 18 functions spanning ELF VA 0x1c46900 through 0x1c47340. Most are
small (16-112 bytes), with the two SelectClosestIndex variants being the
largest (~650 bytes each due to std::map usage).

Only 4 functions contain assertions that reference the source file path.
The remaining functions are attributed by address proximity and functional
coherence. Many of these small functions have zero direct callers because
the compiler inlines them at call sites, keeping out-of-line copies in the
object file.


## Function Catalog

### Rounding and Precision

| ELF VA | Size | Signature | Description |
|--------|------|-----------|-------------|
| 0x1c46900 | 64 | `double RoundTowardZero(double)` | Rounds toward zero: adds 0.5 then calls ceil (negative) or floor (positive). Returns 0.0 for exact zeros. |
| 0x1c46940 | 32 | `double Ceil(double)` | Ceiling function wrapper. Calls `ceil()`, then zeroes out exact 0.0 results (avoids -0.0). |
| 0x1c46960 | 112 | `bool IsInteger(double)` | Tests if a double is (approximately) an integer. Rounds toward zero, takes absolute difference, compares against small epsilon. Used for divisibility validation (e.g., "DAT / rate must be integer" in sample rate setup). |

### Clamping

| ELF VA | Size | Signature | Description |
|--------|------|-----------|-------------|
| 0x1c469d0 | 32 | `U8 ClampU8(int)` | Clamps signed int to [0, 255]. Uses two CMOVs: `min(val, 255)` then `max(result, 0)`. |

### Distance and Absolute Value

| ELF VA | Size | Signature | Description |
|--------|------|-----------|-------------|
| 0x1c469f0 | 64 | `double Distance2D(double x0, double y0, double x1, double y1)` | Euclidean distance: `sqrt((x1-x0)^2 + (y1-y0)^2)`. SSE-optimized with packed subtract, multiply, horizontal add. Falls through to `sqrt@plt` for negative inputs (NaN propagation). |
| 0x1c46a30 | 32 | `U64 AbsDiff(U64, U64)` | Unsigned 64-bit absolute difference. Uses SUB + NEG + CMOVB. |
| 0x1c46a50 | 16 | `U32 AbsDiff(U32, U32)` | Unsigned 32-bit absolute difference. Same pattern, 32-bit registers. |
| 0x1c46a60 | 16 | `I32 Abs(I32)` | Signed 32-bit absolute value. NEG + CMOVL. |
| 0x1c46a70 | 16 | `I64 Abs(I64)` | Signed 64-bit absolute value. NEG + CMOVL (REX.W). |
| 0x1c46a80 | 48 | `double Abs(double)` | Double absolute value. SSE bit manipulation: XOR with sign-bit mask, then blend using ANDPD/ANDNPD/ORPD. |

### Selection / Search

| ELF VA | Size | Signature | Description |
|--------|------|-----------|-------------|
| 0x1c46ab0 | 656 | `U32 SelectClosestIndex(std::vector<U64>&, U64)` | Finds the index of the element closest to the target value. Uses a std::map (red-black tree) internally to sort candidates by absolute difference, then walks the tree to find the minimum. Asserts vector is non-empty ("selections can not be empty"). Assertion at line 0x6c (108). |
| 0x1c46d40 | 640 | `U32 SelectClosestIndex(std::vector<U32>&, U32)` | Same algorithm as above but for 32-bit values. Assertion at line 0x8a (138). |

### Random Number Generation

| ELF VA | Size | Signature | Description |
|--------|------|-----------|-------------|
| 0x1c46fc0 | 48 | `void SeedRandom()` | One-time random seed initialization. Calls `time(0)` then `srand()`. Uses a static bool flag (at BSS 0x2f7aaa8) to ensure single initialization. |
| 0x1c46ff0 | 80 | `double RandomInRange(double low, double high)` | Returns a random double in [low, high). Computes: `low + (rand() / RAND_MAX) * (high - low)`. |
| 0x1c47040 | 64 | `long RandomSelect(int low, int high)` | Returns a random integer in [low, high). 32-bit input version. Calls `rand()`, scales to range. |
| 0x1c47080 | 192 | `long RandomSelect(long low, long high)` | Returns a random integer in [low, high). 64-bit input version. Uses integer-to-double conversion for range scaling. |

### Number Theory (GCD / LCM)

| ELF VA | Size | Signature | Description |
|--------|------|-----------|-------------|
| 0x1c47140 | 48 | `U32 Gcd(U32 a, U32 b)` | Euclidean GCD algorithm. If a==0, returns b. Loop: swap a,b while b = a mod b. No assertions (too simple to fail). Called by DeviceCalculateMixRate (0x1c19fa0 in DeviceHelpers.cpp). |
| 0x1c47170 | 208 | `U32 Lcm(U32 a, U32 b)` | LCM via GCD: `(a / gcd(a,b)) * b`. Two assertions: (1) "LCM with at least one zero input." at line 0xd9 (217) if either input is 0. (2) "LCM math function exceded U32 size." at line 0xe2 (226) if result overflows 32 bits (checks via `jo` / overflow flag). Called by DeviceCalculateSamplesCollected (0x1c1a130 in DeviceHelpers.cpp). |
| 0x1c47240 | 256 | `U32 LcmWithQuotients(U32 a, U32 b, U32* qa, U32* qb)` | Computes LCM and also returns the quotients `lcm/a` and `lcm/b` via output pointers. If a==0, sets `*qa = 0` and returns b. If b==0, sets `*qb = 0` and returns a. Null output pointers are handled (uses local stack variable as scratch). Same overflow assertion as Lcm. |


## Assertion Metadata

The assertion function (at 0x2dca300 via PLT thunk) takes these parameters:

```
FUN_02eca300(
    const char* condition,      // "false" (assertion expression)
    int condition_len,          // 5
    const char* function_name,  // e.g. "U32 Lcm(U32, U32)"
    int function_name_len,      // e.g. 0x11 (17)
    const char* source_file,    // "/workspace/monorepo/.../ComputationUtilities.cpp"
    int source_file_len,        // 0x51 (81)
    int line_number,            // e.g. 0xd9 (217)
    const char* message,        // e.g. "LCM with at least one zero input."
    int message_len             // e.g. 0x21 (33)
)
```

### Source Line Numbers

| Function | Line Number(s) | Assertion Message |
|----------|----------------|-------------------|
| SelectClosestIndex(U64) | 108 (0x6c) | "selections can not be empty" |
| SelectClosestIndex(U32) | 138 (0x8a) | "selections can not be empty" |
| Lcm | 217 (0xd9), 226 (0xe2) | "LCM with at least one zero input.", "LCM math function exceded U32 size." |
| LcmWithQuotients | 226 (0xe2) | "LCM math function exceded U32 size." |

The line gap between SelectClosestIndex (108-138) and Lcm (217-226) suggests
there is likely source code for the smaller utility functions (Abs, AbsDiff,
ClampU8, Distance2D, Random*) between lines ~140-216, but since those functions
have no assertions, we cannot confirm their exact line positions.


## Caller Analysis

| Function | Direct Callers (E8 near call) | Notes |
|----------|-------------------------------|-------|
| Gcd | 1: DeviceCalculateMixRate (0x1c19fa0, DeviceHelpers.cpp) | Computes interleave ratios |
| Lcm | 1: DeviceCalculateSamplesCollected (0x1c1a130, DeviceHelpers.cpp) | Computes total samples from capture params |
| All others | 0 | Likely inlined by the compiler at all call sites |

The zero-caller count for most functions is expected. These are small utility
functions (many under 50 bytes) defined in a `.cpp` file. The compiler:
1. Inlines them at every call site (they are small enough)
2. Emits one out-of-line copy in the `.o` file (required for ODR compliance)
3. The linker keeps the out-of-line copy even though nothing calls it directly

GCD and LCM are large enough (contain loops) that the compiler chose NOT to
inline them, hence they retain direct callers.


## Cross-Reference with Prior Work

| Prior Task | Function | Status |
|------------|----------|--------|
| TASK-0070 | ComputeTransferSize (NOT in ComputationUtilities.cpp) | Already documented. Source: LogicAnalyzerDevice.cpp |
| TASK-0069 | DeviceCalculateMixRate (calls Gcd from ComputationUtilities) | Already documented. Source: DeviceHelpers.cpp |
| TASK-0090 | IsInteger (inlined in GetDeviceSettings for rate divisibility checks) | Confirmed usage pattern |

The IsInteger function is used in GetDeviceSettings (LogicGraduateDevice at
0x1c3e0a0, LogicProfessionalDevice at 0x1c418c0) for the divisibility
validation: "DAT / rate must be an integer". The compiler inlines it at
those call sites (confirmed by `floor@plt` calls at 0x1c3e363, 0x1c3e3c2,
0x1c41b56, 0x1c41bb5).

SelectClosestIndex is likely used when selecting the "closest valid sample
rate" from the available rate arrays. The fact that it has zero direct
callers suggests it is either:
- Inlined despite its size (~650 bytes -- unlikely)
- Called via a function pointer or template instantiation that results in indirect calls
- Dead code that was compiled but never actually used at runtime

Given that the function is 650 bytes with complex std::map operations, inlining
is unlikely. The most probable explanation is that it is dead code or called
through an indirect mechanism not detectable by simple E8 scanning.


## Relevance to Rust Implementation

For the Rust driver implementation, these utility functions correspond to:

| Binary Function | Rust Equivalent | Notes |
|----------------|-----------------|-------|
| Gcd | `num::integer::gcd()` or manual Euclidean | Already used in interleave ratio computation |
| Lcm | `num::integer::lcm()` | Check for U32 overflow |
| LcmWithQuotients | Custom: `let l = lcm(a,b); (l, l/a, l/b)` | Returns quotients alongside LCM |
| IsInteger | `(rate % divisor) == 0` or `f64::fract() < epsilon` | Used in rate validation |
| SelectClosestIndex | `vec.iter().enumerate().min_by_key(\|&(_, &v)\| v.abs_diff(target))` | Rust iterators make this trivial |
| AbsDiff | `u32::abs_diff()` / `u64::abs_diff()` | Stable since Rust 1.60 |
| ClampU8 | `(val as i32).clamp(0, 255) as u8` | Built-in clamp |
| Distance2D | `((x1-x0).powi(2) + (y1-y0).powi(2)).sqrt()` | Probably not needed for the driver |
| Random* | `rand` crate | Probably not needed for the driver |
| RoundTowardZero | `f64::trunc()` | Built-in |


## Address Summary

All functions in the ComputationUtilities cluster:

| ELF VA | Ghidra VA | Function |
|--------|-----------|----------|
| 0x1c46900 | 0x1d46900 | RoundTowardZero(double) |
| 0x1c46940 | 0x1d46940 | Ceil(double) |
| 0x1c46960 | 0x1d46960 | IsInteger(double) |
| 0x1c469d0 | 0x1d469d0 | ClampU8(int) |
| 0x1c469f0 | 0x1d469f0 | Distance2D(double, double, double, double) |
| 0x1c46a30 | 0x1d46a30 | AbsDiff(U64, U64) |
| 0x1c46a50 | 0x1d46a50 | AbsDiff(U32, U32) |
| 0x1c46a60 | 0x1d46a60 | Abs(I32) |
| 0x1c46a70 | 0x1d46a70 | Abs(I64) |
| 0x1c46a80 | 0x1d46a80 | Abs(double) |
| 0x1c46ab0 | 0x1d46ab0 | SelectClosestIndex(vector<U64>&, U64) |
| 0x1c46d40 | 0x1d46d40 | SelectClosestIndex(vector<U32>&, U32) |
| 0x1c46fc0 | 0x1d46fc0 | SeedRandom() |
| 0x1c46ff0 | 0x1d46ff0 | RandomInRange(double, double) |
| 0x1c47040 | 0x1d47040 | RandomSelect(int, int) |
| 0x1c47080 | 0x1d47080 | RandomSelect(long, long) |
| 0x1c47140 | 0x1d47140 | Gcd(U32, U32) |
| 0x1c47170 | 0x1d47170 | Lcm(U32, U32) |
| 0x1c47240 | 0x1d47240 | LcmWithQuotients(U32, U32, U32*, U32*) |


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| SelectClosestIndex signatures (both overloads) | Confirmed | Assertion strings with full C++ signatures |
| Lcm signature and overflow check | Confirmed | Assertion strings + decompilation |
| LcmWithQuotients computes LCM + quotients | Confirmed | Ghidra decompilation shows `lcm/a` and `lcm/b` stored via output ptrs |
| Gcd is Euclidean algorithm | Confirmed | Disassembly: classic div/mod loop |
| IsInteger checks divisibility | High | floor/ceil calls + epsilon comparison pattern; inlined in rate validation |
| Small functions (Abs, AbsDiff, ClampU8) | High | Disassembly patterns are unambiguous single-purpose arithmetic |
| Distance2D computes Euclidean distance | High | SSE subtract-square-sum-sqrt pattern |
| Random functions use rand()/srand() | High | PLT calls to rand, srand, time |
| All 18 functions belong to ComputationUtilities.cpp | Medium | Address proximity (contiguous 0x1c46900-0x1c47340), functional coherence, but only 4 have explicit source-file assertions |
| Zero-caller functions are compiler out-of-line copies | Medium | Consistent with small-function inlining behavior, but could also be dead code |
