# GPIF II State Machine Analysis

Analysis date: 2026-03-09
Binary: fw-fx3-pro8.img (119,824 bytes, Cypress FX3 firmware)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0046
Tool: `re/scripts/parse-gpif.py`


## Summary

The FX3 firmware contains a 3-state GPIF II state machine that implements
a **synchronous 32-bit parallel slave FIFO interface** between the FPGA
and the FX3 microcontroller. The FX3 is the clock master and generates
the bus clock internally (50-100 MHz range). Data flows unidirectionally
from the FPGA to the FX3 during capture, pushed into DMA write queues
that feed USB EP2 IN bulk transfers.

The GPIF configuration is **identical** between the Logic Pro 8
(fw-fx3-pro8.img) and Logic Pro 16 (fw-fx3-pro16.img) firmware images.
Zero register differences exist in the GPIF descriptor data.

Key findings:

1. **32-bit data bus** (DQ[31:0]) configured at firmware load time
2. **Synchronous mode** with internal clock, 50-100 MHz speed setting
3. **Simple 2-phase handshake**: CTRL[0] from FPGA controls data flow
4. **4 DMA threads** active, each with 64-byte burst size
5. **THREAD_IN_STATE** mode: state machine beta outputs select the
   active DMA thread, allowing round-robin or priority-based multiplexing
6. The initial 32-bit bus width may be reconfigured at runtime by the
   host via the WriteConfigurationData command (opcode 0x7F), which can
   switch to 16-bit mode depending on the number of analog channels


## Method

The GPIF II descriptor was located by:

1. Parsing the FX3 firmware's B0 image sections to build a memory map
2. Disassembling the ARM926EJ-S code in Section 2 (SYSMEM) to find the
   `CyU3PGpifLoad()` call site in `LogicProApplicationInit`
3. Following the literal pool to find the `CyU3PGpifConfig_t` pointer
   at memory address 0x40008E58
4. Parsing the struct fields to extract pointers to waveform data,
   transition functions, and register init arrays in Section 4

All data was cross-referenced against the Cypress FX3 SDK source code
(`cyu3gpif.h`, `cyu3gpif.c`, `gpif_regs.h`) to verify bit field
interpretations.


## CyU3PGpifConfig_t Structure

Located at memory 0x40008E58 (firmware file offset 0x81EC):

| Field          | Value       | Description                           |
|----------------|-------------|---------------------------------------|
| stateCount     | 3           | Number of GPIF states                 |
| stateData      | 0x40030138  | Waveform descriptor array (Section 4) |
| statePosition  | 0x40030134  | State -> waveform entry mapping       |
| functionCount  | 3           | Number of transition functions        |
| functionData   | 0x40030168  | Transition function truth tables      |
| registerCount  | 76          | GPIF config register values           |
| registerData   | 0x40030004  | Register init array (Section 4)       |


## Transition Functions

Three 16-bit truth tables define the transition logic. Each function
takes 4 lambda inputs (Fa, Fb, Fc, Fd) and produces a 1-bit output.
The 16-bit value is the truth table indexed by the 4-bit input vector.

| Function | Value  | Meaning                    |
|----------|--------|----------------------------|
| 0        | 0x0000 | ALWAYS FALSE               |
| 1        | 0xAAAA | Output = Fa (pass-through) |
| 2        | 0x5555 | Output = NOT Fa            |


## State Machine

### State Position Mapping

| State | Waveform Entry | Description        |
|-------|----------------|--------------------|
| 0     | 0              | IDLE / initial     |
| 1     | 1              | PUSH to DMA        |
| 2     | 0              | SAMPLE (same as 0) |

States 0 and 2 share the same waveform descriptor (entry 0).

### Waveform Descriptors

Only LEFT descriptors are valid. RIGHT descriptors are all zero (invalid).
When the LEFT transition function evaluates FALSE and no RIGHT descriptor
exists, the state machine stays in the current state.

