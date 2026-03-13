# EP2 config_id Assignment and Start Packet Emission Timing

Analysis date: 2026-03-13
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0202


## Summary

The `config_id` field (bits [31:16] of the EP2 chunk header) is a
**monotonically incrementing counter** assigned by the host software.
Each call to `SetConfiguration` increments the counter by 1. The value
is written to FPGA registers on MSO-class devices; the FPGA embeds it
in every chunk header. On Logic2FpgaDevice (Spartan-6), no config_id
registers exist in the 8-bit register interface.

Start packets (`is_start=1`) are emitted **once per enabled channel at
the beginning of each capture**. A second start packet for the same
channel resets the per-channel state.


## config_id: Monotonically Incrementing Counter

### Assignment (SetConfiguration)

**Function**: `MsoOscilloscopeInstrument::SetConfiguration`
**ELF VA**: 0x2d477d0 (Ghidra: 0x2e477d0)
**Source**: `oscilloscope_instrument.cpp`

```c
// Decompiled pseudocode (simplified)
int SetConfiguration(MsoOscilloscopeInstrument *this, OscilloscopeConfiguration config, uint64_t session_id) {
    // ... validate channel config against device constants ...
    FUN_02e47b00(&derived_config, this);  // derive internal config

    std::mutex::lock(&this->mutex_0x150);

    int new_config_id = *(int *)(this + 0x1f0) + 1;  // INCREMENT
    *(int *)(this + 0x1f0) = new_config_id;           // STORE
    *(uint64_t *)(this + 0x1f8) = session_id;         // store session
    memcpy(this + 0x200, &derived_config, 0x1b8);     // store full config
    memcpy(this + 0x3b8, &extra_data, 0x109);

    std::mutex::unlock(&this->mutex_0x150);
    // ... signal condition variable ...

    return new_config_id;  // return type is ConfigurationId
}
```

**Key observations**:
- Counter is at object offset **0x1f0**
- Increment is **+1 each SetConfiguration call** (not a hash, not random)
- Protected by mutex at offset 0x150 (thread-safe)
- Return type is `MsoOscilloscopeInstrument::ConfigurationId` (confirmed
  from assertion string)
- Session ID (uint64_t) is stored alongside at offset 0x1f8

**Confidence: High** -- direct decompilation of the function, mutex
pattern is clear, assertion string confirms return type.


### FPGA Register Write (MSO Path)

**Function**: `MsoFpga::SetConfigurationIds`
**ELF VA**: 0x2d64230 (Ghidra: 0x2e64230)
**Source**: `mso_fpga.cpp` line 0x240

Takes two packed uint64 parameters containing 4 config_id values
(one per channel). Writes to FPGA registers:

| Register | Address | Value |
|----------|---------|-------|
| SCOPE_CONFIG_ID_0 | 0x0044 | param_2 low 32 bits |
| SCOPE_CONFIG_ID_1 | 0x0048 | param_2 high 32 bits |
| SCOPE_CONFIG_ID_2 | 0x004C | param_3 low 32 bits |
| SCOPE_CONFIG_ID_3 | 0x0050 | param_3 high 32 bits |

Log: `"Setting scope configuration IDs: {} {} {} {}"`

After this write, the FPGA includes the config_id value in bits [31:16]
of every EP2 chunk header.

**Confidence: High** -- decompiled function, register addresses match
`fpga-register-map.md`.


### Logic2FpgaDevice Path (Spartan-6, Logic Pro 8)

`Logic2FpgaDevice::SetCaptureParameters` (ELF VA 0x1c24fb9) writes
these 8-bit FPGA registers:

| Register | Purpose |
|----------|---------|
| 0x06, 0x07 | Sample rate divider (low/high) |
| 0x08, 0x09 | Channel config (conditional on 16-bit mode) |
| 0x0A | Analog decimation lower |
| 0x0B, 0x0C | Trigger config |
| 0x0D, 0x0E | Mix ratio |
| 0x12 | Trigger channel select |
| 0x13 | Flags (performance, ignore errors) |
| 0x14 | Decimation factor |
| 0x1A | Stats/analog samples per chunk (conditional) |
| 0x1B | Analog decimation upper (revision-dependent) |

