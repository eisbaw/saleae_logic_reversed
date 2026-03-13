#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the 12-byte start packet extension parser -- should never panic.
    let _ = saleae_proto::capture_data::StartPacketFields::parse(data);
});
