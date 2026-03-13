#![no_main]

use libfuzzer_sys::fuzz_target;
use saleae_proto::{Disaggregator, LOGIC_PRO_8_PROFILE};

fuzz_target!(|data: &[u8]| {
    // Fuzz the Pro disaggregator with a fixed device profile.
    // Uses Logic Pro 8 profile with 2 digital + 2 analog channels at
    // representative rates.
    let Ok(disag) = Disaggregator::new(
        &LOGIC_PRO_8_PROFILE,
        &[0, 1],    // 2 digital channels
        &[0, 1],    // 2 analog channels
        500_000,    // 500 kHz digital
        500_000,    // 500 kHz analog
    ) else {
        return;
    };

    let _ = disag.process_chunk(data);
});
