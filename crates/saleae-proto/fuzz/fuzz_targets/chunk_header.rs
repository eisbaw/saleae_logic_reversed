#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the 4-byte EP2 chunk header parser -- should never panic.
    let _ = saleae_proto::capture_data::ChunkHeader::parse(data);
});
