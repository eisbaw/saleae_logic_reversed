# Logic Pro 8 Initialization Sequence (from pcap)

Investigation date: 2026-03-11
Source: `~/topics/saleae_logic_8_pro/data-002/usb.logic8pro.pcap`
Device: Logic Pro 8 (21a9:1005)
Task: TASK-0066 AC#4
Decoded by: `re/scripts/decode-pcap-init.py`
Status: **COMPLETE** -- all 928 EP1 frames decoded, 0 unknown opcodes


## Key Discovery: Reseed Encryption Uses Fixed Key

Reseed packets are ALWAYS encrypted with the initial/default LFSR seed
(0x354b248e), NOT the current rolling LFSR state. This allows the device to
decode reseed packets regardless of the current LFSR state -- the device
always knows the fixed key for reseeds.

Normal commands use the rolling seed that advances after each write/read.
This was validated across all 4 reseeds in the pcap (frames 15, 1627, 1639,
1651), all of which decode correctly with the initial seed and produce
garbage when decoded with the current rolling seed.

**Impact on the Rust driver**: The `encrypt_reseed` function in
`obfuscation.rs` currently uses `self.seed` (the current rolling state).
This works for the FIRST reseed (when current == initial), but will fail
for subsequent reseeds. The driver must either:
1. Store the initial seed separately and use it for all reseed operations, or
2. Create a fresh `SimpleEncryption` with the initial seed just for reseeds.


## Summary Statistics

- Total EP1 frames: 928 (464 command/response pairs)
- Total decoded operations: 261 (including reseeds and summarized FPGA upload)
- Reseeds: 4
- Unknown opcodes: 0
- FPGA bitstream: 334 chunks, 340,604 bytes
- Time span: 12.96s to 97.04s (84s total, but most is idle/capture time)


## Initialization Phases

The initialization follows a clear sequence of phases:

### Phase 1: Initial Handshake (12.956s, frames 15-36)

| Seq | Frame | Command | Details |
|-----|-------|---------|---------|
| 1 | 15 | RESEED #1 | -> 0x6fe77bc8 |
| 2 | 17 | Ping | test=0x55, response=0xAA (OK) |
| 3 | 21 | GetDeviceInfo | Response: "Logic Pro Firmware Compiled Oct 25 2022 16:42:00" |
| 4 | 25 | ReadVersion | params=[0x33,0x81,0x08,0x00,0x08,0x00] |
| 5 | 29 | ReadVersion | params=[0x33,0x81,0x20,0x00,0x06,0x00], response="REV." |
| 6 | 33 | ResetFpga | response=0x00 (success) |

After reseed, the host sends Ping to verify communication, then queries
device identity and firmware version. ResetFpga prepares for bitstream
upload.


### Phase 2: FPGA Bitstream Upload (12.963s-14.697s, frames 37-1369)

334 WriteConfigData (0x7F) chunks totaling 340,604 bytes of FPGA bitstream.
Each chunk is ~1020 bytes + 2-byte length prefix. Each chunk gets a 1-byte
ACK response of 0x00. The upload takes approximately 1.7 seconds.


### Phase 3: Post-FPGA Register Initialization (14.697s-14.779s, frames 1373-1624)

Extensive register configuration following FPGA load. This includes:

#### 3a: FPGA ID Verification (frames 1373-1384)
```
WriteRegisters r0x7f=0xaa  (write magic value)
WriteRegisters r0x7f=0xaa  (write again)
ReadRegisters  r0x7f       -> 0xaa (read back to verify FPGA is responsive)
```

#### 3b: ADC Init / Bank Configuration (frames 1385-1468)
Multiple 4-register writes using the pattern `[r0x03, r0x16, r0x04, r0x05]`:
- r0x03 = register address (bank select)
- r0x16 = always 0x00
- r0x04 = register value
- r0x05 = always 0x00

This writes to internal ADC registers via the FPGA's register proxy:
- Bank 0x08: values 0x03, 0x00, 0x01, 0x00 (reset sequence?)
- Bank 0x0d: value 0x00
- Bank 0x15: value 0x21
- Bank 0x21: value 0xc2
- Bank 0x05: value 0x0f
- Bank 0x04: value 0x0f
- Bank 0x10: value 0x00
- Bank 0x18: value 0x04

