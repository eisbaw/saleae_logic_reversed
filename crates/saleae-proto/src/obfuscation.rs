//! XOR obfuscation layer for the EP1 command protocol.
//!
//! Saleae Logic Pro 8/16 devices XOR-encrypt all EP1 command traffic using a
//! shared 32-bit seed established by `ReseedDevice`. Non-Pro FX3 devices
//! (Logic 4, Logic 8) only apply the XOR during firmware download. Legacy FX2
//! devices use a separate substitution cipher (not implemented here).
//!
//! The scheme is called "SimpleEncryption" in the original source
//! (`SimpleEncryption.cpp`). It uses a 32-bit LFSR with taps at bits 0, 1,
//! 21, and 31 to advance the seed after each write and read operation. Both
//! host and firmware must stay in lockstep.
//!
//! # Wire format
//!
//! ## Command write (encrypted)
//!
//! The encrypted output is 1 byte longer than the input (a control byte is
//! prepended):
//!
//! ```text
//! output[0] = control byte (see below)
//! output[1] = opcode ^ seed_byte(1)
//! output[2] = payload[0] ^ seed_byte(2)
//! output[3] = payload[1] ^ seed_byte(3)
//! output[4] = payload[2] ^ seed_byte(0)   // wraps
//! output[5] = payload[3] ^ seed_byte(1)
//! ...
//! ```
//!
//! The control byte carries the opcode with bits 3 and 5 masked out (AND
//! 0xD7), bit 3 set if in firmware download mode, then XOR'd with the same
//! mask applied to seed byte 0:
//!
//! ```text
//! control = (opcode & 0xD7) [| 0x08 if firmware_download] ^ (seed_byte(0) & 0xD7)
//! ```
//!
//! Note the data XOR starts at seed_byte(1), not seed_byte(0). The control
//! byte uses seed_byte(0) with the D7 mask independently.
//!
//! ## Response read (decrypted)
//!
//! Simple cyclic XOR with the full seed bytes (no mask, no control byte):
//!
//! ```text
//! for i in 0..len:
//!     data[i] ^= seed_byte(i % 4)
//! ```
//!
//! ## Seed advancement
//!
//! After each write and each read, both sides advance the LFSR. One
//! write-read pair consumes two advancements. The number of LFSR steps per
//! advancement is `(seed & 0x1F) + 0x23` (range 35..=66).
//!
//! # References
//!
//! - `re/notes/xor-obfuscation.md` (full analysis)
//! - `re/notes/reseed-mechanism.md` (ReseedDevice)
//! - `doc/protocol/ep1-commands.md` (command table)

/// The firmware's hardcoded default seed, used to encrypt/decrypt ReseedDevice
/// packets. Both host and firmware use this fixed value regardless of the
/// current rolling LFSR state. Confirmed by pcap analysis of all 4 reseed
/// operations (frames 15, 1627, 1639, 1651).
///
/// See `re/notes/xor-obfuscation.md` (reseed key discovery) and
/// `re/notes/init-sequence-pcap.md` (Phase 1 reseed validation).
pub const DEFAULT_INITIAL_SEED: u32 = 0x354b248e;

/// Mask applied to the control byte and its corresponding seed byte.
///
/// Clears bits 3 and 5 so they can carry the firmware download flag (bit 3)
/// and remain reserved (bit 5).
const CONTROL_MASK: u8 = 0xD7;

/// Firmware download flag in the control byte (bit 3).
const FIRMWARE_FLAG: u8 = 0x08;

/// Reseed flag in the control byte (bit 5).
///
/// Used by `prepare_buffer` (VA 0x2d99c10) when encrypting ReseedDevice
/// commands. The `| 0x20` flag distinguishes reseed packets from normal
/// commands and firmware download packets.
const RESEED_FLAG: u8 = 0x20;

/// Encryption mode, determined by device type.
///
/// This corresponds to the mode selection at VA 0x2d97ec0 in
/// `libgraph_server_shared.so`:
///
/// - Pro devices (DeviceType 6, 7): XOR all EP1 commands
/// - Non-Pro FX3 (DeviceType 4, 5): XOR only during firmware download
/// - Legacy FX2 (DeviceType 0, 1): substitution cipher (not implemented)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionMode {
    /// No encryption (passthrough). Used when the device does not require
    /// obfuscation for the current operation.
    None,
    /// XOR all EP1 commands and responses with the rolling seed.
    /// Used by Logic Pro 8 and Logic Pro 16.
    XorAll,
    /// XOR only during firmware download. Normal commands are unencrypted.
    /// Used by Logic 4 and Logic 8.
    XorFirmwareOnly,
}

// ---------------------------------------------------------------------------
// LFSR core
// ---------------------------------------------------------------------------

/// Perform one step of the 32-bit LFSR.
///
/// Taps at bits 0, 1, 21, and 31. Feedback is XOR of these four bits,
/// shifted into the MSB while the rest shifts right by 1.
///
/// Reconstructed from the odd-fixup path at VA 0x2d99290+0x5b in
/// `libgraph_server_shared.so`. The main loop body at +0x20 computes two
/// of these steps per iteration (optimized unrolling).
///
/// This function is `#[inline]` because `advance_seed` calls it in a tight
/// loop (35-66 iterations).
#[inline]
fn lfsr_step(state: &mut u32) {
    let s = *state;
    let feedback = (s ^ (s >> 1) ^ (s >> 21) ^ (s >> 31)) & 1;
    *state = (s >> 1) | (feedback << 31);
}

