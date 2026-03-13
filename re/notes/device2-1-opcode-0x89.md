# Opcode 0x89 (Device2_1): I2C Wake Token for ATECC Crypto IC

Investigation date: 2026-03-13
Task: TASK-0169
Method: USB capture correlation, decompiled code analysis


## Summary

EP1 opcode 0x89 ("Device2_1") is the **I2C wake token** command for the
ATECC508A/608A crypto authentication IC. It must be sent before every I2C
transaction with the crypto IC because the ATECC enters a low-power idle/
sleep state between commands and requires a wake-up pulse to become
responsive.


## Evidence

### 1. Always Precedes Crypto IC I2C Operations

In the USB capture (`re/captures/usbmon4-ep1-decoded.txt`), Device2_1
(0x89) appears exactly 14 times, always immediately before a WriteI2C or
ReadI2C operation to I2C address 0xC0 (the ATECC crypto IC):

```
seq 147: Device2_1     -> 0x00 (ACK)
seq 149: WriteI2C c0...  (write to ATECC)

seq 151: Device2_1     -> 0x00 (ACK)
seq 153: WriteI2C c0...  (write to ATECC)

seq 155: Device2_1     -> 0x00 (ACK)
seq 157: WriteI2C c0...  (write Nonce command to ATECC)

seq 159: Device2_1     -> 0x00 (ACK)
seq 161: ReadI2C c0...   (read Nonce result from ATECC)
...
```

Every single I2C operation to 0xC0 is preceded by a Device2_1 command.
No Device2_1 appears anywhere else in the capture -- it is exclusive to
the crypto IC communication phase.

### 2. ATECC508A Requires Wake Before Every Transaction

The ATECC508A/608A datasheet specifies:
- Device enters idle state after processing a command
- Device must be woken via a low-level I2C wake (SDA held low for tWLO)
- Without wake, the device will not respond to I2C reads/writes

The string `"Failed to send i2c wake token"` in the binary (at VA 0x427847)
confirms the host software has a dedicated I2C wake operation.

### 3. Code Region Matches

From `re/notes/i2c-eeprom-protocol.md`:
```
VA 0x1c51900-0x1c51E00: Device2_1 (opcode 0x89), SetIdToDevice, WriteRead-Write/Read
```

This VA range is in the same code region as the I2C tunnel operations
(WriteI2C 0x87, ReadI2c 0x88), confirming Device2_1 is part of the I2C
subsystem.

### 4. Packet Format

```
Host -> Device (EP1 OUT):  [0x89]     (1 byte, opcode only)
Device -> Host (EP1 IN):   [0x00]     (1 byte, ACK/success)
```

No parameters needed -- the FX3 firmware knows the crypto IC I2C address
and performs the wake sequence (SDA pulse) on the I2C bus.


## How the FX3 Firmware Implements It

The FX3 firmware likely implements opcode 0x89 as:
1. Drive SDA low for the required wake time (tWLO, typically 60us)
2. Release SDA
3. Wait for the ATECC to wake (tWHI, typically 1500us)
4. Return success (0x00)

This matches the CyU3PI2C SDK functions available on the FX3. The wake
is a special I2C bus condition, not a normal I2C write, which is why it
needs its own EP1 opcode rather than being tunneled through WriteI2C (0x87).


## Relevance to Rust Driver

**None.** Since we skip crypto authentication entirely, we never need to
send opcode 0x89. The ATECC IC remains in its sleep state and does not
interfere with other I2C bus operations.


## Suggested Rename

The opcode should be renamed from "Device2_1" (a placeholder name from
initial analysis) to "WakeI2cCryptoIC" or "I2cWakeToken" in the decoder
scripts and protocol documentation.


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| Device2_1 is I2C wake for ATECC | High | Always precedes 0xC0 I2C ops, matches ATECC wake requirement |
| 1-byte opcode, 1-byte ACK response | Confirmed | All 14 instances in capture show this format |
| Not needed by Rust driver | Confirmed | Driver operates without crypto auth |


## References

- `re/captures/usbmon4-ep1-decoded.txt` -- 14 instances of Device2_1
- `re/notes/crypto-authentication.md` -- ATECC identification and auth analysis
- `re/notes/i2c-0xC0-identification.md` -- I2C 0xC0 = ATECC508A/608A
- `re/notes/i2c-eeprom-protocol.md` -- VA range for opcode 0x89
- ATECC508A datasheet section 6.1 -- I2C wake sequence
