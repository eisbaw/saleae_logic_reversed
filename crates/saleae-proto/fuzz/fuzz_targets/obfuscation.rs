#![no_main]

use libfuzzer_sys::fuzz_target;
use saleae_proto::{EncryptionMode, SimpleEncryption};

fuzz_target!(|data: &[u8]| {
    if data.len() < 6 {
        return;
    }

    // Use first 4 bytes as seed, 5th byte selects mode, 6th byte as opcode.
    let seed = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let mode = match data[4] % 3 {
        0 => EncryptionMode::None,
        1 => EncryptionMode::XorAll,
        _ => EncryptionMode::XorFirmwareOnly,
    };
    let opcode = data[5];
    let payload = &data[6..];

    // Encrypt a command -- should never panic.
    let mut enc = SimpleEncryption::new(seed, mode);
    let _encrypted = enc.encrypt_command(opcode, payload, false);

    // Decrypt a response -- should never panic.
    let mut dec = SimpleEncryption::new(seed, mode);
    let mut response = payload.to_vec();
    dec.decrypt_response(&mut response, false);
});
