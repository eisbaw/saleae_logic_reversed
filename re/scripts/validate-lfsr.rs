/// Validate LFSR against pcap ground truth - comprehensive search.
///
/// Pcap data:
///   Frame 15: EP1 OUT reseed  = [66, ec, 30, d2, e1]  (5 bytes)
///   Frame 17: EP1 OUT ping    = [95, 06, b2]           (3 bytes)
///   Frame 20: EP1 IN  resp    = [cd]                   (1 byte)
///   Frame 21: EP1 OUT cmd2    = [96, 42]               (2 bytes)
///   Frame 24: EP1 IN  resp2   = [6a, b2, e5, 0c, ...] (128 bytes)
///   Frame 25: EP1 OUT cmd3    = [d3, 38, bf, 43, f4, 3f, 84, c2] (8 bytes)
///   Frame 28: EP1 IN  resp3   = [09, a1, 9f, 5c, 4d, 29, 0b, 31] (8 bytes)

fn main() {
    println!("=== KEY INSIGHT ===");
    println!("Our control byte formula is PROVABLY incompatible with the pcap.");
    println!("(0x7D & 0xD7) ^ (0x7B & 0xD7) = 0x06, but pcap byte 0 = 0x95");
    println!();

    // The REQUIRED seed state for the ping, assuming format:
    // [0x00, opcode, payload] with ALL bytes plain XOR'd (including first):
    //   sb0 = 0x95 (0x00 ^ sb0 = 0x95)
    //   sb1 = 0x7B (0x7D ^ sb1 = 0x06)
    //   sb2 = 0xE7 (0x55 ^ sb2 = 0xB2)
    // After iterate, response: sb0_new = 0x67 (0xCD ^ sb0_new = 0xAA)

    let reseed = [0x66u8, 0xec, 0x30, 0xd2, 0xe1];

    println!("=== Testing: does seed from reseed iterate to required state? ===");
    println!();

    // Seed derivation candidates
    let seed_candidates: Vec<(u32, &str)> = vec![
        (u32::from_le_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] LE"),
        (u32::from_le_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] LE"),
        (u32::from_be_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] BE"),
        (u32::from_be_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] BE"),
    ];

    // Required states (sb0,sb1,sb2 fixed, sb3 unknown)
    // For plain XOR format: sb0=0x95
    let required_sb0: u8 = 0x95;
    let required_sb1: u8 = 0x7B;
    let required_sb2: u8 = 0xE7;
    let required_resp_sb0: u8 = 0x67;

    println!("--- Old LFSR (sigrok: taps 0,1,21,31) ---");
    for (seed, desc) in &seed_candidates {
        for iters in 0..=3u32 {
            let mut state = *seed;
            for _ in 0..iters {
                old_lfsr_iterate(&mut state);
            }
            let sb0 = (state & 0xFF) as u8;
            let sb1 = ((state >> 8) & 0xFF) as u8;
            let sb2 = ((state >> 16) & 0xFF) as u8;

            if sb0 == required_sb0 && sb1 == required_sb1 && sb2 == required_sb2 {
                // Check response constraint
                let mut resp_state = state;
                old_lfsr_iterate(&mut resp_state);
                let resp_sb0 = (resp_state & 0xFF) as u8;
                let resp_ok = resp_sb0 == required_resp_sb0;
                println!("  MATCH: {} (seed={:#010x}) after {} iterate(s) -> state={:#010x} resp_sb0={:#04x} {}",
                    desc, seed, iters, state,
                    resp_sb0, if resp_ok { "FULL MATCH!" } else { "(resp wrong)" });
            }
        }
    }
    println!();

    println!("--- Our LFSR (disasm: taps 1,21,22) ---");
    for (seed, desc) in &seed_candidates {
        for iters in 0..=3u32 {
            let mut state = *seed;
            for _ in 0..iters {
                our_advance_seed(&mut state);
            }
            let sb0 = (state & 0xFF) as u8;
            let sb1 = ((state >> 8) & 0xFF) as u8;
            let sb2 = ((state >> 16) & 0xFF) as u8;

            if sb0 == required_sb0 && sb1 == required_sb1 && sb2 == required_sb2 {
                let mut resp_state = state;
                our_advance_seed(&mut resp_state);
                let resp_sb0 = (resp_state & 0xFF) as u8;
                let resp_ok = resp_sb0 == required_resp_sb0;
                println!("  MATCH: {} (seed={:#010x}) after {} advance(s) -> state={:#010x} resp_sb0={:#04x} {}",
                    desc, seed, iters, state,
                    resp_sb0, if resp_ok { "FULL MATCH!" } else { "(resp wrong)" });
            }
        }
    }
    println!();

    // -----------------------------------------------------------------------
    // Also try: reseed is raw, first byte is a marker, seed = bytes[1..5]
    // LFSR is NOT iterated after reseed (direct use of seed for ping)
    // -----------------------------------------------------------------------
    println!("--- Raw seed = bytes[1..5], no iteration, plain XOR ---");
    let seed = u32::from_le_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]);
    println!("  seed = {:#010x}", seed);
    println!("  sb0={:#04x} sb1={:#04x} sb2={:#04x} sb3={:#04x}",
        seed & 0xFF, (seed >> 8) & 0xFF, (seed >> 16) & 0xFF, (seed >> 24) & 0xFF);
    println!("  Required: sb0={:#04x} sb1={:#04x} sb2={:#04x}",
        required_sb0, required_sb1, required_sb2);
    println!();

    // -----------------------------------------------------------------------
    // NEW HYPOTHESIS: Maybe there IS no 0x00 prefix.
    // Maybe the format is: [opcode, payload] with first byte having special mask.
    // The 3 pcap bytes are: opcode=0x7D encrypted to 3 bytes = [ctrl, enc_op, enc_pay]
    // But maybe it's [enc_op_masked, enc_pay0, enc_pay1]
    // where the ping has 2 payload bytes [0x55, ???]
    // OR the ping is [opcode_hi, opcode_lo, payload] = 3 plaintext bytes
    // -----------------------------------------------------------------------
    println!("=== Alternative: format is plain XOR, no prefix, no control byte ===");
    println!("  If ping = [0x7D, 0x55] (2 bytes) → can't produce 3 bytes on wire.");
    println!("  So either there's a prefix, or the command has 3 plaintext bytes.");
    println!();

    // -----------------------------------------------------------------------
    // CRITICAL TEST: Try our LFSR with the actual reseed seed, see what we get
    // for various formats. The key question is whether our LFSR or sigrok's
    // is correct.
    // -----------------------------------------------------------------------
    println!("=== Full pcap validation for both LFSRs ===");
    println!("  Testing all seed derivations × all formats");
    println!();

    // For command 2 (frame 21, 2 bytes = [96, 42]):
    // If format [0x00, opcode], then:
    //   sb0(S3) = 0x96, opcode ^ sb1(S3) = 0x42
    // If opcode = 0x8B (ReadFwVersion): sb1(S3) = 0xC9
    // If opcode = 0x07 (ReadEeprom): sb1(S3) = 0x45

    for (seed, desc) in &seed_candidates {
        for lfsr_name in &["old", "our"] {
            for iters in 0..=2u32 {
                let mut state = *seed;
                for _ in 0..iters {
                    match *lfsr_name {
                        "old" => old_lfsr_iterate(&mut state),
                        _ => our_advance_seed(&mut state),
                    }
                }

                // Ping state S1
                let s1 = state;
                let sb0 = (s1 & 0xFF) as u8;
                let sb1 = ((s1 >> 8) & 0xFF) as u8;
                let sb2 = ((s1 >> 16) & 0xFF) as u8;

                // Format A: [0x00, 0x7D, 0x55] plain XOR
                let enc_a = [0x00 ^ sb0, 0x7D ^ sb1, 0x55 ^ sb2];
                let ping_match_a = enc_a == [0x95, 0x06, 0xB2];

                if ping_match_a {
                    // Advance for ping
                    let mut s2 = s1;
                    match *lfsr_name {
                        "old" => old_lfsr_iterate(&mut s2),
                        _ => our_advance_seed(&mut s2),
                    }

                    // Decrypt response
                    let resp_dec = 0xCD ^ (s2 & 0xFF) as u8;

                    // Advance for response
                    let mut s3 = s2;
                    match *lfsr_name {
                        "old" => old_lfsr_iterate(&mut s3),
                        _ => our_advance_seed(&mut s3),
                    }

                    // Check command 2
                    let cmd2_sb0 = (s3 & 0xFF) as u8;
                    let cmd2_sb1 = ((s3 >> 8) & 0xFF) as u8;
                    let cmd2_enc0 = 0x00 ^ cmd2_sb0;
                    // Try various opcodes for command 2
                    let cmd2_opcodes = [(0x8B, "ReadFwVer"), (0x07, "ReadEeprom"),
                                        (0x81, "ReadReg"), (0x7D, "Ping2")];
                    let mut cmd2_match = false;
                    let mut cmd2_opcode_name = "";
                    for (op, name) in &cmd2_opcodes {
                        let cmd2_enc1 = op ^ cmd2_sb1;
                        if cmd2_enc0 == 0x96 && cmd2_enc1 == 0x42 {
                            cmd2_match = true;
                            cmd2_opcode_name = name;
                        }
                    }

                    println!("  *** PING MATCH: {} LFSR, {} (seed={:#010x}), {} iters",
                        lfsr_name, desc, seed, iters);
                    println!("      S1={:#010x}, resp_dec={:#04x} (expect 0xAA): {}",
                        s1, resp_dec, if resp_dec == 0xAA { "OK" } else { "WRONG" });
                    if cmd2_match {
                        println!("      cmd2 opcode={} S3={:#010x}: MATCH", cmd2_opcode_name, s3);
                    } else {
                        println!("      cmd2: S3={:#010x}, enc0_expect={:#04x} (have 0x96), no opcode match",
                            s3, cmd2_enc0);
                    }
                    println!();
                }
            }
        }
    }
}