/// Advance the LFSR seed by a data-dependent number of steps.
///
/// The iteration count is `(seed & 0x1F) + 0x23`, giving a range of 35 to 66
/// single LFSR steps. The original binary at VA 0x2d99290 processes two steps
/// per loop iteration (optimized unrolling), but the total count is N single
/// steps.
fn advance_seed(seed: &mut u32) {
    let n = (*seed & 0x1F) + 0x23;
    for _ in 0..n {
        lfsr_step(seed);
    }
}

// ---------------------------------------------------------------------------
// Seed byte extraction
// ---------------------------------------------------------------------------

/// Extract byte `i % 4` from the 32-bit seed (little-endian order).
#[inline]
fn seed_byte(seed: u32, i: usize) -> u8 {
    ((seed >> ((i % 4) * 8)) & 0xFF) as u8
}

// ---------------------------------------------------------------------------
// SimpleEncryption struct
// ---------------------------------------------------------------------------

/// XOR obfuscation state for EP1 command traffic.
///
/// Holds the current LFSR seed and encryption mode. The caller is responsible
/// for calling [`encrypt_command`] before each EP1 OUT write and
/// [`decrypt_response`] after each EP1 IN read. Each of these methods
/// advances the seed internally.
///
/// # Lockstep requirement
///
/// Host and firmware must maintain identical LFSR state. The advancement
/// order is:
///
/// 1. Host encrypts command with current seed, then advances seed.
/// 2. Firmware decrypts with its seed, then advances.
/// 3. Firmware encrypts response with current seed, then advances.
/// 4. Host decrypts response with its seed, then advances.
///
/// If either side misses a step, all subsequent communication is garbled.
///
/// # Reseed key
///
/// ReseedDevice packets are always encrypted with the *initial* seed (the
/// value passed to [`new`](Self::new)), not the rolling LFSR state. Pcap
/// analysis of all 4 reseed operations (frames 15, 1627, 1639, 1651)
/// confirms this. The `initial_seed` field is set once in `new()` and
/// never modified by [`reseed`](Self::reseed).
///
/// # Example
///
/// ```
/// use saleae_proto::obfuscation::{SimpleEncryption, EncryptionMode};
///
/// let mut enc = SimpleEncryption::new(0xDEADBEEF, EncryptionMode::XorAll);
///
/// // Encrypt a ping command: opcode 0x7D, payload [0x55]
/// let encrypted = enc.encrypt_command(0x7D, &[0x55], false);
/// assert_eq!(encrypted.len(), 3); // 1 control byte + 2 original bytes
///
/// // Decrypt a response
/// let mut response = vec![0xAA];
/// enc.decrypt_response(&mut response, false);
/// ```
#[derive(Debug, Clone)]
pub struct SimpleEncryption {
    seed: u32,
    /// The seed value passed to `new()`. Used exclusively by `encrypt_reseed`
    /// to encrypt ReseedDevice packets. This value never changes -- it is the
    /// fixed key that both host and firmware use for reseed encryption,
    /// regardless of how many reseeds have occurred.
    initial_seed: u32,
    mode: EncryptionMode,
}

impl SimpleEncryption {
    /// Create a new encryption state with the given seed and mode.
    ///
    /// The seed is the 4-byte value sent by `ReseedDevice` and shared
    /// between host and firmware. This value is also stored as the
    /// `initial_seed` used for encrypting all future ReseedDevice packets.
    pub fn new(seed: u32, mode: EncryptionMode) -> Self {
        Self {
            seed,
            initial_seed: seed,
            mode,
        }
    }

    /// Return the current seed value (for debugging/testing).
    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// Return the encryption mode.
    pub fn mode(&self) -> EncryptionMode {
        self.mode
    }

    /// Encrypt a command for EP1 OUT.
    ///
    /// Takes the raw opcode and payload, returns an encrypted buffer that is
    /// 1 byte longer (control byte prepended). Advances the seed afterward.
    ///
    /// If the encryption mode is `None`, or if mode is `XorFirmwareOnly` and
    /// `is_firmware_download` is false, the command is returned unmodified
    /// (no control byte, no XOR, no seed advancement).
    ///
    /// # Arguments
    ///
    /// - `opcode`: The command opcode byte.
    /// - `payload`: The payload bytes (may be empty).
    /// - `is_firmware_download`: Set to true during firmware download to set
    ///   bit 3 in the control byte.
    pub fn encrypt_command(
        &mut self,
        opcode: u8,
        payload: &[u8],
        is_firmware_download: bool,
    ) -> Vec<u8> {
        if !self.should_encrypt(is_firmware_download) {
            // No encryption: return the raw command as-is.
            let mut out = Vec::with_capacity(1 + payload.len());
            out.push(opcode);
            out.extend_from_slice(payload);
            return out;
        }

        let seed = self.seed;
        let input_len = 1 + payload.len(); // opcode + payload
        let output_len = 1 + input_len; // control byte + opcode + payload

        let mut out = Vec::with_capacity(output_len);

        // Byte 0: control byte
        let mut control = opcode & CONTROL_MASK;
        if is_firmware_download {
            control |= FIRMWARE_FLAG;
        }
        control ^= seed_byte(seed, 0) & CONTROL_MASK;
        out.push(control);

        // Byte 1: opcode XOR'd with seed byte 1
        // (the data XOR starts at sb1, not sb0 -- sb0 is used only for the
        // control byte with the D7 mask)
        out.push(opcode ^ seed_byte(seed, 1));

        // Bytes 2+: payload XOR'd with cycling seed bytes (sb2, sb3, sb0, sb1, ...)
        for (i, &b) in payload.iter().enumerate() {
            out.push(b ^ seed_byte(seed, i + 2));
        }

        // Advance seed after write
        advance_seed(&mut self.seed);

        out
    }

