# Capture Configuration Constants

Status: Confirmed (extracted from libgraph_server_shared.so v2.4.40)
Corroboration: CMP instruction immediates in binary, assertion string
cross-reference, JS bundle analysis
Source: TASK-0023


## FpgaConstants (Compile-Time)

These constants define the valid range for `mSamplesPerChunk` (the number
of samples in each USB data packet). The correct set of limits is selected
based on the FPGA type in the device.

| Constant | Value | Hex | Notes |
|----------|-------|-----|-------|
| MinSamplesPerPacket_Xilinx | 256 | 0x100 | Xilinx Spartan-6 FPGAs |
| MaxSamplesPerPacket_Xilinx | 7680 | 0x1E00 | 30 x 256 |
| MinSamplesPerPacket_Altera | 64 | 0x40 | Altera FPGAs |
| MaxSamplesPerPacket_Altera | 6016 | 0x1780 | Not a multiple of 256 |
| SamplesPerPacketMultiple_WithStats | 256 | 0x100 | Alignment when stats enabled |

Extraction method: CMP instruction immediates in the `ValidateConfig`
function at VA 0x02d64640 in `mso_fpga.cpp`.

### Validation Rules

```
mSamplesPerChunk >= MinSamplesPerPacket_{Altera,Xilinx}
mSamplesPerChunk <= MaxSamplesPerPacket_{Altera,Xilinx}

// Only when stats collection is enabled:
mSamplesPerChunk % SamplesPerPacketMultiple_WithStats == 0
(mPreTriggerLength + mPostTriggerLength) % SamplesPerPacketMultiple_WithStats == 0
```

When stats are enabled, `mSamplesPerChunk` must be a multiple of 256.
This means the effective maximum for Altera with stats is 5888 (23 x 256),
since 6016 is not a multiple of 256.

### FPGA Type Selection

The device type byte determines which limits apply:
- Device type >= 2: Xilinx limits (larger packets, higher min)
- Device type <= 1: Altera limits (smaller packets, lower min)

Evidence: `test $0xfe, %al; je` at VA 0x2d64750 selects the code path.


## MsoConstants (Compile-Time)

These constants are hardcoded in the binary and used in assertions
for trigger buffer and hardware configuration limits.

| Constant | Value | Hex | Notes |
|----------|-------|-----|-------|
| MinTriggerBufferSamples | 8 | 0x8 | Common to both FPGA types |
| MaxTriggerBufferSamples_Xilinx | 1,073,725,440 | 0x3FFFC000 | ~1 billion samples |
| MaxTriggerBufferSamples_Altera | 805,289,984 | 0x2FFFC000 | ~805 million samples |
| MaxScopeChannels | 4 | 0x4 | Oscilloscope analog channels |
| TotalAfeConfigurations | 66 | 0x42 | AFE config numbers 0-66 |

Extraction method: CMP instruction immediates in mso_fpga.cpp and
oscilloscope_simulator.cpp functions.

### Trigger Buffer Constraints

```
sample_length = mPreTriggerLength + mPostTriggerLength
sample_length >= MinTriggerBufferSamples                    (>= 8)
sample_length <= MaxTriggerBufferSamples_{Altera,Xilinx}
-mPreTriggerLength < mPostTriggerLength
```


## MsoDeviceConstants (Runtime, Per-Device)

These values are NOT compile-time constants. They are device-specific
configuration values, populated at runtime and serialized as JSON between
the native code and the Electron frontend.

The MsoDeviceConstants struct is approximately 0xA8 bytes. Fields
are deserialized from JSON keys in a serialization function at
VA 0x02d77b00.

### Struct Layout

| Offset | JSON Key | Description |
|--------|----------|-------------|
| 0x00 | /maxAdcCode | Maximum ADC code value |
| 0x08 | /analogChannelCount | Number of analog channels |
| 0x10 | /smartCablePortCount | Number of smart cable ports |
| 0x18 | /clockRateHz | FPGA clock rate in Hz |
| 0x20 | /analogSampleRate | Analog sample rate in Hz |
| 0x28 | /scopeDigitalSampleRate | Digital sample rate for scope mode |
| 0x30 | /minAnalogSamplesPerPacket | Min analog samples per packet |
| 0x38 | /maxAnalogSamplesPerPacket | Max analog samples per packet |
| 0x40 | /minDacVoltage | DAC minimum voltage |
| 0x48 | /maxDacVoltage | DAC maximum voltage |
| 0x50 | /dacBitDepth | DAC bit depth |
| 0x60 | /fpgaImageType | FPGA image type identifier |
| 0x68 | /analogLimit_MaxDownsampleRatio | Max downsample ratio |
| 0x70 | /analogLimit_MinTotalTriggerBufferLength | Min trigger buffer length |
| 0x78 | /analogLimit_MaxTotalTriggerBufferLength | Max trigger buffer length |
| 0x80 | /analogLimit_MinTriggerBufferIndex | Min trigger buffer index (pre-trigger) |
| 0x88 | /analogLimit_MaxTriggerBufferIndex | Max trigger buffer index (post-trigger) |
| 0x90 | /analogLimit_MaxHoldoffSamples | Max holdoff in samples |
| 0x98 | /analogLimit_MaxTimeoutSamples | Max timeout in samples |
| 0xA0 | /digitalLimit_MinGlitchFilterLength | Min glitch filter pulse width |
| 0xA4 | /digitalLimit_MaxGlitchFilterLength | Max glitch filter pulse width |

