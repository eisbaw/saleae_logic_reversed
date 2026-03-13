# GPIF Width Selection and ContractReadData Divisor Logic

Analysis date: 2026-03-13
Binary: libgraph_server_shared.so v2.4.40
Task: TASK-0199
Method: Ghidra headless decompilation of four functions


## Summary

Two mechanisms control how EP2 bulk data is compacted before disaggregation:

1. **GPIF Width** (16 or 32 bits): Determines the logical word size in the
   interleaved data stream. Set per device type, not per capture.

2. **LowSpeedDivider** (1, 2, 4, or 8): Determines how many bytes are
   stripped by ContractReadData. Set per sample rate.


## GPIF Width Selection

### Source

`DeviceSettings.cpp`, caller of `DeviceCalculateMixRate` at Ghidra VA
0x1d4a6f9 (ELF VA 0x1c4a6f9), line 2204 of decompiled output:

```c
param_5 = ((device_type & 0xfffffffe) == 6) * 0x10 + 0x10;
```

### Algorithm

The GPIF width (param_5 to DeviceCalculateMixRate) is computed from the
external device type ID:

| device_type_id | (id & ~1) == 6 | GPIF Width |
|----------------|-----------------|------------|
| 0 (Logic)      | 0 == 6? No      | 16 (0x10)  |
| 1 (Logic16)    | 0 == 6? No      | 16 (0x10)  |
| 4 (Logic4)     | 4 == 6? No      | 16 (0x10)  |
| 5 (Logic8)     | 4 == 6? No      | 16 (0x10)  |
| 6 (LogicPro8)  | 6 == 6? Yes     | 32 (0x20)  |
| 7 (LogicPro16) | 6 == 6? Yes     | 32 (0x20)  |

**Result**: Pro devices (type_id 6 and 7) use 32-bit GPIF. All others
use 16-bit. This is a static per-device property, NOT dependent on
analog channel count.

### ADC Bits

In the same DeviceSettings caller (line 2207), ADC bits are hardcoded:

```c
param_6 = 0xc;  // Always 12-bit ADC
```

This means the `(16, 10) -> 8/5` entry in the ADC packing table (for
10-bit HMCAD1100 ADC) is only reachable from other callers of
DeviceCalculateMixRate (Logic2FpgaDevice.cpp callers), not from the
DeviceSettings path. The DeviceSettings path always assumes 12-bit ADC.

**Confidence: Confirmed** -- Direct decompilation of DeviceSettings caller.


## LowSpeedDivider Selection

### Source

`LogicDevice::StartSampling(DeviceSettings &)` at Ghidra VA 0x1d3ad40
(ELF VA 0x1c3ad40), size 1252 bytes.

### Algorithm

```
sample_rate = GetDigitalSampleRate(settings)

if sample_rate < 200,000:
    // Low-speed mode: FPGA samples at a fixed rate, pads with zeros.
    // Host strips the padding with ContractReadData.
    switch (sample_rate):
        case 25,000:   divider = 8   // keep every 8th byte
        case 50,000:   divider = 4   // keep every 4th byte
        case 100,000:  divider = 2   // keep every 2nd byte
        default:       ASSERT("invalid sample rate")

    // Also sets param_1[0x33] = 0xef (digital divider register to 0xef)
    // 0xef = 239; unclear exactly what this means but it's a fixed
    // value for all three low-speed modes.

else:
    // High-speed mode: FPGA outputs real data at full width.
    ASSERT(48,000,000 % sample_rate == 0)
    divider = 1                              // no contraction
    digital_divider_reg = 48000000 / sample_rate - 1
```

### Selection Table

| Sample Rate  | LowSpeedDivider | Effective EP2 Rate | Digital Divider Reg |
|-------------|-----------------|--------------------|--------------------|
| 25 kSa/s    | 8               | 200 kB/s           | 0xef               |
| 50 kSa/s    | 4               | 200 kB/s           | 0xef               |
| 100 kSa/s   | 2               | 200 kB/s           | 0xef               |
| 200 kSa/s   | 1               | 200 kB/s           | 239 (48M/200k-1)   |
| 500 kSa/s   | 1               | 500 kB/s           | 95 (48M/500k-1)    |
| 1 MSa/s     | 1               | 1 MB/s             | 47                 |
| 2 MSa/s     | 1               | 2 MB/s             | 23                 |
| ...         | 1               | ...                | ...                |
| 12 MSa/s    | 1               | 12 MB/s            | 3                  |
| 24 MSa/s    | 1               | 24 MB/s            | 1                  |
| 48 MSa/s    | 1               | 48 MB/s            | 0                  |

The 48 MHz clock is the FPGA's digital sampling clock for non-Pro devices
(LogicDevice base class path). Pro devices override StartSampling in the
Logic2FpgaDevice subclass and use a different clock (200 MHz).

### Physical Interpretation

At low sample rates (< 200 kSa/s), the FPGA still outputs data at its
minimum transfer rate but fills most bytes with padding. The host-side
ContractReadData strips the padding in-place before disaggregation:

```
EP2 raw:     [D0 P1 P2 P3 P4 P5 P6 P7] [D1 P1 P2 P3 P4 P5 P6 P7] ...
                                          (divider=8: keep every 8th byte)
After:       [D0 D1 D2 D3 ...]
```

