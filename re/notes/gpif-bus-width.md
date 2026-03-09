# GPIF Bus Width: 16-bit vs 32-bit Selection Logic

Investigation date: 2026-03-09
Binary: libgraph_server_shared.so v2.4.40
Firmware: fw-fx3-pro8.img, fw-fx3-pro16.img
Task: TASK-0053


## Summary

The "GPIF width" referenced in the host-side code is a **logical data
width** that describes how many useful bytes are packed into each
physical GPIF bus transfer. It is NOT a runtime reconfiguration of the
FX3 GPIF II hardware bus width. The FX3 physical bus is always 32-bit.

The FPGA outputs data in either 16-bit or 32-bit logical mode depending
on the number of enabled analog channels. When the FPGA uses 16-bit
mode, only 2 bytes of each 4-byte bus word contain valid data. The host
uses `ContractReadData` to strip the 2 padding bytes per word.


## Physical Bus Width (FX3 GPIF II)

The FX3 firmware initializes GPIF_BUS_CONFIG (0xE0014004) to 0x6C,
which decodes as:

| Field      | Value    | Meaning                      |
|------------|----------|------------------------------|
| PIN_COUNT  | 0 (0x00) | 47-pin (full GPIF interface) |
| BUS_WIDTH  | 3 (0x0C) | 32-bit (DQ[31:0])           |
| MUX_MODE   | 0        | Disabled                     |
| ADR_CTRL   | 3 (0x60) | 3 CTRL pins as address       |

The FX3 firmware contains exactly 2 references to GPIF_BUS_CONFIG, both
in initialization code (CyU3PGpifLoad and its helper). There is no
runtime reconfiguration of the physical bus width in the FX3 firmware.

This applies identically to Logic Pro 8 (fw-fx3-pro8.img) and Logic
Pro 16 (fw-fx3-pro16.img) -- zero differences in GPIF register config.


## Logical Data Width (FPGA Output Mode)

The FPGA can output data in two modes:

- **32-bit logical mode**: All 4 bytes of each bus word contain valid
  sample data. Used when multiple analog channels are active.
- **16-bit logical mode**: Only 2 bytes per bus word are valid; the
  other 2 bytes are padding. Used when fewer channels are active.

The selection is based on the number of enabled analog channels. The
host-side `LogicStudentDevice::StartSampling()` contains the error
string `"No valid analog and gpif width combo given"` which fires when
the channel configuration does not map to either valid logical width.


## Error String Context

The string `"No valid analog and gpif width combo given"` appears
**only once** in the binary, at file offset 0x34ed19, in the context
of `LogicStudentDevice.cpp` (the Logic 4 device class).

Nearby strings at the same file offset range:

| Offset   | String                               |
|----------|--------------------------------------|
| 0x34ecff | USB Device Init Error: {}            |
| 0x34ed19 | No valid analog and gpif width combo given |
| 0x34ed44 | Digital Fifo overflow                |
| 0x34ed76 | SetBankPower                         |
| 0x34ed9c | Failed to set capture settings       |
| 0x34eebb | LogicStudentDevice.cpp               |

This means the GPIF width selection logic is most constrained (and
explicitly validated) on the Logic 4. Other device classes likely have
wider valid combinations and may not hit this error.

Note: Logic 4 is FX2LP-based (not FX3), so its GPIF is the original
8/16-bit GPIF, not the 32-bit GPIF II. For Logic 4, the "GPIF width"
directly refers to the physical bus width (8 or 16 bits). The same
host-side concept ("GPIF width") applies to all device classes but
maps to different physical realities.


## Device-Specific GPIF Width

### Logic 4 (FX2LP, LogicStudentDevice)

- FX2LP GPIF supports 8-bit and 16-bit physical bus widths
- The "No valid analog and gpif width combo" error applies here
- HMCAD1100 ADC (8-bit) or ADS528x (12-bit, newer revisions)
- 4 channels total; limited analog+digital combinations
- Physical bus width likely changes between 8 and 16 bits

### Logic 8 (FX3, LogicGraduateDevice)

- FX3 GPIF II: 32-bit physical bus always
- Logical width is 16-bit or 32-bit depending on channels
- `ContractReadData` / `ExpandWriteData` in base LogicDevice class
- HMCAD1100 (old rev) or ADS528x (new rev) ADC
- 8 channels

### Logic Pro 8/16 (FX3, LogicProfessionalDevice)

- FX3 GPIF II: 32-bit physical bus always
- Logical width is 16-bit or 32-bit depending on channels
- AD9637 (old rev) or ADS528x (new rev) ADC, 12-bit
- 8 or 16 channels respectively
- Identical GPIF firmware configuration


