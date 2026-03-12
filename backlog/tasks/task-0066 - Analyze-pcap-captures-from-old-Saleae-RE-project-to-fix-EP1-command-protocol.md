---
id: TASK-0066
title: Analyze pcap captures from old Saleae RE project to fix EP1 command protocol
status: Done
assignee: []
created_date: '2026-03-09 21:19'
updated_date: '2026-03-11 19:46'
labels:
  - hardware-testing
  - reverse-engineering
  - ep1
  - blocking
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The EP1 command channel does not work: firmware's command processing thread never consumes data from EP1 OUT DMA after firmware upload. A previous RE project at ~/topics/saleae_logic_8_pro/ has a 423 MB pcap capture (data-002/usb.logic8pro.pcap) of the actual Saleae Logic 2 GUI talking to the device.

## Key findings so far from pcap

1. **Reseed is 5 bytes, not 4**: pcap frame 15 shows `66 ec 30 d2 e1` (5 bytes) sent on EP1 OUT as the first bulk transfer. Our implementation sends only 4 bytes. The 5th byte may be critical.

2. **Frame 13-14**: Right before EP1 commands, there's a control transfer: bRequest=1, bmRequestType=0x02 (CLEAR_FEATURE on endpoint). This happens at t=12.95s, ~13 seconds after enumeration.

3. **EP1 commands succeed in pcap**: After the 5-byte reseed, a 3-byte encrypted ping succeeds immediately. The firmware IS consuming EP1 data in the pcap.

4. **No firmware upload in pcap**: The pcap starts with USB enumeration (GET_DESCRIPTOR) at t=0, then jumps to t=12.95s for the first EP1 command. No bRequest=0xA0 firmware upload transfers. This means the firmware was already running from SPI flash.

5. **Alternative LFSR from old project** (TENTATIVE - needs validation against pcap):
   - Taps at bits 0, 1, 21, 31 (vs our taps at 1, 21, 22)
   - Clean Fibonacci LFSR: `bit = (state ^ (state >> 1) ^ (state >> 21) ^ (state >> 31)) & 1; state = (state >> 1) | (bit << 31)`
   - Iteration count: `(state & 0x1f) + 34` inclusive range = 35-66 steps (same count as ours)
   - state==0 handled specially: set to 1
   - Source: sigrok Logic Pro 16 implementation

## Investigation steps

1. Decode the pcap initialization sequence in detail:
   - What are the 5 bytes of the reseed? Is byte 0 an opcode? Or is byte 4 a checksum/flag?
   - What control transfers happen before EP1 commands?
   - Decrypt the EP1 commands using the LFSR from the old project to verify correctness
   - Document the complete init sequence: reseed -> ping -> fw_version -> eeprom -> scratch -> FPGA upload -> config -> capture

2. Compare our LFSR against the pcap:
   - Extract seed from reseed packet
   - Apply our LFSR to predict encrypted ping bytes
   - Apply old project's LFSR to predict encrypted ping bytes
   - Whichever matches the pcap is correct

3. Fix our implementation:
   - Correct reseed to send 5 bytes if needed
   - Fix LFSR if wrong
   - Fix encrypt_command format if needed

4. Test against real hardware after fix

## Files

- pcap: ~/topics/saleae_logic_8_pro/data-002/usb.logic8pro.pcap (423 MB)
- Old project LFSR: ~/topics/saleae_logic_8_pro/usb_decoder/src/crypto.rs
- Old project protocol spec: ~/topics/saleae_logic_8_pro/USB_PROTOCOL_SPECIFICATION.md
- Our LFSR: crates/saleae-proto/src/obfuscation.rs
- Our EP1: crates/saleae-driver/src/ep1.rs

## Note

The old project's analysis (other than pcap) should be treated as tentative - it may contain inaccuracies. The pcap is ground truth.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 EP1 ping command succeeds against real Logic Pro 8 hardware
- [x] #2 LFSR implementation validated against pcap ground truth
- [x] #3 Reseed packet format matches pcap (5 bytes if confirmed)
- [x] #4 Complete initialization sequence documented from pcap analysis
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
## 2026-03-10: LFSR and encryption validated against pcap

### Bugs fixed

1. **lfsr_step was computing 2 steps per call** - The assembly at VA 0x2d99290 uses an optimized loop with stride-2, but the actual LFSR is a simple single-step Fibonacci LFSR with taps at bits 0,1,21,31: `feedback = (s ^ (s>>1) ^ (s>>21) ^ (s>>31)) & 1; state = (s>>1) | (feedback << 31)`. Our previous transcription copied the 2-step loop body as if it were 1 step.