    /// Decrypt a response from EP1 IN, in place.
    ///
    /// Simple cyclic XOR with the full seed bytes (no mask, no control byte).
    /// Advances the seed afterward.
    ///
    /// If the encryption mode is `None`, this is a no-op (no XOR, no seed
    /// advancement).
    ///
    /// # Arguments
    ///
    /// - `data`: The response buffer to decrypt in place.
    /// - `is_firmware_download`: Must match the mode used for the preceding
    ///   write. Controls whether decryption is applied in `XorFirmwareOnly`
    ///   mode.
    pub fn decrypt_response(&mut self, data: &mut [u8], is_firmware_download: bool) {
        if !self.should_encrypt(is_firmware_download) {
            return;
        }

        let seed = self.seed;
        for (i, byte) in data.iter_mut().enumerate() {
            *byte ^= seed_byte(seed, i);
        }

        // Advance seed after read
        advance_seed(&mut self.seed);
    }

    /// Encrypt a ReseedDevice command for EP1 OUT.
    ///
    /// The reseed sends 4 seed bytes to the device, encrypted with the
    /// **initial seed** (from [`new`](Self::new)), not the rolling LFSR
    /// state. Pcap analysis of all 4 reseed operations (frames 15, 1627,
    /// 1639, 1651) confirms that the initial seed is always used as the
    /// encryption key for reseed packets.
    ///
    /// The wire format is 5 bytes:
    ///
    /// ```text
    /// output[0] = ((seed_bytes[0] & 0xD7) | 0x20) ^ (initial_sb0 & 0xD7)
    /// output[1] = seed_bytes[0] ^ initial_sb1
    /// output[2] = seed_bytes[1] ^ initial_sb2
    /// output[3] = seed_bytes[2] ^ initial_sb3
    /// output[4] = seed_bytes[3] ^ initial_sb0
    /// ```
    ///
    /// After sending, the caller MUST call [`reseed`](Self::reseed) to set
    /// the new LFSR state to the raw seed value.
    ///
    /// For `EncryptionMode::None`, returns the raw 4-byte seed without
    /// encryption or control byte. For `XorFirmwareOnly`, the reseed is
    /// still encrypted -- `prepare_buffer` always applies the XOR
    /// regardless of the normal command encryption mode, because the
    /// firmware needs to know the seed to establish shared state.
    pub fn encrypt_reseed(&mut self, seed_bytes: &[u8; 4]) -> Vec<u8> {
        if self.mode == EncryptionMode::None {
            return seed_bytes.to_vec();
        }

        // Use the initial seed (from new()), NOT the rolling self.seed.
        // The firmware always uses the initial/default seed to decrypt
        // reseed packets, regardless of the current LFSR state.
        let key = self.initial_seed;
        let mut out = Vec::with_capacity(5);

        // Control byte: seed_bytes[0] is the "opcode", with RESEED_FLAG set
        let control =
            ((seed_bytes[0] & CONTROL_MASK) | RESEED_FLAG) ^ (seed_byte(key, 0) & CONTROL_MASK);
        out.push(control);

        // Data bytes: XOR with cycling seed bytes starting at sb1
        // (same pattern as encrypt_command)
        for (i, &b) in seed_bytes.iter().enumerate() {
            out.push(b ^ seed_byte(key, i + 1));
        }

        // Advance the rolling seed (matches prepare_buffer behavior; the
        // result will be discarded when the caller calls reseed() with
        // the new seed value)
        advance_seed(&mut self.seed);

        out
    }

    /// Reset the seed (as done by ReseedDevice).
    ///
    /// This should be called after [`encrypt_reseed`](Self::encrypt_reseed)
    /// to set the LFSR state to the new seed. The old seed (and any
    /// advance_seed result) is discarded.
    pub fn reseed(&mut self, new_seed: u32) {
        self.seed = new_seed;
    }

    /// Manually advance the seed (exposed for testing).
    ///
    /// Normally you should not call this directly -- `encrypt_command` and
    /// `decrypt_response` advance the seed automatically.
    pub fn advance_seed(&mut self) {
        advance_seed(&mut self.seed);
    }

    /// Whether encryption should be applied for the current operation.
    fn should_encrypt(&self, is_firmware_download: bool) -> bool {
        match self.mode {
            EncryptionMode::None => false,
            EncryptionMode::XorAll => true,
            EncryptionMode::XorFirmwareOnly => is_firmware_download,
        }
    }
}

// ---------------------------------------------------------------------------
// Ping helper
// ---------------------------------------------------------------------------

/// Ping opcode (0x7D).
pub const PING_OPCODE: u8 = 0x7D;

/// Default ping test byte.
pub const PING_TEST_BYTE: u8 = 0x55;

/// Expected ping response for the default test byte.
pub const PING_EXPECTED_RESPONSE: u8 = 0xAA;