### Glitch Filter Constraint

The pulse width for the digital glitch filter must satisfy:
```
pulseWidth >= digitalLimit_MinGlitchFilterLength
pulseWidth <= digitalLimit_MaxGlitchFilterLength
pulseWidth == 5 || (pulseWidth % 10 == 0)
```

The valid values are: 5, 10, 20, 30, ... up to the device-specific maximum.
The special value 5 and the multiple-of-10 constraint are hardcoded in
`MsoFpga::InternalSetDigitalConfiguration`.


## Downsample Ratio Rules

From JS bundle analysis. The downsample ratio is quantized:

| Input Ratio | Quantized To | Notes |
|-------------|-------------|-------|
| 1 | 1 | No downsampling |
| 2 | 2 | |
| 3 | 4 | Bumped up |
| 4 | 4 | |
| 5 | 6 | Bumped up |
| 6 | 6 | |
| 7 | 8 | Bumped up |
| 8+ | nearest 8*N | N in [1, 162500] |

Maximum effective downsample ratio: 8 x 162500 = 1,300,000.

Minimum ratios per downsample mode:
- Normal: 1
- Average: 2
- PeakDetect (MinMax): 4

### Available Downsampled Rates

Fixed list for devices supporting downsampling (Logic 4/8/Pro 8/Pro 16):
```
31250, 15625, 6250, 1250, 250, 50 Hz
```


## Confidence Assessment

| Finding | Confidence | Method |
|---------|-----------|--------|
| FpgaConstants packet size limits | **High** | CMP immediates in binary (Ghidra only, not yet hardware-validated) |
| SamplesPerPacketMultiple_WithStats = 256 | **High** | test %cl,%cl pattern; consistent with Xilinx max=7680=30x256 |
| MsoConstants trigger buffer limits | **High** | CMP immediates in binary (Ghidra only) |
| MaxScopeChannels = 4 | **High** | CMP immediate in binary (Ghidra only) |
| TotalAfeConfigurations = 66 | **High** | CMP immediate in binary (Ghidra only) |
| Glitch filter: 5 or multiple of 10 | **High** | Assertion string in binary + fpga-registers.md documents same constraint |
| MsoDeviceConstants struct layout | **High** | Serialization function disassembly |
| Downsample ratio quantization | **High** | JS bundle source code (single source, not hardware-validated) |
| analogLimit_* are runtime values | **High** | JSON serialization + Ghidra (2 of 3 sources) |


## Open Questions

1. **Exact analogLimit values per device**: These are runtime values and
   their actual numeric values depend on the device model and firmware
   version. Need USB capture or runtime instrumentation to observe.

2. **MinTriggerBufferIndex values**: The Altera and Xilinx
   MinTriggerBufferIndex values were not extracted as clear immediates.
   The MaxTriggerBufferIndex is likely the same as MaxTriggerBufferSamples
   but needs verification.

3. **Altera vs Xilinx device type mapping**: Device type 0/1 = Altera,
   type >= 2 = Xilinx. The meaning of the specific type values (0, 1, 2,
   3, ...) and which hardware revisions they correspond to is unknown.
   See `doc/protocol/fpga-registers.md` for FPGA variant constraints on
   SamplesPerChunk and MaxTriggerBuffer (Xilinx: 0x100-0x1E00 / 0x3FFFC000,
   Altera: 0x40-0x1780 / 0x2FFFC000).

4. **SamplesPerPacketMultiple without stats**: When stats collection is
   disabled, what alignment (if any) is required? The code only checks
   the modulo when the withStats flag is set.


## Cross-References

- **FPGA register interface**: `doc/protocol/fpga-registers.md` -- documents
  the FpgaDeviceFeatures register map including capture config registers
  (0x06-0x14) and FPGA variant constraints that correspond to the constants here.
- **Capture sequence**: `doc/protocol/capture-sequence.md` -- uses these
  constants for validation during SetCaptureParameters.