// Old LFSR (sigrok: taps 0,1,21,31)
fn old_lfsr_step(state: &mut u32) {
    let bit = (*state ^ (*state >> 1) ^ (*state >> 21) ^ (*state >> 31)) & 1;
    *state = (*state >> 1) | (bit << 31);
}

fn old_lfsr_iterate(state: &mut u32) {
    if *state == 0 {
        *state = 1;
        return;
    }
    let max = (*state & 0x1f) + 34;
    for _ in 0..=max {
        old_lfsr_step(state);
    }
}

// Our LFSR (decompiled: taps 1,21,22)
fn our_lfsr_step(state: &mut u32) {
    let s = *state;
    let shifted = s >> 1;
    let feedback = shifted ^ s;
    let tap21 = ((s >> 21) ^ feedback) << 31;
    let msb = (s & 0x8000_0000) ^ tap21;
    let combined = (msb.wrapping_add(shifted)) >> 1;
    let newval = ((shifted ^ (s >> 22)) ^ combined) << 31;
    *state = (newval ^ msb) | combined;
}

fn our_advance_seed(state: &mut u32) {
    let n = (*state & 0x1F) + 0x23;
    let mut i = 0u32;
    while i < n {
        our_lfsr_step(state);
        if i + 1 < n {
            our_lfsr_step(state);
        }
        i += 2;
    }
}