## Width Selection Logic (Hypothesis)

Based on the data-interleaving analysis and the GPIF firmware:

**32-bit logical mode** is selected when the per-sample row width
(sum of all channel byte widths) exceeds 2 bytes:
- Multiple analog channels enabled (each analog = 2 bytes)
- Many digital channels enabled (4 digital channels = 1 byte, packed)
- Total row width > 2 bytes -> 32-bit mode

**16-bit logical mode** is selected when the per-sample row width
fits in 2 bytes:
- 1 analog channel (2 bytes), no or few digital channels
- Digital-only with few channels
- Total row width <= 2 bytes -> 16-bit mode

The `ContractReadData` function on the host removes padding when the
logical width is less than the physical bus width (i.e., 16-bit logical
mode on the 32-bit FX3 bus results in 2 padding bytes per word that
get stripped).

**Confidence: Hypothesis** -- The exact selection thresholds need
Ghidra decompilation of the width selection code or USB capture
comparison with different channel configurations.


## Relationship to USB 2.0 vs 3.0

The GPIF bus width is NOT directly related to USB speed. The FX3 GPIF
is the FPGA-to-FX3 internal bus, independent of USB negotiation.
However:

- USB 2.0 limits max throughput to ~40 MB/s (480 Mbps overhead)
- USB 3.0 allows ~350 MB/s (5 Gbps with overhead)
- At USB 2.0 speeds, fewer channels can sustain full sample rate
- This indirectly affects which channel configurations are valid
- The WriteConfigurationData command uses 62-byte chunks (USB 2.0)
  vs 509-byte chunks (USB 3.0), but this is the command channel size
  limit, not the GPIF data width

For Logic 4 (FX2LP), USB speed more directly affects GPIF:
- FX2LP is USB 2.0 only (480 Mbps max)
- FX2LP GPIF physical bus width may be 8 or 16 bits
- Lower bus width = lower throughput = fewer simultaneous channels


## ContractReadData / ExpandWriteData

These base-class methods on `LogicDevice` handle the mismatch between
physical bus width and logical data width:

- `ContractReadData(U8 *)`: Removes padding bytes from USB bulk data
  received on EP2 IN. When the FPGA outputs 16-bit logical data on a
  32-bit physical bus, each 4-byte word has 2 bytes of padding that
  must be stripped before the disaggregator processes the data.

- `ExpandWriteData(U8 *)`: Adds padding bytes when writing
  configuration data back to the FPGA. The FPGA expects data aligned
  to its bus width.

These run in the `OnDataBuffer` path, before the `DisaggregatorNode`
splits the interleaved stream into per-channel data.


## FPGA Register Interface

The host configures the FPGA's data output mode through register
writes (FPGA registers 0x0020 GPIF_ENABLE and 0x0024 GPIF_FLUSH_TIMEOUT)
and the broader capture configuration blob sent via
WriteConfigurationData (opcode 0x7F). The FPGA's output mode
(16-bit vs 32-bit logical width) is set as part of the capture
configuration, not as a separate GPIF width command.


## Open Questions

1. **Exact selection thresholds**: At what analog channel count does
   the mode switch from 16-bit to 32-bit? Is it 1 analog channel =
   16-bit, 2+ = 32-bit? Or is it more nuanced?

2. **Logic 4 valid combinations**: What specific (analog_count,
   gpif_width) pairs are valid for LogicStudentDevice? The error
   string implies a validation table exists.

3. **ContractReadData algorithm**: Does it simply strip every other
   pair of bytes, or is the contraction pattern more complex?

4. **FPGA register controlling output mode**: Which FPGA register
   bit(s) select the 16-bit vs 32-bit output mode? Is it in
   CAPTURE_CONTROL (0x0040) or a separate register?

5. **Configuration data blob format**: The capture parameters sent
   via WriteConfigurationData presumably include the logical width
   setting. The exact field offset is unknown.


## References

- re/notes/gpif-state-machine.md -- FX3 GPIF II state machine analysis
- re/notes/data-interleaving.md -- interleaving architecture
- re/notes/ep1-command-protocol.md -- WriteConfigurationData (0x7F)
- re/notes/capture-start-stop.md -- capture sequence
- re/notes/logic4-8-firmware.md -- Logic 4 is FX2LP, not FX3
- re/notes/device-settings.md -- device constructor constants
- doc/protocol/fpga-registers.md -- confirmed FPGA register map
- re/scripts/parse-gpif.py -- GPIF descriptor parser
