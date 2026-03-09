# FpgaDeviceFeatures Virtual Method Table

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0045
Method: RTTI relocation analysis, vtable extraction, disassembly, string cross-reference


## Summary

FpgaDeviceFeatures has 8 vtable entries (2 destructors + 6 virtual methods).
ProFpgaDeviceFeatures overrides 2 of the 8 entries: the deleting destructor
(slot 1) and WriteRegisters (slot 3).

The 6 virtual methods are: WriteRegister (single), WriteRegisters (batch),
ReadRegister (single), ReadRegisters (batch), DownloadBitstream, and
ResetFpga. Additionally, two non-virtual member functions exist:
WriteRegisterBit and WriteConfigurationData.


## Vtable Layout

Source file: `/workspace/monorepo/graph-io/devices/logic_interface/src/LogicAnalyzerDevice.cpp`

### FpgaDeviceFeatures (base class)

| Slot | Offset | VA (Fpga) | VA (Pro) | Function | Pro Override |
|------|--------|-----------|----------|----------|:------------:|
| [0]  | 0x00   | 0x1c54e30 | 0x1c54e30 | `~FpgaDeviceFeatures()` [D1 complete] | No |
| [1]  | 0x08   | 0x1c53520 | 0x1c54f80 | `~FpgaDeviceFeatures()` [D0 deleting] | Yes* |
| [2]  | 0x10   | 0x1c53570 | 0x1c53570 | `WriteRegister(U8 addr, U8 val)` | No |
| [3]  | 0x18   | 0x1c535f0 | 0x1c549d0 | `WriteRegisters(vector<pair<U8,U8>>&, const S8*)` | Yes |
| [4]  | 0x20   | 0x1c53990 | 0x1c53990 | `ReadRegister(U8 addr, const S8*) -> U8` | No |
| [5]  | 0x28   | 0x1c53af0 | 0x1c53af0 | `ReadRegisters(vector<U8>&, const S8*) -> vector<U8>` | No |
| [6]  | 0x30   | 0x1c53f20 | 0x1c53f20 | `DownloadBitstream(const U8*, U64, callback, double)` | No |
| [7]  | 0x38   | 0x1c54740 | 0x1c54740 | `ResetFpga()` | No |

Vtable VA: FpgaDeviceFeatures at 0x2e48fb8, ProFpgaDeviceFeatures at 0x2e49008.
Typeinfo VA: FpgaDeviceFeatures at 0x2e49088, ProFpgaDeviceFeatures at 0x2e49098.

\*The Pro deleting destructor is semantically identical to the base version.
Both store the FpgaDeviceFeatures vtable pointer (0x2e48fb8) during
destruction. The compiler generates a separate copy per class as part of
standard C++ destructor chaining.


## Non-Virtual Member Functions

These are regular (non-virtual) member functions of FpgaDeviceFeatures, not
present in the vtable:

| VA | Function | Called By |
|----|----------|-----------|
| 0x1c53e30 | `WriteRegisterBit(U8 reg, U8 bit, U8 val, const S8*)` | Device configuration code |
| 0x1c544b0 | `WriteConfigurationData(const U8*, U32)` | DownloadBitstream (slot 6) |

Additionally, the ReadRegisters implementation has an internal helper at
VA 0x1c53bc0 that performs assertion checking and buffer management.


## EP1 Command Opcodes

Each virtual method maps to specific EP1 command opcodes:

| Method | EP1 Opcode | Wire Format |
|--------|-----------|-------------|
| WriteRegisters | **0x80** | `[0x80] [count] {[addr] [val]}*count` |
| ReadRegister | **0x81** | `[0x81] [0x01] [addr]` -> 1-byte response |
| ReadRegisters | **0x81** | `[0x81] [count] {[addr]}*count` -> multi-byte response |
| WriteConfigurationData | 0x7F | `[0x7F] [len...] [data...]` |
| ResetFpga | 0x7E | `[0x7E]` -> `[0x00]` |

**New finding**: EP1 opcode 0x80 is used for batch register writes through
FpgaDeviceFeatures. This opcode was not previously documented in
`doc/protocol/ep1-commands.md`.

