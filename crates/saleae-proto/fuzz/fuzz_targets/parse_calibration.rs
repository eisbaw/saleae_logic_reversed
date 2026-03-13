#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz the calibration XML parser -- should never panic.
    // This parses untrusted Boost.Serialization XML from the cal server.
    let _ = saleae_proto::calibration::parse_calibration(data);
});