This means the FPGA USB transfer rate doesn't drop below a minimum
throughput, even at very low sample rates. The padding ensures the USB
bulk endpoint stays active and responsive.

**Confidence: Confirmed** -- Direct decompilation of
`LogicDevice::StartSampling`.


## ContractReadData Algorithm (Recap)

Source: `LogicDevice::ContractReadData(U8 *)` at Ghidra VA 0x1d3bad0
(ELF VA 0x1c3bad0), 383 bytes. Source: LogicDevice.cpp line 0x16e.

```
divider = this->0x19c   // LowSpeedDivider
buf_size = this->0x1a0

switch (divider):
    case 1:  return  // no-op
    case 2:
    case 4:
    case 8:
        // Keep every divider-th byte, compact in-place
        output_count = buf_size / divider
        src = buf + divider  // start from byte[divider]
        for i in 1..output_count:
            buf[i] = src[0]
            src += divider
    default:
        ASSERT("_LowSpeedDivider is not a valid size: " + str(divider))
```

The algorithm is a simple stride-N copy. The first byte (buf[0]) is
already in place. For divider=2, every other byte is kept. For divider=8,
every 8th byte is kept.

The loop in the decompiled code uses 4x unrolling for the main loop body,
with a scalar cleanup loop for the remainder. This is a standard compiler
optimization pattern.

**Confidence: Confirmed** -- Direct decompilation, matches existing docs.


## OnWrite / OnDataBuffer Path

The `ContractReadData_caller` at Ghidra VA 0x1d3bca0 (ELF VA 0x1c3bca0)
is actually `LogicDevice::OnWrite(U8 *, U32, const LogicError &)`. Its
flow:

1. If no error (`*param_4 == 0`):
   a. Assert `mOnWriteData` callback exists
   b. Compute contracted size: `local_34 = param_3 / this->0x19c`
   c. Call `mOnWriteData` callback with original buffer + contracted size
   d. If divider > 1: call ExpandWriteData to reverse the contraction
      (restores the buffer for potential reuse by the USB transfer queue)
2. If error: call error handler

The ExpandWriteData step after the callback is significant -- it means
the buffer is contracted, passed to the disaggregator, then re-expanded
so the USB transfer queue can reuse the DMA buffer.

**Confidence: Confirmed** -- Source file: LogicDevice.cpp, assertion at
line 0x11f.


## Interaction with Disaggregator

The data flow for EP2 capture:

```
EP2 IN bulk transfer
  -> OnDataBuffer (size = raw_usb_bytes)
    -> ContractReadData (in-place, divider from LowSpeedDivider)
    -> OnWrite callback (size = raw_usb_bytes / divider)
      -> DisaggregatorNode (operates on contracted data)
    -> ExpandWriteData (restore buffer)
```

The disaggregator sees already-contracted data. Its cycle size computation
uses the contracted byte count. The GPIF width affects the disaggregator's
digital_packet_size (2 or 4 bytes per word) and analog packing parameters.


## Addresses

| Function | ELF VA | Ghidra VA | Size | Source |
|----------|--------|-----------|------|--------|
| ContractReadData | 0x1c3bad0 | 0x1d3bad0 | 383 | LogicDevice.cpp:0x16e |
| OnWrite (ContractReadData caller) | 0x1c3bca0 | 0x1d3bca0 | 565 | LogicDevice.cpp:0x11f |
| ExpandWriteData | 0x1c3bef0 | 0x1d3bef0 | 338 | LogicDevice.cpp:0x194 |
| StartSampling (sets LowSpeedDivider) | 0x1c3ad40 | 0x1d3ad40 | 1252 | LogicDevice.cpp |
| Constructor (sets 0x19c=1) | 0x1c399f1 | 0x1d399f1 | 689 | LogicDevice.cpp |
| Reset (sets 0x19c=1) | 0x1c39ed0 | 0x1d39ed0 | 101 | LogicDevice.cpp |
| ContractReadData thin wrapper | 0x1c3b4c0 | 0x1d3b4c0 | 81 | LogicDevice.cpp |
| DeviceSettings caller of MixRate | 0x1c4a6f9 | 0x1d4a6f9 | 195 | DeviceSettings.cpp |
| DeviceCalculateMixRate | 0x1c19fa0 | 0x1d19fa0 | 317 | DeviceHelpers.cpp |


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| GPIF width: Pro=32, non-Pro=16 | **Confirmed** | Decompilation of DeviceSettings caller: `(type_id & ~1) == 6` |
| ADC bits always 12 in DeviceSettings | **Confirmed** | Hardcoded `param_6 = 0xc` at line 2207 |
| LowSpeedDivider: 25k->8, 50k->4, 100k->2 | **Confirmed** | Decompilation of StartSampling switch |
| LowSpeedDivider: >=200k -> 1 | **Confirmed** | Decompilation of StartSampling else branch |
| 48 MHz base clock for non-Pro digital | **Confirmed** | `48000000 % sample_rate == 0` assertion |
| OnWrite does contract-callback-expand | **Confirmed** | Decompilation of FUN_01d3bca0 |
| Divider 0xef for low-speed modes | **Confirmed** | Decompilation, but 0xef meaning unclear |
| Low-speed rates 25k/50k/100k are the only valid sub-200k rates | **Confirmed** | All other values hit assert |