Note: EP1 opcode 0x81 was previously documented for ReadRegisters. The
0x80 opcode for WriteRegisters follows the same pattern (0x80 = write,
0x81 = read), which is consistent with the FX3 firmware command dispatch
design.


## Method Details

### Slot [0]: ~FpgaDeviceFeatures() [D1 Complete Destructor]

VA: 0x1c54e30 (58 bytes)

Non-deleting destructor. Resets the vtable pointer to FpgaDeviceFeatures,
decrements the shared_ptr reference count on the internal USB device object
(at `this+0x10`). Does not free memory.

### Slot [1]: ~FpgaDeviceFeatures() [D0 Deleting Destructor]

VA: 0x1c53520 (base), 0x1c54f80 (Pro)

Deleting destructor. Same as D1 but ends with `jmp operator_delete` to
free the object's memory. The Pro override is byte-for-byte identical in
behavior (only differing in RIP-relative address encoding).

### Slot [2]: WriteRegister (Single)

VA: 0x1c53570 (88 bytes)

```cpp
virtual void WriteRegister(U8 addr, U8 val);
```

Convenience wrapper. Packs `(addr, val)` into a 2-byte vector element,
then calls `this->WriteRegisters(vec, nullptr)` (slot 3, vtable offset
0x18). No label string is passed (third argument is NULL).

Encoding: `addr | (val << 8)` packed into 16 bits, stored as a 2-byte
`pair<U8,U8>` element.

### Slot [3]: WriteRegisters (Batch)

VA: 0x1c535f0 (base, 928 bytes), 0x1c549d0 (Pro, 1120 bytes)

```cpp
virtual void WriteRegisters(std::vector<std::pair<U8, U8>> &pairs,
                             const S8 *label);
```

Batch register write. Sends up to 31 register (address, value) pairs in
a single EP1 OUT transfer using opcode 0x80.

**Wire format**:
```
EP1 OUT: [0x80] [count] [addr0] [val0] [addr1] [val1] ... [addrN] [valN]
```

Constraints:
- `num_registers < 32` (assertion: "num_registers < 32")
- `num_registers >= 1` (exits early if count is zero)
- Error: "too many registers for single write command" if limit exceeded

**Base vs Pro difference**:
- Base: fire-and-forget. Calls UsbDevice::vtable[0x20] (EP1 OUT write) only.
- Pro: write-then-verify. Calls UsbDevice::vtable[0x20] (EP1 OUT write)
  followed by UsbDevice::vtable[0x28] (EP1 IN read) to check the response.
  String reference: "write register response check".

This is the only behaviorally significant Pro override. The Pro firmware
likely sends a confirmation byte after processing register writes, and
the Pro host code verifies it.

### Slot [4]: ReadRegister (Single)

VA: 0x1c53990 (352 bytes)

```cpp
virtual U8 ReadRegister(U8 addr, const S8 *label);
```

Single register read using EP1 command opcode 0x81.

**Wire format**:
```
EP1 OUT: [0x81] [0x01] [addr]     (3 bytes: opcode, count=1, address)
EP1 IN:  [value]                   (1 byte response)
```

Sends via UsbDevice::vtable[0x20] with 3 bytes, reads 1 byte via
UsbDevice::vtable[0x28]. Returns the response byte.

String reference: "ReadRegister" (label for logging).

Note: This returns a single U8, not a 32-bit register value. The 32-bit
register read path documented in `doc/protocol/fpga-registers.md` uses a
different code path (MsoFpga direct register access, not through
FpgaDeviceFeatures).

### Slot [5]: ReadRegisters (Batch)

VA: 0x1c53af0 (208 bytes + helper at 0x1c53bc0)

```cpp
virtual std::vector<U8> ReadRegisters(std::vector<U8> &addrs,
                                       const S8 *label);
```

Batch register read using EP1 command opcode 0x81.

**Wire format**:
```
EP1 OUT: [0x81] [count] [addr0] [addr1] ... [addrN]
EP1 IN:  [val0] [val1] ... [valN]     (count bytes)
```

Constraints:
- `1 <= count <= 63` (from `(size-1) >= 0x3E` overflow check)
- Assertion: "Invalid number of registers" (in helper at 0x1c53bc0)
- Assertion: "false" with full method signature for invariant violations

