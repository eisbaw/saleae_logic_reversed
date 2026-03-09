# JESD204B Link Management on Lattice ECP5 Devices

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0041
Method: String extraction from binary, cross-referenced with JESD204B
specification, ADS528x datasheet, and existing RE notes


## Summary

Lattice ECP5 hardware revisions of Saleae Logic devices use ADS528x
(ADS5282/ADS5287) octal 12-bit ADCs with a JESD204B serial interface to
the FPGA. This is a fundamentally different data path from the parallel
LVDS interface used by HMCAD1100 and AD9637 on Xilinx Spartan-6 revisions.

The JESD204B link initialization is managed by the host software through
FPGA register 0x0008 (STATUS_DDR_JESD_CONTROL). The process involves
PLL lock verification, SERDES transceiver reset, JESD lane synchronization,
and bitslip detection/correction. A built-in BER (Bit Error Rate) test
facility exists for link quality validation.


## JESD204B Background

JESD204B is a high-speed serial interface standard for ADC/DAC to FPGA
communication. Key concepts relevant to this analysis:

- **Lanes**: Physical serial links between ADC and FPGA. The ADS528x
  has 8 output lanes (one per ADC channel).
- **SYNC~**: Assertion signal from FPGA to ADC requesting link re-init.
- **CGS (Code Group Synchronization)**: First phase of link init where
  the receiver locks to the K28.5 comma character.
- **ILAS (Initial Lane Alignment Sequence)**: Second phase where multi-
  frame alignment is established.
- **Bitslip**: Bit-level alignment correction within the SERDES deserializer.
  Required when the serial-to-parallel conversion boundary does not align
  with the ADC's transmitted word boundary.
- **PLL lock**: Both the ADC (transmitter) and FPGA (receiver) must have
  stable clock recovery PLLs before data transfer.


## Hardware Configuration

### ADC: ADS528x (Texas Instruments)

- Part: ADS5282 or ADS5287
- Resolution: 12 bits
- Channels: 8 per chip
- Interface: JESD204B serial output
- No SPI register access from host -- JESD link management is FPGA-side

### FPGA: Lattice ECP5 variants

All Lattice bitstreams contain "ADS528x" in their design name, confirming
the tight coupling between FPGA design and ADC interface:

| Design Name | Device |
|-------------|--------|
| Logic8_Top_ADS528x_impl1.ncd | Logic 8 |
| Logic_Pro_Top_ADS528x_lse_impl1.ncd | Logic Pro 8 |
| Logic_Pro16_Top_ADS528x_lse_impl1.ncd | Logic Pro 16 |

ECP5 parts used (all CSFBGA285 package):

| Part | Architecture | LUTs |
|------|-------------|------|
| LFE5UM-25F | ECP5UM | 24K |
| LFE5UM5G-45F | ECP5UM5G (w/ SERDES) | 44K |
| LFE5U-45F | ECP5U | 44K |
| LFE5U-12F | ECP5U | 12K |

The ECP5UM5G variant has integrated multi-gigabit SERDES transceivers
natively. The ECP5U/ECP5UM variants may use soft SERDES or the PCS
(Physical Coding Sublayer) blocks available in those parts.


## JESD204B Initialization Sequence

The JESD init is part of `CommonDeviceInit`, which runs after FPGA
bitstream programming. The sequence for Lattice ECP5 devices (from string
analysis and cross-references):

```
Post-FPGA-programming init:
1. Write reg 0x01C0 (COMMIT_BANK_CONTROL)
2. Write reg 0x0040 (CAPTURE_CONTROL)
3. Write reg 0x00E0 (INIT_CONFIG)
4. Write reg 0x0024 (GPIF_FLUSH_TIMEOUT)
5. Write reg 0x0020 (GPIF_ENABLE)
6. [Lattice only] ADC power-on:
   a. SetAds528xDrive       -- configure physical layer drive strength
   b. SetAds528xEnabled     -- power on the ADS528x ADC
7. [Lattice only] JESD link init:
   a. ResetJesdPhyTransceivers  -- reset FPGA SERDES transceivers
   b. Poll JESD status until lock or timeout
   c. Bitslip detection and correction
8. [Lattice only] DDR init:
   a. WaitForDdrReady       -- poll reg 0x0008 for DDR locked+calibrated
```


## ResetJesdPhyTransceivers

Function signature (from binary):
```cpp
void Saleae::MsoFpga::ResetJesdPhyTransceivers(int, std::function<void ()>)
```

Source file: `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/component/mso_fpga.cpp`

The function takes an integer parameter (likely the retry count or timeout)
and a callback (likely a progress/status notification function).

### Sequence

1. Log: `"Resetting JESD Transceivers"`
2. Log: `"JESD reset started, waiting for lock..."`
3. Write to FPGA register 0x0008 (multiple times in a loop -- the exact
   values written control JESD PHY reset and release)