**No config_id registers** are written in this path. The 8-bit register
space for Spartan-6 FPGAs does not include SCOPE_CONFIG_ID equivalents.

**Implication**: On Logic Pro 8 hardware, the config_id in chunk headers
is expected to be 0 (default FPGA register value) or a fixed constant
from the FPGA bitstream. The host software's comparison logic still
works: it will see config_id=0 in all packets and accept them.

**Confidence: High** -- complete decompilation of SetCaptureParameters,
exhaustive register list, no config_id writes found.


## Start Packet Emission Timing

### When Start Packets Are Emitted

Start packets (`is_start=1` in chunk header) are emitted **once per
enabled channel at the beginning of each capture**.

Evidence from `DigitalCreatorNode::HandleData` (ELF VA 0x2d51710):

1. The handler maintains per-channel state with a `has_start_packet`
   flag at offset 0x500 + channel * 0x30 in the node object.

2. When a data packet arrives for a channel without `has_start_packet`,
   the handler logs "Expected start packet" and drops the data.

3. When `is_start=1`, the handler:
   a. Checks if config_id matches the current configuration
   b. Stores timestamp + time in per-channel state
   c. Sets `has_start_packet = true`
   d. Logs "Got start packet: ch={} timestamp={}, time={}, config_id={}"

### Configuration Change Detection

When a start packet arrives with a config_id that differs from the
current configuration's ID (stored at `param_1 + 0x2d0`):

```
"Received start digital data for old configuration id: current={}, received={}"
```

This is logged at warning level (3), but the packet is **not dropped**.
The handler continues processing it. This allows the pipeline to handle
late-arriving packets from a previous configuration gracefully.

### Time Ordering Check

When a second start packet arrives for a channel that already has one,
the handler checks:

```
"Jumped back in time - New start time starts before old end time. {} < {}"
```

This is logged at trace level (5) and does not prevent the state reset.

### Per-Channel State Reset

On each start packet (first or subsequent), the per-channel state is
reset:

```c
per_channel[ch].time_reference = 1.25e9;  // 0x41d2a05f20000000
per_channel[ch].start_data = {timestamp, time};  // 16 bytes
per_channel[ch].config_id = header.config_id;
per_channel[ch].sample_counter = 0;
per_channel[ch].has_start_packet = true;
```

**Confidence: High** -- decompiled handler, all state offsets match
`start-packet-format.md`, log strings confirm behavior.


## Implications for Rust Driver

1. **For Logic Pro 8 captures**: config_id will likely be 0 in all
   packets. The driver should accept any consistent config_id value
   (not hardcode expectations).

2. **Start packet handling**: The driver must wait for a start packet
   on each enabled channel before processing data. This is already
   implemented in `capture.rs`.

3. **No need to write config_id registers**: For Spartan-6 devices,
   SetCaptureParameters should not attempt to write config_id
   registers (they don't exist in the 8-bit register space).

4. **Config_id in test mocks**: Integration tests that use config_id=1
   are fine -- the actual value doesn't matter as long as it's
   consistent between start and data packets.


## Cross-References

- `re/notes/start-packet-format.md` -- chunk header bitfield layout
- `re/notes/fpga-register-map.md` -- SCOPE_CONFIG_ID register details
- `doc/protocol/fpga-registers.md` -- established register documentation
- `doc/protocol/capture-sequence.md` -- capture start/stop sequence
- `re/ghidra/decompiled-config-id.txt` -- full decompilation output


## Source Code References (from embedded paths)

```
/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/oscilloscope_instrument.cpp
/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/component/mso_fpga.cpp
/workspace/monorepo/graph-io/devices/logic_interface/src/Logic2FpgaDevice.cpp
/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/logic_analyzer_instrument.cpp
```
