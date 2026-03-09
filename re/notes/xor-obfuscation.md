# SimpleEncryption XOR Obfuscation Scheme

Investigation date: 2026-03-09
Binary: libgraph_server_shared.so v2.4.40
Task: TASK-0049
Depends on: TASK-0028 (ReseedDevice)
Source file: SimpleEncryption.cpp (from assertion metadata)


## Summary

EP1 command traffic is obfuscated using a scheme called "SimpleEncryption"
with two distinct modes depending on device type. For Logic Pro 8/16, all
EP1 commands and responses are XOR'd with a rolling seed derived from a
32-bit LFSR. The Rust driver MUST implement this for Pro devices. For
Logic 4/8 (non-Pro FX3 devices), the XOR is NOT applied to normal
commands -- only during firmware download. Legacy FX2 devices use a
separate substitution cipher.


## DeviceType Enum

Discovered via GetVidPidForDevice switch table at VA 0x1c13df0.

| Value | Device         | VID:PID   | Encryption Mode |
|-------|----------------|-----------|-----------------|
| 0     | Original Logic | 0925:3881 | Substitution    |
| 1     | Logic 16       | 21a9:1001 | Substitution    |
| 2     | (reserved)     | --        | throws          |
| 3     | (reserved)     | --        | throws          |
| 4     | Logic 4        | 21a9:1003 | Seed (fw only)  |
| 5     | Logic 8        | 21a9:1004 | Seed (fw only)  |
| 6     | Logic Pro 8    | 21a9:1005 | Seed (all cmds) |
| 7     | Logic Pro 16   | 21a9:1006 | Seed (all cmds) |

The DeviceType is stored at `this->0x28` in the UsbDevice object and
passed to the SimpleEncryption wrapper as the mode parameter.


## Encryption Mode Selection

The wrapper function at VA 0x2d97ec0 selects the encryption path based
on the mode value:

```
if mode <= 1:
    apply substitution cipher (no seed needed)
elif (mode & 0xFE) == 6:    // DeviceType 6 or 7 (Pro 8 / Pro 16)
    apply seed-based XOR to ALL commands
else:                        // DeviceType 4 or 5 (Logic 4 / Logic 8)
    apply seed-based XOR ONLY during firmware download
```


## Wire Format (Seed-Based XOR, Mode 6-7)

### Command Write

Original command:  `[opcode] [payload_0] [payload_1] ... [payload_N]`

After SetControlByte + XOR:

```
output[0] = (opcode & 0xD7)                        ; control byte (bits 3,5 cleared)
           | (0x08 if firmware_download_mode)        ; bit 3 = firmware flag
           ^ ((seed & 0xFF) & 0xD7)                  ; XOR with masked seed byte 0

output[1] = opcode ^ ((seed >> 0) & 0xFF)            ; original opcode XOR'd
output[2] = payload_0 ^ ((seed >> 8) & 0xFF)         ; payload bytes XOR'd
output[3] = payload_1 ^ ((seed >> 16) & 0xFF)        ; cycling through seed bytes
output[4] = payload_2 ^ ((seed >> 24) & 0xFF)
output[5] = payload_3 ^ ((seed >> 0) & 0xFF)         ; wraps around
...
```

The output is 1 byte longer than the input (control byte prepended).

### Command Read (Response)

Responses are XOR'd without a control byte:

```
for i in 0..response_len:
    response[i] ^= (seed >> ((i % 4) * 8)) & 0xFF
```

No 0xD7 mask, no control byte. Simple cyclic XOR with the full seed.

### Seed Byte Extraction

The 32-bit seed is treated as 4 bytes in little-endian order:

```
byte[0] = (seed >>  0) & 0xFF
byte[1] = (seed >>  8) & 0xFF
byte[2] = (seed >> 16) & 0xFF
byte[3] = (seed >> 24) & 0xFF
```

For the control byte (byte 0 of write output), the seed byte is masked
with 0xD7 before XOR to preserve bits 3 and 5 for the firmware download
flag. All other bytes use the full seed byte.