**Entry 0 (used by State 0 and State 2):**

| Field          | Value                              |
|----------------|------------------------------------|
| Next state     | 1                                  |
| Repeat         | 0 (= 1 clock cycle)               |
| Transition F0  | function[1] = Fa (CTRL[0])         |
| Transition F1  | function[0] = FALSE                |
| Alpha (left)   | 0x04 = SAMP_DIN (sample input data)|
| Alpha (right)  | 0x00 (none)                        |
| Beta           | 0x00000000 (none)                  |

**Entry 1 (used by State 1):**

| Field          | Value                                  |
|----------------|----------------------------------------|
| Next state     | 2                                      |
| Repeat         | 0 (= 1 clock cycle)                   |
| Transition F0  | function[2] = NOT Fa (NOT CTRL[0])     |
| Transition F1  | function[0] = FALSE                    |
| Alpha (left)   | 0x00 (none)                            |
| Alpha (right)  | 0x00 (none)                            |
| Beta           | 0x00000080 = PUSH_WQ (push to DMA WQ) |

### State Machine Operation

```
    CTRL[0]=1         CTRL[0]=0           CTRL[0]=1
  +----------+     +----------+       +----------+
  |          |     |          |       |          |
  |  State 0 |---->|  State 1 |------>|  State 2 |---+
  |  (IDLE)  |     |  (PUSH)  |       |  (SAMPLE)|   |
  |          |     |          |       |          |   |
  +----------+     +----------+       +----------+   |
       ^  |             ^  |                          |
       |  |             |  |                          |
       +--+             +--+                          |
    CTRL[0]=0        CTRL[0]=1           CTRL[0]=0    |
       wait            wait                           |
                                      +---------------+
                                      |  CTRL[0]=0
                                      |  (wait, go back to State 1
                                      |   when CTRL[0] asserts again)
                                      v
                                (loops State 2 <-> State 1)
```

**Two-phase handshake protocol:**

1. **Phase 1 (Sample)**: The FPGA places 32-bit data on DQ[31:0] and
   asserts CTRL[0] (active-low at the pin). The FX3 samples the data
   bus (`SAMP_DIN` alpha). Transition: State 0/2 -> State 1.

2. **Phase 2 (Push)**: The FX3 pushes the sampled data into the DMA
   write queue (`PUSH_WQ` beta). It waits for CTRL[0] to deassert.
   When CTRL[0] deasserts, transition: State 1 -> State 2.

3. **Repeat**: State 2 is functionally identical to State 0. The cycle
   continues: sample on CTRL[0] assert, push on CTRL[0] deassert.

The steady-state loop is State 1 <-> State 2, alternating between
PUSH and SAMPLE on each toggle of CTRL[0].


## CTRL Bus Configuration

| Pin     | Direction | Polarity | Function                               |
|---------|-----------|----------|----------------------------------------|
| CTRL[0] | Input     | Inverted | Data valid strobe from FPGA            |
| CTRL[1] | Input     | Inverted | FPGA status (purpose TBD)              |
| CTRL[2] | Output    | Inverted | Thread 0 partial flag (DMA watermark)  |
| CTRL[3] | Input     | Normal   | Unused or FPGA status                  |
| CTRL[4] | Output    | Normal   | Alpha 0 output (state-machine driven)  |

CTRL[0] is the primary handshake signal. The FPGA toggles CTRL[0] to
clock data across the 32-bit parallel bus. The active-low polarity
(inverted at the GPIF input) means the physical pin is asserted when
driven low by the FPGA.

CTRL[2] provides DMA backpressure to the FPGA: when the DMA buffer
is filling up (partial flag asserted), the FPGA should pause data
transmission to avoid overflow. This maps to the string
`"Analog Fifo overflow"` seen in the host-side code.


## Bus Configuration

### Data Bus