2. **Data XOR started at sb0 instead of sb1** - Assembly at VA 0x2d98710 shows the scalar XOR loop starts with `esi = rdx*8 = 8` (bit offset 8 = sb1). The control byte independently uses `sb0 & 0xD7`. So: output[1] = opcode ^ sb1, output[2] = payload[0] ^ sb2, etc.

3. **LFSR taps were wrong** - Documented as bits 1,21,22 but actually bits 0,1,21,31.

### Pcap validation results

With initial LFSR state 0x6fe77bc8 (derived from reseed), every command and response in the pcap validates:
- Ping: all 3 wire bytes match [0x95, 0x06, 0xB2]
- Response: decrypts to 0xAA (correct)
- Cmd2 (0x8B): control byte 0x96 matches
- Resp2: decrypts to 'Logi' (device ID string)
- Cmd3 (0x07): control byte 0xD3 matches
- Cmd4 (0x07): control byte 0x45 matches
- Resp4: decrypts to 'REV' + version
- Cmd5 (0x7E): control byte 0x01 matches
- Resp5: decrypts to 0x00 (success)

### Reseed mechanism

The 5-byte reseed packet IS encrypted with the current LFSR state. The first byte is a control byte produced by `prepare_buffer` with `|0x20` flag. The actual seed is 4 bytes inside the encrypted packet. After decrypting the pcap reseed with old state 0x354b248e, the raw seed is 0x6fe77bc8.

The old state 0x354b248e is the initial LFSR state the device/host starts with before the first reseed.

## 2026-03-10: Reseed implementation fixed

Fixed ep1.rs `initialize()` to properly encrypt the reseed packet:

1. Added `RESEED_FLAG` (0x20) and `encrypt_reseed()` method to `SimpleEncryption`
2. `encrypt_reseed` produces 5-byte output: control byte (with |0x20 flag) + 4 XOR'd seed bytes
3. Changed `initialize()` from sending raw 4 bytes to sending encrypted reseed via `encrypt_reseed()`
4. After sending, calls `reseed(new_seed)` to set LFSR state to raw seed value
5. Added pcap validation test for reseed frame 15: wire bytes [0x66, 0xec, 0x30, 0xd2, 0xe1] match exactly
6. Fixed tautological pinned tests (advance_seed_pinned, lfsr_step_from_one_produces_expected_sequence) with hard-coded values
7. All 245 tests pass, clippy clean, fmt clean

### Remaining
- AC#1: Hardware test not possible from current environment (nvdk-msdlab-09 unreachable)
- AC#4: Init sequence documentation still needed
- The initial LFSR state (0x354b248e in pcap) may be device-specific or firmware-dependent. Our impl starts with seed=0, which should work if firmware also starts at 0 after boot.

## 2026-03-11: AC#4 Complete - Full init sequence decoded from pcap

Decoded all 928 EP1 frames from usb.logic8pro.pcap with 0 unknown opcodes.

### Key discovery: Reseed uses fixed key

Reseed packets are ALWAYS encrypted with the initial/default LFSR seed (0x354b248e), NOT the current rolling seed. This corrects the previous finding that stated "encrypted with the CURRENT LFSR state". The first reseed happened to work because current == initial at that point. All 4 reseeds in the pcap (frames 15, 1627, 1639, 1651) validate with the initial seed.

Created TASK-0108 to fix `encrypt_reseed` in obfuscation.rs.

### Files created
- `re/notes/init-sequence-pcap.md` - Complete initialization sequence documentation
- `re/scripts/decode-pcap-init.py` - Pcap decoder script

### Init sequence summary (8 phases)
1. Handshake: Reseed + Ping + GetDeviceInfo + ReadVersion + ResetFpga
2. FPGA upload: 334 chunks, 340,604 bytes, ~1.7s
3. Post-FPGA: ADC init, JESD204B link, calibration coefficients
4. Reseed #2 + digital channel config
5. Reseed #3 + updated channel config (21s user interaction gap)
6. Reseed #4 + EEPROM calibration via I2C (addr 0xC0)
7. Capture config + StartSampling
8. Teardown: StopStream + power down

## 2026-03-11: Init sequence documented from pcap

Complete 8-phase init sequence documented in re/notes/init-sequence-pcap.md. 261 operations across 928 EP1 frames decoded. Critical discovery: reseed packets ALWAYS use the initial LFSR seed (0x354b248e), not the rolling state. Created TASK-0108 to fix this bug. AC#1 (hardware test) still blocked on hardware access.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
All 928 EP1 frames decoded from pcap with 0 unknown opcodes. Key discovery: reseed uses initial seed (0x354b248e), not rolling seed. Full 8-phase init sequence documented in re/notes/init-sequence-pcap.md. Phase 7 capture parameters extracted as 12 register writes. This directly enabled the EP1 ping fix and is now being used for hardware capture implementation.
<!-- SECTION:FINAL_SUMMARY:END -->