4. Poll JESD status from register 0x0008
5. Log extended status:
   ```
   FPGA JESD lock=0x{:x} synced=0x{:x} error=0x{:x}
     power_good={} init_done={} sync_err={}
     lane_0-3_pll={} lane_4-7_pll={} smpl_pll={}
   ```
6. On success: `"Locked JESD after {} ms, {} retries"`
   On failure: `"Failed to reset JESD transceivers, JESD status: synced={:x}, locked={:x}, error={:x}"`
   Also (typo variant): `"Failed to reset JESD trasceivers"`
   Fatal: `"JESD lock failed. Exiting."`

### PLL Lock Order

The extended status log reveals the PLL lock verification order:

1. **power_good**: FPGA power supply rails stable
2. **init_done**: FPGA initialization complete (post-bitstream)
3. **lane_0-3_pll**: Channel PLL lock for JESD lanes 0-3 (ADC channels 0-3)
4. **lane_4-7_pll**: Channel PLL lock for JESD lanes 4-7 (ADC channels 4-7)
5. **smpl_pll**: Sample clock PLL lock
6. **sync_err**: JESD SYNC~ error flag (should be clear)

The system PLL and PCLK PLL are checked separately:
```
PLL(s) failed to lock after PLL reset! system PLL: {}, PCLK PLL: {}.
  status register: {:#x}
```

After all PLLs lock:
```
Lattice PLL and SERDES reset complete.
```


## JesdStatus Struct

The JESD link status is represented by a `JesdStatus` struct with the
following fields (from format string in binary):

```
JesdStatus {
    link_up: bool,       // Overall link status
    sync_status: u32,    // Per-lane sync status (bitmask)
    realigned: bool,     // Link has been realigned
    aligned: bool,       // All lanes are aligned
    spll_locked: bool,   // System PLL locked
    cpll_locked: bool,   // Channel PLL(s) locked
}
```

Format string:
```
JesdStatus(link_up={}, sync_status={}, realigned={}, aligned={},
           spll_locked={}, cpll_locked={})
```


## FPGA Register 0x0008 (STATUS_DDR_JESD_CONTROL)

This is a multi-purpose register that serves DDR, JESD, and BER test
functions. Known bit assignments:

```
Bit 4: DDR locked status
Bit 3: DDR calibrated status

WaitForDdrReady check: (value & 0x18) == 0x18  (bits 3 and 4 both set)

During JESD init, the register is read, OR'd with 0x10 (bit 4), and
written back. Bit 4 may serve dual purpose as a DDR status readback
and a JESD control write.
```

The exact bit layout for JESD status fields (synced, locked, error,
power_good, init_done, sync_err, lane PLLs, smpl_pll) is not fully
decoded from string analysis alone. The status is likely read from
register 0x0008 and possibly adjacent registers, then unpacked by
the host code into the JesdStatus struct and the extended log fields.

### JESD Control Signals

Two string constants found that may map to register bit fields or
FPGA signal names:

- `JESD_ASSERT` -- likely the SYNC~ assertion control (FPGA asserts
  SYNC~ to ADC, requesting link re-initialization)
- `JESD_ASSH` -- possibly "JESD Assert/De-assert SYNC~ High" or a
  similar control for the SYNC~ handshake mechanism


## Bitslip Detection and Correction

Bitslip errors occur when the SERDES deserializer's word boundary does
not align with the ADC's transmitted word boundary. The Saleae software
handles this with a retry loop.

### Detection

After JESD lock is achieved, the software checks for bitslip on each
ADC (or ADC bank):

```
bitslip detected after enabling Ads528x. ADC1: {} ADC2: {}. Attempt {} of {}
bitslip error detected after SERDES reset. attempt {}. ADC0: {}, ADC1: {}
ADC 1 bitslip detected
ADC 2 bitslip detected
```

Note the inconsistency: some messages refer to ADC0/ADC1, others to
ADC1/ADC2. This may reflect different code paths or refactoring over
time. The two ADCs likely correspond to two physical ADC chips or two
banks within a single ADS528x (channels 0-3 and channels 4-7).

### Correction Algorithm (Hypothesis)

Based on string evidence, the bitslip correction follows this pattern:

```
1. Enable ADS528x ADC
2. Check bitslip status for ADC1 and ADC2
3. If bitslip detected:
   a. Reset SERDES ("bitslip error detected after SERDES reset")
   b. Retry (attempt N of M)
4. If max retries exceeded:
   - "Ads528x initialization - bitslip would not lock after multiple attempts."
   - Throw BitslipError exception
5. On success: proceed to normal operation
```

### Bitslip During Shutdown

Bitslip state is also managed during device shutdown:

```
resetting bitslip after slip, during shutdown.
resetting bitslip after slip, during shutdown, after bank shutoff
```