| Parameter        | Value                                     |
|------------------|-------------------------------------------|
| Bus width        | 32-bit (DQ[31:0])                         |
| Pin count        | 47-pin (full GPIF interface)              |
| Mode             | Synchronous                               |
| Clock source     | Internal (GCTL, FX3 generates clock)      |
| Clock output     | Enabled (FX3 is clock master)             |
| Speed setting    | 50-100 MHz range                          |
| Endianness       | Little-endian                             |
| DDR mode         | Disabled (SDR)                            |
| MUX mode         | Disabled (separate address/data lines)    |
| Data direction   | Bidirectional (alpha-controlled DQ_OEN)   |

The 32-bit bus width at firmware init matches the host-side code that
references `"No valid analog and gpif width combo given"` and the
ContractReadData/ExpandWriteData functions documented in
`data-interleaving.md`. The host may reconfigure the bus width to
16-bit at runtime via the WriteConfigurationData command (opcode 0x7F).

### Address Bus

| Parameter        | Value                                     |
|------------------|-------------------------------------------|
| ADR_CTRL         | 3 (3 CTRL pins repurposed as address)     |
| Address thread   | 1                                         |
| Data thread      | 0                                         |

Three CTRL pins (CTRL[13-15]) are repurposed as address lines. This
gives the FPGA a way to communicate address/command information to the
FX3 alongside data, potentially used for interleaving channel metadata.


## DMA Thread Configuration

All 4 threads are enabled with identical settings:

| Thread | Socket | Burst (log2) | Burst (bytes) | Watermark | Enabled |
|--------|--------|-------------|---------------|-----------|---------|
| 0      | 0      | 4           | 64            | 1 word    | Yes     |
| 1      | 1      | 4           | 64            | 1 word    | Yes     |
| 2      | 2      | 4           | 64            | 1 word    | Yes     |
| 3      | 3      | 4           | 64            | 1 word    | Yes     |

**THREAD_IN_STATE** is enabled in GPIF_CONFIG (bit 15). This means the
active DMA thread is selected by the beta output bits [4:5] in the state
machine, not by a fixed THREAD_CONFIG assignment. In the current state
machine, the beta outputs for thread selection are always 0, meaning
Thread 0 is used by default. The host may reconfigure this at runtime
to enable multi-thread operation for higher throughput.

The 64-byte burst size means the FX3 will transfer at least 64 bytes
(16 x 32-bit words) to/from the DMA socket before checking the partial
flag. This is important for USB 3.0 SuperSpeed operation where the
GPIF runs at 100 MHz with a 32-bit bus.


## Counter Configuration

| Counter | Enable | Reload | Direction | Increment | Limit  |
|---------|--------|--------|-----------|-----------|--------|
| Control | No     | Yes    | Up        | 0         | 65535  |
| Address | No     | Yes    | Down      | 33        | 65535  |
| Data    | No     | Yes    | Down      | 33        | 65535  |

All counters are disabled in the initial configuration. The address and
data counters have an increment of 33 and count downward, suggesting
they may be enabled at runtime to track transfer sizes. The increment
value of 33 is notable -- it may relate to the 32-bit bus width plus
one status byte, or to a specific packet size alignment.


## Cross-References

### Data Interleaving (re/notes/data-interleaving.md)

The 32-bit GPIF bus width matches the finding that the FPGA outputs data
in "16-bit or 32-bit mode" depending on the number of analog channels.
The GPIF firmware initializes with 32-bit mode. When fewer channels are
active, the host reconfigures to 16-bit mode to reduce bus bandwidth.

The `ContractReadData` function on the host side removes padding bytes
when the 32-bit bus carries less-than-32-bits of useful data per word.

### EP1 Command Protocol (re/notes/ep1-command-protocol.md)

The WriteConfigurationData command (opcode 0x7F) is used to write
FPGA configuration data and potentially reconfigure GPIF registers.
This is the mechanism by which the host switches between 16-bit and
32-bit GPIF modes at runtime.

### FX3 USB Events (re/notes/fx3-usb-events.md)