Then single register writes:
- r0x15=0x00 (bank power off?)
- Bank 0x08: value 0x01 (enable?)

#### 3c: JESD Link Establishment (frames 1429-1508)

Power-down sequence (r0x00 transitions):
```
r0x00=0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00
```
This is a staged power-up: clearing bits 0, 2, 4, 3 sequentially.

Then status polling:
```
ReadRegisters r0x40 -> 0x00 (first attempt: not ready)
ReadRegisters r0x4a -> 0x02
```

Repeated after ~50ms delay:
```
r0x00=0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00  (second power cycle)
ReadRegisters r0x40 -> 0xe0 (JESD link established!)
ReadRegisters r0x4a -> 0x02
ReadRegisters r0x19 -> 0x00
ReadRegisters r0x40 -> 0xe0 (confirmed)
ReadRegisters r0x4a -> 0x00
```

r0x40=0xe0 indicates successful JESD204B link establishment.

#### 3d: Post-JESD Configuration (frames 1509-1520)
```
WriteRegisters r0x00=0x00
WriteRegisters r0x00=0x40
ReadVersion params=[0x33,0x81,0x10,0x00,0x10,0x00]  -> 16 bytes (device calibration data?)
```

#### 3e: EEPROM/Calibration Data Write (frames 1521-1584)

16 individual writes to r0x17 with values:
```
0x46, 0xff, 0xc6, 0xa6, 0x60, 0xb7, 0xde, 0x07,
0x67, 0x8b, 0x03, 0x66, 0x11, 0x35, 0x7c, 0x7f
```
These appear to be calibration coefficients loaded into the FPGA.

#### 3f: Third Power Cycle (frames 1585-1624)
```
Bank 0x08: value 0x01
r0x00=0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00
ReadRegisters r0x40 -> 0x00
ReadRegisters r0x4a -> 0x00
ReadRegisters r0x19 -> 0x00
WriteRegisters r0x12=0x04
```


### Phase 4: Second Reseed + Channel Config (14.935s, frames 1627-1636)

| Seq | Frame | Command | Details |
|-----|-------|---------|---------|
| 139 | 1627 | RESEED #2 | -> 0x6a300ceb (156ms gap from Phase 3) |
| 140 | 1629 | Ping | test=0x55, response=0xAA |
| 142 | 1633 | WriteRegisters | count=3: r0x0f=0x00, r0x10=0x0f, r0x11=0x00 |

Registers 0x0f-0x11 appear to configure digital channel enables:
- r0x0f = 0x00 (no analog channels?)
- r0x10 = 0x0f (enable digital channels 0-3?)
- r0x11 = 0x00 (no additional channels?)


### Phase 5: Third Reseed + Updated Channel Config (36.375s, frames 1639-1648)

21.4 second gap from Phase 4 (user interaction in Saleae software).

| Seq | Frame | Command | Details |
|-----|-------|---------|---------|
| 144 | 1639 | RESEED #3 | -> 0x3bb32b1d |
| 145 | 1641 | Ping | test=0x55, response=0xAA |
| 147 | 1645 | WriteRegisters | count=3: r0x0f=0x0f, r0x10=0x0f, r0x11=0x0f |

Channel config updated to enable all channels (0x0f in all three regs).


### Phase 6: Fourth Reseed + EEPROM Calibration (36.690s, frames 1651-1752)

| Seq | Frame | Command | Details |
|-----|-------|---------|---------|
| 149 | 1651 | RESEED #4 | -> 0x44faeb97 |
| 150 | 1653 | Ping | test=0x55, response=0xAA |
| 152 | 1657 | Device2_1 | response=0x00 (I2C wake for ATECC crypto IC) |

Then ATECC508A/608A crypto authentication via I2C address 0xC0:

#### ATECC Wake + Status Read
```
Device2_1                           -> wake ATECC from sleep
WriteI2C addr=0xC0 [0x01,0x00,0x02] -> idle/status command
```