This suggests that clean shutdown requires resetting the SERDES state
to avoid stale bitslip alignment on the next power-on.

### BitslipError Exception

The binary defines a dedicated C++ exception type for bitslip failures:
```
N6Saleae12BitslipErrorE   (demangled: Saleae::BitslipError)
```

This exception is likely caught in `CommonDeviceInit` to trigger retries
or report initialization failure.


## JESD BER (Bit Error Rate) Test

A diagnostic BER test facility exists for validating JESD link quality.

### Functions

```
StartJesdBerTest     -- "Starting JESD BER test"
GetJesdBerTestResults -- "Getting JESD BER test results"
StopJesdBerTest      -- "Stopping JESD BER test"
```

### Registers

- Register 0x0008 (STATUS_DDR_JESD_CONTROL): Used for JESD status during BER test
- Register 0x00FC (JESD_BER_TEST): Dedicated BER test control register

The BER test likely sends a known test pattern through the JESD link
and counts bit errors over a specified duration. This is a standard
JESD204B diagnostic feature that validates the physical layer integrity.


## DDR Ready Check

After JESD link initialization, the DDR subsystem must also be verified:

```
WaitForDdrReady:
  1. Log: "Waiting for DDR locked and calibrated"
  2. Read register 0x0008 in polling loop
  3. Check: (value & 0x18) == 0x18  (bits 3 and 4)
  4. Sleep 1ms between polls
  5. Success: "Locked and calibrated DDR after {} ms"
```

The DDR check follows the JESD init because the DDR controller in the
ECP5 likely uses the JESD-recovered clock (or a PLL derived from it)
as its reference, so it cannot calibrate until the JESD link is stable.


## CommonDeviceInit Lambda

The CommonDeviceInit function contains a lambda callback:

```
Mangled: ZN16Logic2FpgaDevice16CommonDeviceInitEvE3$_1
Callback signature: void(bool, double)  -- (success flag, elapsed time or progress)
```

This lambda is passed to `ResetJesdPhyTransceivers` (which accepts
`std::function<void()>`) or used as the ADC calibration wait callback.
The `bool` parameter likely indicates success/failure, and the `double`
may be the elapsed time for progress reporting.


## Lattice vs Xilinx: ADC Interface Comparison

| Aspect | Xilinx (Spartan-6/7-Series) | Lattice (ECP5) |
|--------|---------------------------|----------------|
| ADC | HMCAD1100 or AD9637 | ADS528x |
| Interface | Parallel LVDS | JESD204B serial |
| ADC config | SPI via FPGA reg 0x0004 | SetAds528xEnabled/Drive |
| Link init | Not needed (parallel) | ResetJesdPhyTransceivers |
| PLL requirements | FPGA fabric PLL only | System PLL + PCLK PLL + lane PLLs + sample PLL |
| Bitslip | Not applicable | Required, with retry logic |
| DDR | Not mentioned | Explicit WaitForDdrReady |
| BER test | Not applicable | JESD BER test facility |
| Exception type | None specific | Saleae::BitslipError |


## String Evidence Table

All JESD/bitslip/PLL-related strings found in the binary:

| Category | String |
|----------|--------|
| JESD Reset | `Resetting JESD Transceivers` |
| JESD Reset | `JESD reset started, waiting for lock...` |
| JESD Reset | `ResetJesdPhyTransceivers` |
| JESD Reset | `void Saleae::MsoFpga::ResetJesdPhyTransceivers(int, std::function<void ()>)` |
| JESD Status | `FPGA JESD lock=0x{:x} synced=0x{:x} error=0x{:x}` |
| JESD Status | `FPGA JESD lock=0x{:x} synced=0x{:x} error=0x{:x} power_good={} init_done={} sync_err={}, lane_0-3_pll={}, lane_4-7_pll={}, smpl_pll={}` |
| JESD Status | `JesdStatus(link_up={}, sync_status={}, realigned={}, aligned={}, spll_locked={}, cpll_locked={})` |
| JESD Success | `Locked JESD after {} ms, {} retries` |
| JESD Failure | `Failed to reset JESD transceivers, JESD status: synced={:x}, locked={:x}, error={:x}` |
| JESD Failure | `Failed to reset JESD trasceivers` (sic) |
| JESD Failure | `JESD lock failed. Exiting.` |
| JESD Control | `JESD_ASSERT` |
| JESD Control | `JESD_ASSH` |
| JESD BER | `StartJesdBerTest` |
| JESD BER | `Starting JESD BER test` |
| JESD BER | `GetJesdBerTestResults` |
| JESD BER | `Getting JESD BER test results` |
| JESD BER | `StopJesdBerTest` |
| JESD BER | `Stopping JESD BER test` |
| PLL | `PLL(s) failed to lock after PLL reset! system PLL: {}, PCLK PLL: {}. status register: {:#x}` |
| PLL | `Lattice PLL and SERDES reset complete.` |
| Bitslip | `ADC 1 bitslip detected` |
| Bitslip | `ADC 2 bitslip detected` |
| Bitslip | `bitslip detected after enabling Ads528x. ADC1: {} ADC2: {}. Attempt {} of {}` |
| Bitslip | `bitslip error detected after SERDES reset. attempt {}. ADC0: {}, ADC1: {}` |
| Bitslip | `Ads528x initialization - bitslip would not lock after multiple attempts.` |
| Bitslip | `Bitslip lock failed.` |
| Bitslip | `bitslip 1 error` |
| Bitslip | `bitslip 2 error` |
| Bitslip | `resetting bitslip after slip, during shutdown.` |
| Bitslip | `resetting bitslip after slip, during shutdown, after bank shutoff` |
| Bitslip | `N6Saleae12BitslipErrorE` (exception type) |
| DDR | `Waiting for DDR locked and calibrated` |
| DDR | `Locked and calibrated DDR after {} ms` |
| DDR | `WaitForDdrReady` |
| ADC | `SetAds528xEnabled` |
| ADC | `SetAds528xDrive` |
| ADC | `ADC Locked` |
| ADC | `Failed to lock ADC` |
| ADC | `Configuring ADC` |
| ADC | `Force ADC 1V mode enabled` |
| Init | `CommonDeviceInit` |
| Init | `Lattice Semiconductor Corporation Bitstream` |
| Init | `Enabling GPIF, Flush timeout set to {}ns (every {} ticks @ 100Mhz)` |