/// Validate a ping response byte against the expected complement.
///
/// The firmware returns the bitwise complement of the test byte:
/// `response == !test_byte`.
pub fn validate_ping_response(test_byte: u8, response: u8) -> bool {
    response == !test_byte
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- LFSR step tests --

    #[test]
    fn lfsr_step_zero_stays_zero() {
        let mut state = 0u32;
        lfsr_step(&mut state);
        assert_eq!(state, 0, "LFSR step on zero should produce zero");
    }

    #[test]
    fn advance_seed_zero_stays_zero() {
        let mut seed = 0u32;
        advance_seed(&mut seed);
        assert_eq!(seed, 0, "advancing zero seed should stay zero");
    }

    #[test]
    fn lfsr_step_known_values() {
        // Start from a known seed and verify the LFSR produces deterministic output.
        let mut state = 0x0000_0001;
        let mut history = Vec::new();
        for _ in 0..8 {
            lfsr_step(&mut state);
            history.push(state);
        }
        // Verify determinism: running again from same start produces same sequence.
        let mut state2 = 0x0000_0001;
        for &expected in &history {
            lfsr_step(&mut state2);
            assert_eq!(state2, expected);
        }
    }

    #[test]
    fn lfsr_step_does_not_trivially_cycle_in_8_steps() {
        // The LFSR period should be much longer than 8 for a non-zero seed.
        let mut state = 0xDEAD_BEEF;
        let original = state;
        for _ in 0..8 {
            lfsr_step(&mut state);
            assert_ne!(
                state, original,
                "LFSR should not cycle back to start this quickly"
            );
        }
    }

    #[test]
    fn advance_seed_iteration_count() {
        // For seed = 0x0000_0023 (low 5 bits = 0x03):
        //   n = 0x03 + 0x23 = 0x26 = 38 iterations
        // For seed = 0x0000_001F (low 5 bits = 0x1F):
        //   n = 0x1F + 0x23 = 0x42 = 66 iterations
        // For seed = 0x0000_0000 (low 5 bits = 0x00):
        //   n = 0x00 + 0x23 = 0x23 = 35 iterations

        // We can verify the iteration count indirectly by checking that
        // advance_seed produces a specific output for a known input.
        let mut seed = 1u32;
        advance_seed(&mut seed);
        // seed=1, low 5 bits = 1, n = 1 + 0x23 = 36 steps
        // We cannot easily predict the exact output without running the LFSR
        // 36 times, but we can verify it's deterministic and non-zero.
        assert_ne!(seed, 0);
        assert_ne!(seed, 1);

        // Verify determinism
        let mut seed2 = 1u32;
        advance_seed(&mut seed2);
        assert_eq!(seed, seed2);
    }

    #[test]
    fn advance_seed_deterministic_sequence() {
        // Record a sequence of seed values after repeated advancement.
        let initial = 0xCAFE_BABE_u32;
        let mut seed = initial;
        let mut sequence = Vec::new();
        for _ in 0..5 {
            advance_seed(&mut seed);
            sequence.push(seed);
        }

        // Replay and verify
        let mut seed2 = initial;
        for &expected in &sequence {
            advance_seed(&mut seed2);
            assert_eq!(seed2, expected);
        }

        // All values should be distinct (extremely likely for a good LFSR)
        let mut unique = sequence.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            sequence.len(),
            "5 consecutive seed advancements should produce distinct values"
        );
    }

    // -- Seed byte extraction --

    #[test]
    fn seed_byte_extraction() {
        let seed: u32 = 0xDEADBEEF;
        assert_eq!(seed_byte(seed, 0), 0xEF);
        assert_eq!(seed_byte(seed, 1), 0xBE);
        assert_eq!(seed_byte(seed, 2), 0xAD);
        assert_eq!(seed_byte(seed, 3), 0xDE);
        // Wraps around
        assert_eq!(seed_byte(seed, 4), 0xEF);
        assert_eq!(seed_byte(seed, 5), 0xBE);
    }

    // -- Encrypt/decrypt --

    #[test]
    fn encrypt_adds_control_byte() {
        let mut enc = SimpleEncryption::new(0, EncryptionMode::XorAll);
        let result = enc.encrypt_command(0x7D, &[0x55], false);
        // With seed=0, XOR with all-zero bytes is identity:
        // control = (0x7D & 0xD7) ^ (0 & 0xD7) = 0x55
        // opcode  = 0x7D ^ sb1 = 0x7D ^ 0 = 0x7D
        // payload = 0x55 ^ sb2 = 0x55 ^ 0 = 0x55
        assert_eq!(result.len(), 3, "output should be 1 byte longer than input");
        assert_eq!(result[0], 0x7D & CONTROL_MASK); // 0x55
        assert_eq!(result[1], 0x7D);
        assert_eq!(result[2], 0x55);
    }

    #[test]
    fn zero_seed_xor_is_identity() {
        let mut enc = SimpleEncryption::new(0, EncryptionMode::XorAll);
        let opcode = 0x80;
        let payload = [0x01, 0x42, 0xFF];
        let result = enc.encrypt_command(opcode, &payload, false);

        // Control byte: (0x80 & 0xD7) ^ (0x00 & 0xD7) = 0x80
        assert_eq!(result[0], 0x80 & CONTROL_MASK); // bits 3,5 cleared = 0x80
        assert_eq!(result[1], opcode);
        assert_eq!(result[2], payload[0]);
        assert_eq!(result[3], payload[1]);
        assert_eq!(result[4], payload[2]);

        // Seed should still be 0 after advancement (zero stays zero)
        assert_eq!(enc.seed(), 0);
    }

    #[test]
    fn zero_seed_decrypt_is_identity() {
        let mut enc = SimpleEncryption::new(0, EncryptionMode::XorAll);
        let mut data = vec![0xAA, 0xBB, 0xCC];
        let original = data.clone();
        enc.decrypt_response(&mut data, false);
        assert_eq!(data, original);
        assert_eq!(enc.seed(), 0);
    }

    #[test]
    fn firmware_download_flag_sets_bit_3() {
        let mut enc = SimpleEncryption::new(0, EncryptionMode::XorAll);
        let result = enc.encrypt_command(0x80, &[], true);
        // Control byte: (0x80 & 0xD7) | 0x08 ^ (0x00 & 0xD7)
        // = 0x80 | 0x08 = 0x88
        assert_eq!(result[0], (0x80 & CONTROL_MASK) | FIRMWARE_FLAG);
    }

    #[test]
    fn nonzero_seed_encrypt_decrypt_roundtrip() {
        // Use two encryption states starting from the same seed to simulate
        // host and firmware. The host encrypts, the firmware decrypts.
        let seed = 0xDEAD_BEEF;
        let mut host = SimpleEncryption::new(seed, EncryptionMode::XorAll);
        let mut firmware = SimpleEncryption::new(seed, EncryptionMode::XorAll);

        let opcode = 0x7D;
        let payload = [0x55];

        // Host encrypts and sends
        let encrypted = host.encrypt_command(opcode, &payload, false);
        assert_eq!(encrypted.len(), 3);

        // Firmware receives: skip control byte, decrypt opcode+payload.
        // Data bytes (output[1..]) were XOR'd with cycling seed bytes
        // starting at sb1. The firmware reverses this.
        let mut cmd_bytes = encrypted[1..].to_vec();
        let fw_seed = firmware.seed();
        for (i, byte) in cmd_bytes.iter_mut().enumerate() {
            *byte ^= seed_byte(fw_seed, i + 1); // starts at sb1
        }
        firmware.advance_seed();

        assert_eq!(cmd_bytes[0], opcode);
        assert_eq!(cmd_bytes[1], payload[0]);

        // Firmware sends response (plain XOR with sb0..sb3 cycling)
        let mut response = vec![0xAA]; // ping response
        let fw_resp_seed = firmware.seed();
        for (i, byte) in response.iter_mut().enumerate() {
            *byte ^= seed_byte(fw_resp_seed, i);
        }
        firmware.advance_seed();

        // Host decrypts response
        host.decrypt_response(&mut response, false);
        assert_eq!(response[0], 0xAA);
    }

    #[test]
    fn seed_advances_after_encrypt() {
        let seed = 0x1234_5678;
        let mut enc = SimpleEncryption::new(seed, EncryptionMode::XorAll);
        enc.encrypt_command(0x01, &[], false);
        assert_ne!(enc.seed(), seed, "seed should advance after encrypt");
    }

    #[test]
    fn seed_advances_after_decrypt() {
        let seed = 0x1234_5678;
        let mut enc = SimpleEncryption::new(seed, EncryptionMode::XorAll);
        let mut data = vec![0x00];
        enc.decrypt_response(&mut data, false);
        assert_ne!(enc.seed(), seed, "seed should advance after decrypt");
    }

    #[test]
    fn encryption_mode_none_passes_through() {
        let mut enc = SimpleEncryption::new(0xDEAD_BEEF, EncryptionMode::None);
        let result = enc.encrypt_command(0x7D, &[0x55], false);
        // No control byte, no XOR
        assert_eq!(result, vec![0x7D, 0x55]);
        // Seed should NOT advance
        assert_eq!(enc.seed(), 0xDEAD_BEEF);

        let mut data = vec![0xAA];
        enc.decrypt_response(&mut data, false);
        assert_eq!(data, vec![0xAA]);
        assert_eq!(enc.seed(), 0xDEAD_BEEF);
    }

    #[test]
    fn xor_firmware_only_mode_no_encrypt_for_normal_commands() {
        let seed = 0xDEAD_BEEF;
        let mut enc = SimpleEncryption::new(seed, EncryptionMode::XorFirmwareOnly);

        // Normal command: no encryption
        let result = enc.encrypt_command(0x01, &[], false);
        assert_eq!(result, vec![0x01]);
        assert_eq!(enc.seed(), seed);

        // Firmware download command: encrypted
        let result = enc.encrypt_command(0x7F, &[0x01, 0x02], true);
        assert_eq!(result.len(), 4); // control byte + opcode + 2 payload
        assert_ne!(enc.seed(), seed); // seed advanced
    }

    #[test]
    fn xor_firmware_only_mode_no_decrypt_for_normal_responses() {
        let seed = 0xDEAD_BEEF;
        let mut enc = SimpleEncryption::new(seed, EncryptionMode::XorFirmwareOnly);

        let mut data = vec![0xAA];
        enc.decrypt_response(&mut data, false);
        assert_eq!(data, vec![0xAA]);
        assert_eq!(enc.seed(), seed);

        // Firmware download response: decrypted
        let mut data2 = vec![0x00];
        enc.decrypt_response(&mut data2, true);
        assert_ne!(enc.seed(), seed);
    }

    #[test]
    fn reseed_replaces_state() {
        let mut enc = SimpleEncryption::new(0x1111_1111, EncryptionMode::XorAll);
        enc.encrypt_command(0x01, &[], false); // advance seed
        let after_advance = enc.seed();
        assert_ne!(after_advance, 0x1111_1111);

        enc.reseed(0x2222_2222);
        assert_eq!(enc.seed(), 0x2222_2222);
    }

    // -- Control byte tests --

    #[test]
    fn control_byte_masks_bits_3_and_5() {
        // Opcode 0xFF: all bits set.
        // 0xFF & 0xD7 = 0xD7 (bits 3 and 5 cleared)
        let mut enc = SimpleEncryption::new(0, EncryptionMode::XorAll);
        let result = enc.encrypt_command(0xFF, &[], false);
        assert_eq!(result[0], CONTROL_MASK); // 0xFF & 0xD7 = 0xD7
    }

    #[test]
    fn control_byte_with_nonzero_seed() {
        let seed: u32 = 0x0000_00AB; // byte 0 = 0xAB
        let mut enc = SimpleEncryption::new(seed, EncryptionMode::XorAll);
        let opcode = 0x80;
        let result = enc.encrypt_command(opcode, &[], false);
        // control = (0x80 & 0xD7) ^ (0xAB & 0xD7)
        //         = 0x80 ^ 0x83
        //         = 0x03
        let expected_control = (opcode & CONTROL_MASK) ^ (0xAB & CONTROL_MASK);
        assert_eq!(result[0], expected_control);
        // output[1] = opcode ^ sb1 = 0x80 ^ 0x00 = 0x80
        assert_eq!(result[1], 0x80);
    }

    // -- Longer payload cycling --

    #[test]
    fn payload_xor_cycles_through_seed_bytes() {
        let seed: u32 = 0x04030201; // bytes: [0x01, 0x02, 0x03, 0x04]
        let mut enc = SimpleEncryption::new(seed, EncryptionMode::XorAll);
        let payload = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let result = enc.encrypt_command(0x00, &payload, false);

        // result[0] = control byte = (0x00 & 0xD7) ^ (0x01 & 0xD7) = 0x01
        assert_eq!(result[0], 0x01);
        // Data XOR starts at sb1 (not sb0):
        // result[1] = 0x00 ^ sb1 = 0x02
        assert_eq!(result[1], 0x02);
        // result[2] = 0x00 ^ sb2 = 0x03
        assert_eq!(result[2], 0x03);
        // result[3] = 0x00 ^ sb3 = 0x04
        assert_eq!(result[3], 0x04);
        // result[4] = 0x00 ^ sb0 = 0x01 (wraps)
        assert_eq!(result[4], 0x01);
        // result[5] = 0x00 ^ sb1 = 0x02
        assert_eq!(result[5], 0x02);
        // result[6] = 0x00 ^ sb2 = 0x03
        assert_eq!(result[6], 0x03);
        // result[7] = 0x00 ^ sb3 = 0x04
        assert_eq!(result[7], 0x04);
        // result[8] = 0x00 ^ sb0 = 0x01
        assert_eq!(result[8], 0x01);
    }

    #[test]
    fn decrypt_response_cycles_through_seed_bytes() {
        let seed: u32 = 0x04030201;
        let mut enc = SimpleEncryption::new(seed, EncryptionMode::XorAll);
        let mut data = vec![0x01, 0x02, 0x03, 0x04, 0x01, 0x02];
        enc.decrypt_response(&mut data, false);
        // Each byte XOR'd with its corresponding seed byte should give 0x00
        assert_eq!(data, vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    }

    // -- Ping tests --

    #[test]
    fn ping_complement() {
        assert!(validate_ping_response(0x55, 0xAA));
        assert!(validate_ping_response(0x00, 0xFF));
        assert!(validate_ping_response(0xFF, 0x00));
        assert!(!validate_ping_response(0x55, 0x55));
        assert!(!validate_ping_response(0x55, 0x00));
    }

    #[test]
    fn ping_constants_are_consistent() {
        assert!(validate_ping_response(
            PING_TEST_BYTE,
            PING_EXPECTED_RESPONSE
        ));
    }

    // -- Reseed encryption tests --

    #[test]
    fn encrypt_reseed_wire_format_with_zero_seed() {
        let mut enc = SimpleEncryption::new(0, EncryptionMode::XorAll);
        let seed_bytes = [0xAA, 0xBB, 0xCC, 0xDD];
        let wire = enc.encrypt_reseed(&seed_bytes);
        // control = ((0xAA & 0xD7) | 0x20) ^ (0x00 & 0xD7) = (0x82 | 0x20) = 0xA2
        // data[0] = 0xAA ^ sb1(0) = 0xAA
        // data[1] = 0xBB ^ sb2(0) = 0xBB
        // data[2] = 0xCC ^ sb3(0) = 0xCC
        // data[3] = 0xDD ^ sb0(0) = 0xDD
        assert_eq!(wire, vec![0xA2, 0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn encrypt_reseed_none_mode_returns_raw() {
        let mut enc = SimpleEncryption::new(0xDEADBEEF, EncryptionMode::None);
        let seed_bytes = [0x11, 0x22, 0x33, 0x44];
        let wire = enc.encrypt_reseed(&seed_bytes);
        assert_eq!(wire, vec![0x11, 0x22, 0x33, 0x44]);
        // Seed should NOT advance in None mode
        assert_eq!(enc.seed(), 0xDEADBEEF);
    }

    #[test]
    fn encrypt_reseed_pcap_frame15() {
        // Validate against pcap frame 15: reseed from state 0x354b248e
        // Seed bytes (LE of 0x6fe77bc8): [0xc8, 0x7b, 0xe7, 0x6f]
        // Wire bytes from pcap: [0x66, 0xec, 0x30, 0xd2, 0xe1]
        let old_state = 0x354b248e_u32;
        let mut enc = SimpleEncryption::new(old_state, EncryptionMode::XorAll);
        let seed_bytes: [u8; 4] = [0xc8, 0x7b, 0xe7, 0x6f];
        let wire = enc.encrypt_reseed(&seed_bytes);
        assert_eq!(
            wire,
            vec![0x66, 0xec, 0x30, 0xd2, 0xe1],
            "reseed wire bytes must match pcap frame 15"
        );

        // After encrypt_reseed, seed was advanced (but we're about to discard it)
        // Simulate the caller's reseed() call
        enc.reseed(0x6fe77bc8);
        assert_eq!(enc.seed(), 0x6fe77bc8);
    }

    // -- Pcap validation (ground truth) --

    /// Validate the full encryption against captured pcap data from a real
    /// Saleae Logic Pro 8 session (usb.logic8pro.pcap).
    ///
    /// This is the definitive test: every wire byte must match the pcap
    /// exactly. The sequence starts with the reseed (frame 15) and continues
    /// through multiple command/response pairs.
    #[test]
    fn pcap_validation_full_sequence() {
        // Pre-reseed LFSR state (derived from pcap frame 15)
        let pre_reseed_state = 0x354b248e_u32;
        let mut enc = SimpleEncryption::new(pre_reseed_state, EncryptionMode::XorAll);

        // Frame 15: EP1 OUT 5 bytes - ReseedDevice
        let seed_bytes: [u8; 4] = [0xc8, 0x7b, 0xe7, 0x6f]; // 0x6fe77bc8 LE
        let reseed_wire = enc.encrypt_reseed(&seed_bytes);
        assert_eq!(
            reseed_wire,
            vec![0x66, 0xec, 0x30, 0xd2, 0xe1],
            "reseed wire mismatch (frame 15)"
        );
        enc.reseed(0x6fe77bc8);

        // Frame 17: EP1 OUT 3 bytes - encrypted ping [0x7D, 0x55]
        let ping_wire = enc.encrypt_command(PING_OPCODE, &[PING_TEST_BYTE], false);
        assert_eq!(ping_wire, vec![0x95, 0x06, 0xB2], "ping wire mismatch");

        // Frame 20: EP1 IN 1 byte - encrypted response [0xCD]
        let mut resp1 = vec![0xCDu8];
        enc.decrypt_response(&mut resp1, false);
        assert_eq!(
            resp1[0], PING_EXPECTED_RESPONSE,
            "ping response should be 0xAA"
        );

        // Frame 21: EP1 OUT 2 bytes - encrypted cmd2 [0x96, 0x42]
        // Opcode 0x8B (derived from 0x42 ^ sb1 where sb1=0xC9)
        let cmd2_wire = enc.encrypt_command(0x8B, &[], false);
        assert_eq!(cmd2_wire, vec![0x96, 0x42], "cmd2 wire mismatch");

        // Frame 24: EP1 IN 128 bytes - first 4 bytes decrypt to "Logi"
        let mut resp2_first4 = vec![0x6Au8, 0xB2, 0xE5, 0x0C];
        let resp2_seed = enc.seed(); // save for partial decrypt check
                                     // Just check first 4 bytes (we don't have the full 128-byte response)
        for (i, byte) in resp2_first4.iter_mut().enumerate() {
            *byte ^= seed_byte(resp2_seed, i);
        }
        assert_eq!(&resp2_first4, b"Logi", "resp2 should start with 'Logi'");
        // Still need to advance seed for the full response
        enc.advance_seed();

        // Frame 25: EP1 OUT 8 bytes - encrypted cmd3
        // source[0] = 0x07, remaining = [0x33, 0x81, 0x08, 0x00, 0x08, 0x00]
        let cmd3_wire = enc.encrypt_command(0x07, &[0x33, 0x81, 0x08, 0x00, 0x08, 0x00], false);
        assert_eq!(
            cmd3_wire,
            vec![0xD3, 0x38, 0xBF, 0x43, 0xF4, 0x3F, 0x84, 0xC2],
            "cmd3 wire mismatch"
        );
    }

    // -- Empty payload --

    #[test]
    fn encrypt_empty_payload() {
        let mut enc = SimpleEncryption::new(0xAABBCCDD, EncryptionMode::XorAll);
        let result = enc.encrypt_command(0x01, &[], false);
        assert_eq!(result.len(), 2); // control byte + opcode only
    }

    // -- Encrypt then decrypt full host/firmware simulation --

    #[test]
    fn full_write_read_cycle_stays_in_lockstep() {
        let seed = 0xBADC0FFE;
        let mut host = SimpleEncryption::new(seed, EncryptionMode::XorAll);
        let mut fw = SimpleEncryption::new(seed, EncryptionMode::XorAll);

        // Simulate 10 write-read cycles
        for cycle in 0..10 {
            let opcode = (cycle * 7 + 3) as u8; // arbitrary opcodes
            let payload: Vec<u8> = (0..cycle).map(|i| (i * 13 + 5) as u8).collect();

            // Host encrypts
            let encrypted = host.encrypt_command(opcode, &payload, false);

            // Firmware decrypts command (skipping control byte).
            // Data bytes start at sb1, so firmware XORs with seed_byte(i+1).
            let fw_cmd_seed = fw.seed();
            let mut cmd = encrypted[1..].to_vec();
            for (i, b) in cmd.iter_mut().enumerate() {
                *b ^= seed_byte(fw_cmd_seed, i + 1);
            }
            fw.advance_seed();

            // Verify firmware got correct opcode and payload
            assert_eq!(
                cmd[0], opcode,
                "cycle {cycle}: firmware decoded wrong opcode"
            );
            assert_eq!(
                &cmd[1..],
                &payload[..],
                "cycle {cycle}: firmware decoded wrong payload"
            );

            // Firmware sends response (just echo back payload length as 1 byte).
            // Responses use simple cycling XOR from sb0.
            let response_plain = vec![payload.len() as u8];
            let fw_resp_seed = fw.seed();
            let mut response_enc = response_plain.clone();
            for (i, b) in response_enc.iter_mut().enumerate() {
                *b ^= seed_byte(fw_resp_seed, i);
            }
            fw.advance_seed();

            // Host decrypts response
            let mut response = response_enc;
            host.decrypt_response(&mut response, false);
            assert_eq!(
                response, response_plain,
                "cycle {cycle}: host decoded wrong response"
            );

            // Both sides should have the same seed now
            assert_eq!(
                host.seed(),
                fw.seed(),
                "cycle {cycle}: host and firmware seeds diverged"
            );
        }
    }

    // -- LFSR non-trivial output verification --

    #[test]
    fn lfsr_step_from_one_produces_expected_sequence() {
        // Pinned first 8 LFSR outputs from seed=1 (taps at bits 0,1,21,31).
        // These values are validated indirectly by the pcap test which
        // depends on the LFSR being exactly right.
        let mut state = 1u32;
        let expected = [
            0x8000_0000,
            0xC000_0000,
            0xE000_0000,
            0xF000_0000,
            0xF800_0000,
            0xFC00_0000,
            0xFE00_0000,
            0xFF00_0000,
        ];
        for &exp in &expected {
            lfsr_step(&mut state);
            assert_eq!(state, exp);
        }
    }

    #[test]
    fn lfsr_step_from_one_pinned() {
        // Pinned LFSR output from seed=1. Taps at bits 0,1,21,31.
        // Step 1: feedback = (1 ^ 0 ^ 0 ^ 0) & 1 = 1
        //         new = (1 >> 1) | (1 << 31) = 0x80000000
        let mut s = 1u32;
        lfsr_step(&mut s);
        assert_eq!(s, 0x8000_0000);
        // Step 2: feedback = (0 ^ 0 ^ 0 ^ 1) & 1 = 1
        //         new = 0x40000000 | 0x80000000 = 0xC0000000
        lfsr_step(&mut s);
        assert_eq!(s, 0xC000_0000);
        lfsr_step(&mut s);
        assert_eq!(s, 0xE000_0000);
        lfsr_step(&mut s);
        assert_eq!(s, 0xF000_0000);
    }

    #[test]
    fn advance_seed_pinned() {
        // Pinned advance_seed output. seed=1: n = (1 & 0x1F) + 0x23 = 36 steps.
        let mut s = 1u32;
        advance_seed(&mut s);
        assert_eq!(s, 0x4199_557f, "advance_seed(1) pinned value");

        // Pinned against the pcap-validated seed (0x6fe77bc8):
        // n = (0xc8 & 0x1F) + 0x23 = 8 + 35 = 43 steps.
        let mut s2 = 0x6fe7_7bc8u32;
        advance_seed(&mut s2);
        assert_eq!(s2, 0x4c80_1767, "advance_seed(0x6fe77bc8) pinned value");
    }

    // -- Multiple reseed tests (TASK-0108) --

    #[test]
    fn encrypt_reseed_always_uses_initial_seed() {
        // The initial/default seed that both host and firmware start with.
        let initial_seed = 0x354b248e_u32;
        let mut enc = SimpleEncryption::new(initial_seed, EncryptionMode::XorAll);

        // Reseed #1: encrypt with initial seed (0x354b248e).
        let seed1_bytes: [u8; 4] = [0xc8, 0x7b, 0xe7, 0x6f]; // 0x6fe77bc8 LE
        let wire1 = enc.encrypt_reseed(&seed1_bytes);
        assert_eq!(
            wire1,
            vec![0x66, 0xec, 0x30, 0xd2, 0xe1],
            "reseed #1 must use initial seed (pcap frame 15)"
        );
        enc.reseed(0x6fe77bc8); // rolling seed changes to new value

        // Do some commands to advance the rolling seed further away from
        // the initial seed, proving encrypt_reseed ignores the rolling state.
        let _ = enc.encrypt_command(PING_OPCODE, &[PING_TEST_BYTE], false);
        let mut dummy = vec![0xAA];
        enc.decrypt_response(&mut dummy, false);

        // Reseed #2: should STILL use initial seed (0x354b248e), NOT the
        // rolling seed. If the old buggy code were used, this would produce
        // different wire bytes because self.seed has changed.
        let seed2_bytes: [u8; 4] = [0x11, 0x22, 0x33, 0x44];
        let wire2 = enc.encrypt_reseed(&seed2_bytes);

        // Compute expected wire bytes using initial_seed as key.
        let key = initial_seed;
        let expected_control =
            ((seed2_bytes[0] & CONTROL_MASK) | RESEED_FLAG) ^ (seed_byte(key, 0) & CONTROL_MASK);
        let expected = vec![
            expected_control,
            seed2_bytes[0] ^ seed_byte(key, 1),
            seed2_bytes[1] ^ seed_byte(key, 2),
            seed2_bytes[2] ^ seed_byte(key, 3),
            seed2_bytes[3] ^ seed_byte(key, 0),
        ];
        assert_eq!(
            wire2, expected,
            "reseed #2 must use initial seed, not rolling state"
        );
        enc.reseed(0x44332211);

        // Reseed #3: still uses initial seed.
        let seed3_bytes: [u8; 4] = [0xAA, 0xBB, 0xCC, 0xDD];
        let wire3 = enc.encrypt_reseed(&seed3_bytes);

        let expected_control3 =
            ((seed3_bytes[0] & CONTROL_MASK) | RESEED_FLAG) ^ (seed_byte(key, 0) & CONTROL_MASK);
        let expected3 = vec![
            expected_control3,
            seed3_bytes[0] ^ seed_byte(key, 1),
            seed3_bytes[1] ^ seed_byte(key, 2),
            seed3_bytes[2] ^ seed_byte(key, 3),
            seed3_bytes[3] ^ seed_byte(key, 0),
        ];
        assert_eq!(
            wire3, expected3,
            "reseed #3 must use initial seed, not rolling state"
        );
    }

    #[test]
    fn reseed_does_not_change_initial_seed() {
        let initial = 0x354b248e_u32;
        let mut enc = SimpleEncryption::new(initial, EncryptionMode::XorAll);

        // Reseed changes the rolling state but not the initial seed.
        enc.reseed(0xAAAAAAAA);
        assert_eq!(enc.seed(), 0xAAAAAAAA, "rolling seed should change");

        // Verify encrypt_reseed still uses the original initial seed by
        // checking it produces the same output as a fresh instance.
        let fresh = SimpleEncryption::new(initial, EncryptionMode::XorAll);
        let test_bytes: [u8; 4] = [0x11, 0x22, 0x33, 0x44];

        let mut enc_clone = enc.clone();
        let wire_after_reseed = enc_clone.encrypt_reseed(&test_bytes);

        let mut fresh_clone = fresh.clone();
        let wire_fresh = fresh_clone.encrypt_reseed(&test_bytes);

        assert_eq!(
            wire_after_reseed, wire_fresh,
            "encrypt_reseed must produce identical output regardless of rolling seed"
        );
    }
}
