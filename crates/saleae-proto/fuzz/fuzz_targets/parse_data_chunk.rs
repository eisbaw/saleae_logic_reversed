#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the composite EP2 data chunk parser with various parameters.
    // Try multiple expected_sample_bytes and has_stats combinations to
    // exercise all branches.

    if data.is_empty() {
        return;
    }

    // Use the first byte to select parameters, fuzz the rest as payload.
    let control = data[0];
    let payload = &data[1..];

    let expected_sample_bytes = match control & 0x03 {
        0 => 0,
        1 => 32,
        2 => 256,
        _ => payload.len().saturating_sub(4), // variable size
    };
    let has_stats = control & 0x04 != 0;

    let _ = saleae_proto::capture_data::parse_data_chunk(payload, expected_sample_bytes, has_stats);
});