## Open Questions

1. **Register 0x0008 bit field layout for JESD**: The exact bit positions
   for synced, locked, error, power_good, init_done, sync_err, lane PLLs,
   and smpl_pll within register 0x0008 are unknown. Needs Ghidra
   decompilation of ResetJesdPhyTransceivers to extract the bitmask
   constants.

2. **JESD_ASSERT and JESD_ASSH meaning**: These strings appear to be
   FPGA signal names or register field identifiers. They may control the
   SYNC~ assertion mechanism (standard JESD204B link init handshake), but
   the exact mapping is unclear without decompilation.

3. **Number of ADC chips**: The strings reference both "ADC0/ADC1" and
   "ADC1/ADC2" numbering. Are there one or two physical ADS528x chips per
   device? Logic Pro 16 has 16 analog channels -- with 8 channels per
   ADS528x, it would need two chips. Logic 8 / Logic Pro 8 may use one.

4. **Bitslip retry count**: The format string shows "Attempt {} of {}"
   but the actual maximum retry count is not extractable from strings
   alone. Needs decompilation.

5. **JESD lane assignment**: How are the 8 JESD lanes mapped to analog
   channels? Is lane N always channel N, or is there a configurable
   mapping?

6. **ADC register configuration table**: The function
   `GetAdcRegisterConfiguration(MsoDeviceConstants)` returns a static
   table of ADC register values. For ADS528x on Lattice, does this
   configure the ADC via a different path than SPI (since ADS528x uses
   JESD, not the SPI bridge at register 0x0004)?

7. **DDR purpose on ECP5**: What is the DDR controller used for?
   Possibilities include: sample buffering before USB transfer, JESD
   elastic buffer, or FPGA block RAM alternative for deep capture buffers.
   The ECP5 family supports DDR3/DDR3L memory interfaces.

8. **Force ADC 1V mode**: The string "Force ADC 1V mode enabled" suggests
   a debug/override mode for the ADC input voltage range. When and why
   is this used?


## Cross-References

- `re/notes/adc-initialization.md` -- Full ADC init for all three ADC types
- `re/notes/fpga-bitstream-protocol.md` -- Lattice ECP5 bitstream details
- `re/notes/fpga-register-map.md` -- Register 0x0008 and 0x00FC analysis
- `doc/protocol/fpga-registers.md` -- Verified register interface protocol
- `re/notes/fpga-features-vtable.md` -- FpgaDeviceFeatures methods
- `re/notes/device-class-hierarchy.md` -- CommonDeviceInit ownership


## Ghidra Analysis Next Steps

1. Decompile `ResetJesdPhyTransceivers` to extract:
   - Register 0x0008 bit field layout for JESD status/control
   - Retry count and timeout values
   - JESD_ASSERT / JESD_ASSH register field positions
2. Decompile the bitslip detection code to understand:
   - How bitslip status is read from the FPGA
   - The exact retry/reset algorithm
3. Decompile `WaitForDdrReady` to confirm the DDR bit field assignments
4. Examine `CommonDeviceInit` branch for Lattice vs Xilinx to understand
   the conditional logic that selects the JESD path
5. Trace `GetAdcRegisterConfiguration` for ADS528x-specific register table