The initialization sequence shows CyU3PGpifLoad() and CyU3PGpifSMStart()
called during `LogicProApplicationInit`, before USB stack initialization.
The state machine starts running immediately and waits for CTRL[0] from
the FPGA. Since the FPGA is not yet programmed at this point, CTRL[0]
remains deasserted and the state machine idles in State 0.

### Capture Start/Stop (re/notes/capture-start-stop.md)

The COMMAND_START_READ (0x01) command likely triggers the FPGA to begin
toggling CTRL[0] and streaming data. The COMMAND_STOP_STREAM (0x02)
command tells the FPGA to stop. The DMA setup via CyFxBulkSrcSinkApplnStart
connects the P-port sockets (used by GPIF threads) to USB EP2 IN.


## Comparison with Standard Cypress Slave FIFO

This GPIF configuration closely resembles the standard Cypress
"Slave FIFO" GPIF II design from the FX3 SDK examples, but simplified:

| Feature             | Cypress Slave FIFO | Saleae Config    |
|---------------------|-------------------|------------------|
| Bus width           | 16 or 32-bit      | 32-bit           |
| Direction           | Bidirectional      | Bidirectional    |
| States              | 4+                 | 3                |
| Handshake           | FLAGB/FLAGC/SLWR  | CTRL[0] toggle   |
| DMA threads         | 2                  | 4                |
| Socket assignment   | Fixed              | State-machine    |

The Saleae design is simpler because it only needs one-way data transfer
during capture (FPGA -> FX3 -> USB). The bidirectional capability is
likely used only during FPGA programming (FX3 -> FPGA bitstream download).


## Open Questions

1. **16-bit mode reconfiguration**: How and when does the host switch
   the GPIF bus width from 32-bit to 16-bit? This must involve writing
   to GPIF_BUS_CONFIG via the WriteConfigurationData command. A USB
   capture during configuration would reveal the exact register writes.

2. **Thread selection at runtime**: The THREAD_IN_STATE mode allows the
   state machine to select different DMA threads. In the initial config,
   only Thread 0 is active (beta thread bits = 0). Does the host
   reconfigure beta outputs to use multiple threads for higher throughput?

3. **Counter usage**: The address and data counters have increment=33
   but are disabled. Are they enabled at runtime for packet-level
   tracking? The value 33 needs explanation.

4. **CTRL[1] purpose**: CTRL[1] is an input from the FPGA with inverted
   polarity. It is not used by the state machine transition functions.
   It may be a secondary FPGA status signal (e.g., FIFO empty/full).

5. **CTRL[4] Alpha 0 output**: CTRL[4] outputs the Alpha 0 signal
   which is DQ_OEN (data bus output enable). This suggests the FX3 can
   also drive data to the FPGA (for bitstream programming), controlled
   by the Alpha 0 output in the state machine.

6. **Clock frequency**: The GPIF runs at 50-100 MHz (SYNC_SPEED=1).
   The exact frequency depends on the GCTL clock configuration, which
   is not part of the GPIF descriptor. With 32-bit bus at 100 MHz, the
   theoretical throughput is 400 MB/s, well above USB 3.0's 5 Gbps
   (~400 MB/s effective), so the USB link is the bottleneck.


## References

- Cypress FX3 SDK: cyu3gpif.h -- CyU3PGpifConfig_t struct definition
- Cypress FX3 SDK: cyu3gpif.c -- CyU3PGpifLoad() implementation
- Cypress FX3 SDK: gpif_regs.h -- GPIF register bit field definitions
- Cypress EZ-USB FX3 Technical Reference Manual (001-76074)
- re/notes/data-interleaving.md -- GPIF width references, bus modes
- re/notes/fx3-usb-events.md -- GPIF initialization sequence
- re/notes/ep1-command-protocol.md -- WriteConfigurationData (0x7F)
- re/notes/capture-start-stop.md -- capture flow
- re/notes/fx3-firmware-protocol.md -- firmware section layout
