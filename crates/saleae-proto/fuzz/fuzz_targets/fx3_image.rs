#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the FX3 image parser -- should never panic.
    let _ = saleae_proto::firmware::fx3::Fx3Image::parse(data);
});
