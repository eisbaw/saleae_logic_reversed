#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the 20-byte stats trailer parser -- should never panic.
    let _ = saleae_proto::capture_data::StatsHeader::parse(data);
});
