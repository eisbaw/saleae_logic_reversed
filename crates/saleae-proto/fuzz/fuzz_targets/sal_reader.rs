#![no_main]

use libfuzzer_sys::fuzz_target;
use saleae_proto::SalReader;

fuzz_target!(|data: &[u8]| {
    // Fuzz the .sal (zip) archive parser -- should never panic.
    // SalReader::from_bytes takes ownership, so we copy.
    let _ = SalReader::from_bytes(data.to_vec());
});
