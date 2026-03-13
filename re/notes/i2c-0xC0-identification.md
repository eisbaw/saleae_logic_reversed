# I2C Device 0xC0 Identification: ATECC508A/608A Crypto Authentication IC

Investigation date: 2026-03-13
Task: TASK-0169
Method: Cross-referencing USB captures, decompiled code, and RE notes


## Summary

The I2C device at address 0xC0 (7-bit: 0x60) is the **Microchip ATECC508A
or ATECC608A** crypto authentication IC. It is NOT an EEPROM. The previous
label "EEPROM" in `re/notes/init-sequence-pcap.md` was incorrect.

The crypto IC is used for ECDSA-based anti-clone device authentication
during initialization. As documented in `re/notes/crypto-authentication.md`,
this authentication is host-software enforced only -- the FX3 firmware does
not gate operations on authentication results. Our Rust driver skips this
entirely and operates correctly without it.


## Evidence for ATECC508A/608A Identification

### 1. I2C Address 0xC0 Matches ATECC Default

The ATECC508A/608A default I2C address is 0xC0 (8-bit write address,
7-bit address 0x60). This was already noted in `re/notes/crypto-authentication.md`:

> "We need to ensure we do not accidentally wake the crypto IC by sending
> traffic to its I2C address (typically 0xC0 for ATECC508A)."

### 2. Command/Response Pattern Matches ATECC Protocol

The I2C traffic in the capture follows the exact ATECC command execution
pattern:

**ATECC command flow:**
1. Write command word to device (I2C write with command opcode + params)
2. Wait for execution (device holds SCL or returns "not ready" on read)
3. Poll for result (read returns status 0x01 = not ready, 0x02 = ready + data)
4. Read result when ready

**Observed in capture (Phase 6):**
```
WriteI2C 0xC0 [0x1C, ...data...]    -- Write command (Nonce, opcode 0x16 in ATECC)
ReadI2C  0xC0 [0x23, 0x00]          -- Poll for 35-byte result
  -> 0x01 (not ready, 3 attempts)
  -> 0x02 + 34 bytes of data (ready)
WriteI2C 0xC0 [0x01, 0x00, 0x02]    -- Idle command (put IC back to sleep)
```

The status bytes (0x01 = not ready, 0x02 = data ready) and the polling
pattern are characteristic of ATECC devices.

### 3. Read Sizes Match ATECC Response Formats

- **35-byte read (reg 0x23)**: ATECC Nonce response = 1 count byte + 32
  bytes nonce + 2 CRC = 35 bytes. The read request encodes 0x23 = 35
  decimal.

- **67-byte read (reg 0x43)**: ATECC Sign/Verify response = 1 count byte
  + 64 bytes signature + 2 CRC = 67 bytes. 0x43 = 67 decimal.

These are NOT register addresses -- they are ATECC response length bytes
passed as part of the I2C read request format.

### 4. Write Data Matches ATECC Commands

The 33-byte write in seq 157:
```
c0 1c 0003 1b16 0000007af7d45d4fd86b1b401f90e9923f04097f7b8f21fc6d
```

Breaking this down:
- `0xC0` = I2C address
- `0x1C` = count byte (28 bytes following) -- wait, actually 0x1C = 28
  This is the ATECC word address / command frame
- `0x00, 0x03` = ATECC Nonce command (opcode 0x16 tunneled)
- The remaining 26 bytes include a 32-byte host nonce for the
  challenge-response protocol

The 13-byte write in seq 179:
```
c0 08 0003 07 41800000 2805
```

This is an ATECC Sign or GenKey command, requesting signature generation.

### 5. Device ID Read Matches ATECC Info Command

The sequence:
```
WriteI2C 0xC0 [0x01, 0x00, 0x02]  -- Wake / status read
ReadI2C  0xC0 [0x04, ...]         -- Read device info
  -> [0x02, 0x04, 0x11, 0x33, 0x43]
```

The value `0x02, 0x04, 0x11, 0x33, 0x43` is a 5-byte ATECC response:
- 0x02 = "ready" status or count
- 0x04, 0x11, 0x33, 0x43 = device revision/type info

### 6. RTTI and Source Code Confirm

From `re/notes/crypto-authentication.md`:
- RTTI: `N6Saleae18CryptoInterfaceProE` (Logic Pro crypto interface)
- Source: `BasicCryptoCommands.cpp`, `crypto_interface.cpp`
- 32-byte block size, 16 slots, zone model = ATECC508A/608A
- ECDSA P-256 challenge-response protocol


## Correcting Previous Labels

The `re/notes/init-sequence-pcap.md` section "I2C EEPROM (Address 0xC0)"
should be relabeled "I2C Crypto IC (ATECC508A/608A, Address 0xC0)". This
device is NOT an EEPROM. The actual EEPROM on the board is at I2C address
0xA0/0xA2 (AT24CXX) and is accessed via ReadVersion (opcode 0x07) with
I2C params, not through the WriteI2C/ReadI2C tunnel.


## "Registers" Reinterpreted

The previously listed "registers" are actually ATECC I2C protocol fields:

| Previously Called | Actually Is | Purpose |
|-------------------|-------------|---------|
| Register 0x01 | Idle command (word addr 0x01) | Put ATECC to idle/sleep state |
| Register 0x04 | Info command response length | Read 4 bytes of device info |
| Register 0x08 | Command word address (0x03) + params | Send ATECC command (tunneled) |
| Register 0x1C | Command with 28-byte payload | Nonce command (0x16) with host nonce |
| Register 0x23 | Response read, 35-byte expected | Read Nonce result (32b nonce + CRC) |
| Register 0x43 | Response read, 67-byte expected | Read Sign result (64b sig + CRC) |

The first byte after the I2C address in WriteI2C/ReadI2C is not a
"register" -- it is either:
- For writes: the ATECC word address (0x03 = command zone, 0x01 = idle)
- For reads: the expected response length


## Relevance to Rust Driver

**None.** Our Rust driver correctly skips the crypto authentication step.
The ATECC508A/608A sits passively on the I2C bus. Not communicating with
it has no effect on device operation. This is a DRM/anti-clone measure
enforced only by Saleae's host software.


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| Device at 0xC0 is ATECC508A/608A | High | I2C address, command/response sizes, polling pattern, RTTI |
| NOT an EEPROM | Confirmed | EEPROM is at different address, accessed via different opcodes |
| Command/response pattern matches ATECC protocol | High | 35-byte and 67-byte reads match Nonce and Sign response sizes |
| Device ID bytes are ATECC revision info | Medium | 5-byte response fits ATECC Info command, but not independently verified |
| No impact on Rust driver | Confirmed | Device operates correctly without crypto auth |


## References

- `re/notes/crypto-authentication.md` -- Full ATECC analysis with RTTI evidence
- `re/notes/i2c-eeprom-protocol.md` -- I2C tunnel protocol
- `re/notes/init-sequence-pcap.md` -- USB capture decode (contains mislabel to correct)
- `re/captures/usbmon4-ep1-decoded.txt` -- Raw decoded capture
- ATECC508A datasheet (Microchip DS20005927) -- I2C address, command format
- ATECC608A datasheet (Microchip DS40002236)