### Slot [6]: DownloadBitstream

VA: 0x1c53f20 (1424 bytes)

```cpp
virtual void DownloadBitstream(const U8 *data, U64 size,
                                callback_t progress, double timing_param);
```

Downloads an FPGA bitstream to the device in chunks. Sequence:

1. Record start time (`steady_clock::now()`)
2. Call `this->ResetFpga()` (slot 7, vtable offset 0x38)
3. Query USB speed to determine chunk size:
   - USB 2.0 High-Speed: 62 bytes per chunk
   - USB 3.0 SuperSpeed: 508 bytes per chunk (or larger with USB version check)
4. Loop: call `WriteConfigurationData(chunk, size)` for each chunk
5. Log: "Bitstream download took {X} seconds, over {N} sections."

Calls non-virtual `WriteConfigurationData` (at 0x1c544b0) twice in the
function body -- once per USB speed path (USB 2.0 and USB 3.0).

The `WriteConfigurationData` function uses EP1 opcode 0x7F:
- USB 2.0: `[0x7F] [len_8bit] [data...]` (max 62 bytes data)
- USB 3.0: `[0x7F] [len_lo] [len_hi] [data...]` (max 509 bytes data)
- Response: `[0x00]` for success

### Slot [7]: ResetFpga

VA: 0x1c54740 (656 bytes)

```cpp
virtual void ResetFpga();
```

Resets the FPGA before bitstream programming. Sends EP1 opcode 0x7E and
expects a single-byte `0x00` response.

**Wire format**:
```
EP1 OUT: [0x7E]                   (1 byte)
EP1 IN:  [0x00]                   (1 byte, success)
```

Uses UsbDevice::vtable[0x20] (EP1 OUT write) with label "ResetFpga",
then UsbDevice::vtable[0x28] (EP1 IN read) with label "ResetFpga response".

On response != 0x00, throws an exception. The exception message is
constructed by concatenating `(` + a hex-formatted string + `fpga` + `)`,
suggesting the error format is something like "(0xNN fpga)".


## UsbDevice Vtable Offsets Used

FpgaDeviceFeatures methods access the underlying USB device through the
object at `this+0x08` (a shared_ptr<UsbDevice>). The UsbDevice vtable
offsets used:

| Offset | Purpose | Used By |
|--------|---------|---------|
| 0x20 | EP1 OUT bulk write (command send) | WriteRegisters, ReadRegister, ReadRegisters, ResetFpga, WriteConfigurationData |
| 0x28 | EP1 IN bulk read (response receive) | ReadRegister, ResetFpga, Pro WriteRegisters |

Note: The vtable at `this+0x18` mentioned in ep1-commands.md is accessed
from MsoFpga, not from FpgaDeviceFeatures. FpgaDeviceFeatures uses the
UsbDevice at `this+0x08`.


## Object Layout (Partial)

```
FpgaDeviceFeatures:
  +0x00: vtable pointer
  +0x08: shared_ptr<UsbDevice> (USB transport)
  +0x10: shared_ptr<???> (ref-counted resource, decremented in destructor)
  +0x18: mutex or buffer (passed as EP1 write context)
  +0x20: mutex or buffer (passed as EP1 read context)
```


## Pro Override Analysis

ProFpgaDeviceFeatures overrides only two vtable slots:

1. **Slot [1]: Deleting destructor** -- Compiler-generated, semantically
   identical to base. Required by C++ ABI for each concrete class.

2. **Slot [3]: WriteRegisters** -- Adds response verification after EP1
   write. The base version is fire-and-forget; the Pro version reads back
   a confirmation byte after each batch write. This ensures register writes
   are acknowledged by the firmware, which is important for Pro devices
   that may have more complex FPGA register interfaces.

The Pro override does NOT change:
- Register addressing width (both use U8 addresses in the pair)
- Command opcode (both use 0x80)
- Maximum register count (both enforce < 32)
- Wire format structure

The only behavioral difference is the read-back verification step.


## Relationship to Existing Documentation

### Updated understanding of EP1 commands:
- Opcode 0x80 (WriteRegisters): **New** -- batch register write via FpgaDeviceFeatures
- Opcode 0x81 (ReadRegisters): Was documented for 3-byte format. Now confirmed
  to also support batch reads with count > 1.