## LFSR Seed Advancement

After each Write and each Read, both host and firmware advance the seed
using a 32-bit Linear Feedback Shift Register (LFSR).

Location: inlined at VA 0x2d9872a (write path) and 0x2d9975c (read path).
Also exists as standalone function at VA 0x2d99290.

### Iteration Count

The number of LFSR steps per advancement is data-dependent:

```
N = (seed & 0x1F) + 0x23    // range: 35 to 66 iterations
```

### LFSR Step (one iteration)

```c
void lfsr_step(uint32_t *state) {
    uint32_t s = *state;
    uint32_t shifted = s >> 1;
    uint32_t feedback = shifted ^ s;           // taps at bit 0 and 1
    uint32_t tap21 = (s >> 21) ^ feedback;     // tap at bit 21
    tap21 <<= 31;                              // move to MSB position
    uint32_t msb = s & 0x80000000;             // save original MSB
    msb ^= tap21;                              // new MSB from feedback
    uint32_t combined = msb + shifted;
    combined >>= 1;
    uint32_t newval = (shifted ^ (s >> 22)) ^ combined;  // tap at bit 22
    newval <<= 31;
    newval ^= msb;
    newval |= combined;
    *state = newval;
}

void advance_seed(uint32_t *seed) {
    uint32_t n = (*seed & 0x1F) + 0x23;
    for (uint32_t i = 0; i < n; i += 2) {
        lfsr_step(seed);
        if (i + 1 < n) lfsr_step(seed);  // two steps per loop iteration
    }
}
```

### Zero Seed Behavior

If seed = 0x00000000: all LFSR steps produce 0. The seed stays at 0
indefinitely. XOR with zero is identity, so commands pass through
unmodified (except for the control byte prepend).


## Lockstep Requirement

Host and firmware must maintain identical LFSR state. The advancement
order is:

1. Host constructs command, XORs with current seed
2. Host sends XOR'd command via EP1 OUT
3. Host advances its seed (LFSR)
4. Firmware receives, decodes with its seed, advances its seed
5. Firmware constructs response, XORs with current seed
6. Firmware sends XOR'd response via EP1 IN
7. Firmware advances its seed
8. Host receives response, decodes with its seed, advances its seed

Each write-read pair consumes TWO seed advancements (one for write,
one for read). If either side misses a step, all subsequent communication
is garbled.


## Substitution Cipher (Legacy Mode, DeviceType 0-1)

For FX2-based devices (Original Logic, Logic 16), a different cipher is
used. No seed is needed. The cipher is a byte-level substitution with
chained feedback.

### Write Cipher (VA 0x2d981a0)

```
state_a = 0x9B
state_b = 0x54

for each byte in buffer:
    new_b = byte ^ state_b
    new_b ^= 0x2B
    new_b -= 5        // (add 0xFB mod 256)
    new_b ^= 0x35
    new_b -= 0x39     // (add 0xC7 mod 256)
    new_b ^= state_a
    new_b ^= 0x5A
    new_b += 0x50
    new_b ^= 0x38
    new_b -= 0x45     // (add 0xBB mod 256)
    output = new_b
    state_a = byte    // previous plaintext feeds forward
    state_b = new_b   // previous ciphertext feeds back
```

### Read Cipher (VA 0x2d99330, reverse direction)

```
state_a = 0x9B
state_b = 0x54

for each byte in buffer:
    new = byte + 0x45
    new ^= 0x38
    new -= 0x50       // (add 0xB0 mod 256)
    new ^= 0x5A
    new ^= state_a
    new += 0x39
    new ^= 0x35
    new += 5
    new ^= 0x2B
    new ^= state_b
    output = new
    state_b = byte    // previous ciphertext
    state_a = output  // previous plaintext
```


## What the Rust Driver Must Implement

### For Logic Pro 8 (21a9:1005) and Logic Pro 16 (21a9:1006):

**Required -- cannot be bypassed:**