#### ATECC Nonce Command (Challenge-Response Step 1)
```
Device2_1                           -> wake ATECC
WriteI2C addr=0xC0 [0x1C,...26B...] -> Nonce command with host random nonce
Device2_1                           -> wake for read
ReadI2C  addr=0xC0 [0x23,0x00]      -> poll 35-byte Nonce response
  -> 0x01 (not ready, 3 attempts)
  -> 0x02 + 34 bytes (nonce result)
WriteI2C addr=0xC0 [0x01,0x00,0x02] -> idle command
```

#### ATECC Sign Command (Challenge-Response Step 2)
```
Device2_1                           -> wake for Sign command
WriteI2C addr=0xC0 [0x08,...10B...] -> Sign command (sign digest with slot key)
Device2_1                           -> wake for read
ReadI2C  addr=0xC0 [0x43,0x00]      -> poll 67-byte Sign response
  -> 0x01 (not ready, 5 attempts)
  -> 0x02 + 66 bytes (64B ECDSA signature + CRC)
WriteI2C addr=0xC0 [0x01,0x00,0x02] -> idle command
```

The host verifies the ECDSA signature using OpenSSL (host-side), not on
the ATECC IC. See `re/notes/crypto-authentication.md` for details. Our
Rust driver skips this entire phase.


### Phase 7: Capture Configuration (36.809s-36.873s, frames 1753-1846)

#### ADC Re-initialization
Same bank register pattern as Phase 3b:
```
WriteRegisters r0x15=0x03  (bank power on?)
Bank 0x08: values 0x00, 0x03, 0x00
```

#### JESD Re-establishment
```
r0x00=0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00
ReadRegisters r0x40 -> 0xe0 (JESD OK)
ReadRegisters r0x4a -> 0x02
ReadRegisters r0x19 -> 0x00
ReadRegisters r0x40 -> 0xe0 (confirmed)
ReadRegisters r0x4a -> 0x00
```

#### Capture Parameters
```
WriteRegisters count=12:
  r0x00=0x00  r0x08=0xff  r0x06=0xff  r0x0a=0x20
  r0x0b=0x10  r0x0c=0x00  r0x1a=0x00  r0x0d=0x18
  r0x0e=0x08  r0x12=0x02  r0x13=0x00  r0x14=0x10

ReadRegisters (same 12 addrs) -> verify written correctly
```

#### Start Capture
```
StartSampling (0x01) -> response [0x07, 0x00]
WriteRegisters r0x00=0x01  (trigger capture?)
```

60 second gap (data streaming on EP2).


### Phase 8: Capture Teardown (97.036s, frames 5446-5497)

```
WriteRegisters r0x00=0x00  (stop capture)
StopStream (0x02)          -> response [0x07, 0x00]
ReadRegisters r0x40 -> 0xe0
ReadRegisters r0x4a -> 0x00
WriteRegisters r0x15=0x00  (bank power off)
Bank 0x08: value 0x01
r0x00=0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00  (power down sequence)
ReadRegisters r0x40 -> 0x00 (JESD link down)
ReadRegisters r0x4a -> 0x00
```


## Reseed Pattern

Every reseed follows the same protocol:
1. RESEED packet (5 bytes, encrypted with initial seed 0x354b248e)
2. Ping (3 bytes, test=0x55, encrypted with new seed)
3. Ping response (1 byte, 0xAA)
4. Configuration command (varies)

The Ping after reseed serves as validation that the new seed was correctly
established on both sides.

All 4 reseeds in this capture:

| # | Frame | Time | New Seed | Context |
|---|-------|------|----------|---------|
| 1 | 15 | 12.956s | 0x6fe77bc8 | Initial connection |
| 2 | 1627 | 14.935s | 0x6a300ceb | After FPGA init complete |
| 3 | 1639 | 36.375s | 0x3bb32b1d | User changed channel config |
| 4 | 1651 | 36.690s | 0x44faeb97 | Before EEPROM calibration |


## Register Map (Partial, from Observed Usage)