- Opcode 0x7E (ResetFpga): Confirmed, matches existing docs.
- Opcode 0x7F (WriteConfigurationData): Confirmed, matches existing docs.

### Register addressing:
The FpgaDeviceFeatures virtual methods use **8-bit register addresses** (U8),
not the 16-bit addresses documented in `doc/protocol/fpga-registers.md`.
The 16-bit register addresses are used by the MsoFpga direct register access
path (vtable offset 0x18 on the USB device object), which is a separate
code path that bypasses FpgaDeviceFeatures entirely.

This means there are two register access modes:
1. **FpgaDeviceFeatures path**: 8-bit addresses, uses opcodes 0x80/0x81,
   goes through EP1 command protocol with opcode prefix
2. **MsoFpga direct path**: 16-bit addresses, no opcode prefix, raw
   2-byte/6-byte packets on EP1

The MsoFpga path is used for the register map documented in
`doc/protocol/fpga-registers.md` and `re/notes/fpga-register-map.md`.
The FpgaDeviceFeatures path is used for higher-level device configuration
during boot and capture setup.


## Open Questions

1. **ReadRegister response size**: Slot [4] reads only 1 byte of response,
   but `doc/protocol/fpga-registers.md` documents 32-bit register values.
   Does the FpgaDeviceFeatures ReadRegister return a byte-wide register
   value (different register space), or is the response parsing more
   complex than the disassembly suggests?

2. **DownloadBitstream timing parameter**: The `xmm0` double parameter
   to DownloadBitstream is used in timing calculations but its exact
   purpose (timeout, progress interval, expected duration) is unclear.

3. **WriteRegisters batch format**: The 0x80 opcode format `[0x80]
   [count] {[addr][val]}*` uses 8-bit addresses and 8-bit values. How
   does this relate to the 16-bit addr / 32-bit value format used by
   MsoFpga? Are these different register spaces, or does the FX3 firmware
   translate 8-bit addresses to 16-bit before forwarding to the FPGA?

4. **Pro response byte**: What is the format and meaning of the response
   byte that the Pro WriteRegisters reads back? Is it a simple ACK (0x00)
   or does it contain status information?

5. **ReadRegisters batch limit**: The batch read appears to allow up to
   63 registers (0x3E check), while batch writes are limited to 31 (< 32).
   Is this asymmetry intentional?


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| Vtable has 8 entries (2 dtors + 6 methods) | **Confirmed** | Relocation table extraction, all 8 entries verified |
| Pro overrides slots [1] and [3] | **Confirmed** | Different function pointers in relocation data |
| Slot [0] = D1 complete destructor | **Confirmed** | Stores vtable ptr, decrements refcount, no delete |
| Slot [1] = D0 deleting destructor | **Confirmed** | Same as D1 plus `jmp operator_delete` |
| Slot [2] = WriteRegister (single) | **Confirmed** | Packs pair, calls vtable[0x18] = slot[3] |
| Slot [3] = WriteRegisters (batch) | **Confirmed** | Assertion string "virtual void FpgaDeviceFeatures::WriteRegisters(...)" |
| Slot [4] = ReadRegister (single) | **High** | Uses 0x81 opcode, reads 1 byte, string "ReadRegister" |
| Slot [5] = ReadRegisters (batch) | **High** | Uses 0x81 opcode, iterates addresses, helper has assertion string |
| Slot [6] = DownloadBitstream | **Confirmed** | String "DownloadBitstream", calls ResetFpga + WriteConfigurationData |
| Slot [7] = ResetFpga | **Confirmed** | String "ResetFpga", sends opcode 0x7E, checks 0x00 response |
| EP1 opcode 0x80 = WriteRegisters | **Confirmed** | `movb $0x80` in both base and Pro WriteRegisters |
| Pro adds response verification | **Confirmed** | Base has 1 UsbDevice vtable call, Pro has 2, string "write register response check" |
| WriteConfigurationData is non-virtual | **Confirmed** | Not in vtable, called directly by DownloadBitstream |
| WriteRegisterBit is non-virtual | **Confirmed** | Not in vtable, has own assertion string at separate VA |