1. `ReseedDevice`: Generate 4 random bytes, send to EP1 OUT, store as seed
2. `SetControlByte`: For each write, prepend a control byte:
   - `control = (opcode & 0xD7) | (0x08 if firmware_download)`
3. `XOR encrypt`: XOR the entire output buffer with cyclic seed bytes
   - Byte 0: XOR with `(seed & 0xFF) & 0xD7`
   - Bytes 1+: XOR with `(seed >> ((i % 4) * 8)) & 0xFF`
4. `XOR decrypt`: XOR received response bytes with cyclic seed bytes
   - All bytes: XOR with `(seed >> ((i % 4) * 8)) & 0xFF` (no mask)
5. `LFSR advance`: After each write AND each read, advance the seed
6. Both sides must stay in lockstep

### For Logic 4 (21a9:1003) and Logic 8 (21a9:1004):

1. `ReseedDevice`: Still required (called before capture start)
2. No XOR on normal EP1 commands
3. XOR only during firmware download (same algorithm as above)

### For Original Logic (0925:3881) and Logic 16 (21a9:1001):

1. Substitution cipher (no seed)
2. Lower priority, may not need implementation initially


## Can the Obfuscation Be Bypassed?

**For Pro devices: No.** The firmware applies the inverse XOR using its
copy of the seed. If the host sends unencrypted data, the firmware will
"decrypt" it and get garbage. The only way to "bypass" would be to send
seed = 0x00000000 so that XOR is identity, but:

1. The control byte is still prepended (firmware expects it)
2. Unknown whether firmware accepts/validates a zero seed
3. The LFSR with seed=0 stays at 0 forever, which is stable, but this
   is untested against hardware

**Recommendation**: Implement the full XOR scheme. It is straightforward:
~50 lines of Rust for the XOR + LFSR + control byte. Attempting to bypass
risks subtle failures and is not worth the saved effort.


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| XOR with cyclic seed bytes | High | SIMD + scalar loops both confirm |
| Control byte prepended on write | High | SetControlByte at VA 0x2d987b0, wrapper at 0x2d97ec0 |
| 0xD7 mask on byte 0 of write | High | Both scalar loops confirm |
| No mask on read XOR | High | Read scalar loop at 0x2d99730 has no 0xD7 |
| LFSR advancement after each op | High | Inlined at both write (0x2d9872a) and read (0x2d9975c) |
| LFSR taps at bits 1, 21, 22 | High | Disassembly is unambiguous |
| Iteration count = (seed & 0x1F) + 0x23 | High | Clear from `and $0x1f` + `add $0x23` |
| Pro devices (6,7) XOR all commands | High | `(mode & 0xFE) == 6` condition at 0x2d98290 |
| Non-Pro FX3 (4,5) XOR only fw download | High | `test %al, %al; jne` at 0x2d98295 |
| Legacy (0,1) use substitution cipher | High | `cmp $0x1; ja` at 0x2d97edc and 0x2d99314 |
| DeviceType enum values | High | Jump table at 0x4719d4 with VID/PID |
| Zero seed stays zero | Medium | Follows from LFSR math, untested on hardware |


## Key Virtual Addresses

| VA | Function |
|----|----------|
| 0x2d97ec0 | SimpleEncryption wrapper (write path) |
| 0x2d987b0 | SetControlByte |
| 0x2d989e0 | XOR encrypt function (standalone, SIMD) |
| 0x2d99290 | LFSR advance (standalone) |
| 0x2d99310 | SimpleEncryption dispatch (read path) |
| 0x2d91e80 | LinuxUsbDevice::Write |
| 0x2d92310 | LinuxUsbDevice::Read |
| 0x2d91950 | LinuxUsbDevice::ReseedDevice |
| 0x1c4e860 | TestFirmwareResponsivnessAndReseed |
| 0x1c13df0 | GetVidPidForDevice |


## References

- `re/notes/reseed-mechanism.md` -- ReseedDevice analysis (TASK-0028)
- `doc/protocol/ep1-commands.md` -- EP1 command protocol
- `re/notes/ep1-command-protocol.md` -- detailed analysis notes