| Addr | Read/Write | Purpose (inferred) |
|------|-----------|---------------------|
| 0x00 | RW | Power control / capture enable |
| 0x03 | W | ADC bank address (indirect register access) |
| 0x04 | W | ADC bank value (indirect register access) |
| 0x05 | W | ADC bank value high (always 0x00) |
| 0x06 | RW | Channel mask? (set to 0xFF) |
| 0x08 | RW | Channel mask 2? (set to 0xFF) |
| 0x0a | RW | Sample config (0x20) |
| 0x0b | RW | Sample config (0x10) |
| 0x0c | RW | Sample config (0x00) |
| 0x0d | RW | Timing config (0x18) |
| 0x0e | RW | Timing config (0x08) |
| 0x0f | RW | Digital channel enable low |
| 0x10 | RW | Digital channel enable mid |
| 0x11 | RW | Digital channel enable high |
| 0x12 | RW | Capture config (0x02 or 0x04) |
| 0x13 | RW | Capture config (0x00) |
| 0x14 | RW | Capture config (0x10) |
| 0x15 | RW | Bank power control (0x00=off, 0x03=on) |
| 0x16 | W | ADC bank address high (always 0x00) |
| 0x17 | W | Calibration coefficient storage |
| 0x19 | R | Status register |
| 0x1a | RW | Config (0x00) |
| 0x40 | R | JESD link status (0xe0=established, 0x00=down) |
| 0x4a | R | JESD status detail |
| 0x7f | RW | FPGA ID register (magic 0xAA readback test) |


## JESD204B Link Power Sequence

The power-up/down of the JESD link follows a specific bit-clearing pattern
on register 0x00:

```
Power up:   0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00
Bit pattern: clear bit 0, then bit 1, then bit 2, then bit 4
```

After the sequence completes, register 0x40 is polled for 0xe0 (link
established). The first attempt after FPGA load fails (returns 0x00),
but after a ~50ms delay and second attempt, the link establishes.


## I2C Crypto IC (ATECC508A/608A, Address 0xC0)

The device communicates with an ATECC508A/608A crypto authentication IC at
I2C address 0xC0 (7-bit: 0x60). This is NOT an EEPROM -- it is a crypto
IC for anti-clone authentication (see `re/notes/i2c-0xC0-identification.md`).

The bytes previously labeled as "register addresses" are actually ATECC
protocol fields (command word addresses and response lengths):

- **0x01**: Idle command -- put ATECC to sleep between operations
- **0x04**: Info command response read (4 bytes of device revision info)
- **0x08**: Command word address with Sign/GenKey params
- **0x1C**: Nonce command with 28-byte payload (host nonce for challenge)
- **0x23**: Read 35-byte Nonce response (32b nonce + count + CRC)
- **0x43**: Read 67-byte Sign response (64b ECDSA signature + count + CRC)

Read operations require polling: status 0x01 = ATECC still computing,
status 0x02 = result ready + data. This matches ATECC command execution
latency (crypto operations take milliseconds).


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| Reseed uses initial seed, not rolling | Confirmed | All 4 reseeds decode correctly; fails with rolling seed |
| All opcodes in capture are known | Confirmed | 0 unknown opcodes across 928 frames |
| FPGA bitstream = 340,604 bytes | Confirmed | Sum of chunk lengths from WriteConfigData |
| JESD link status in r0x40 | High | Consistent 0x00->0xe0 transition after power cycle |
| Channel enable in r0x0f-0x11 | Medium | Values change between reseeds, correlate with config changes |
| Calibration in r0x17 | Medium | 16 sequential writes after version read, plausible pattern |
| Crypto IC (ATECC) at I2C 0xC0 | High | I2C address, polling pattern, response sizes match ATECC protocol |


## References

- `re/scripts/decode-pcap-init.py` -- decoder script used to generate this analysis
- `re/notes/xor-obfuscation.md` -- LFSR encryption scheme documentation
- `crates/saleae-proto/src/obfuscation.rs` -- Rust implementation of encryption
- `doc/protocol/ep1-commands.md` -- EP1 command opcode reference
