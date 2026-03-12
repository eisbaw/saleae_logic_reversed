/// Validate LFSR against pcap - corrected iteration count + correct byte alignment.
///
/// KEY FINDING from assembly at VA 0x2d99290:
/// advance_seed does ceil(N/2) LFSR steps, NOT N steps as our RE notes claimed.
///
/// Format from RE notes (xor-obfuscation.md):
///   output[0] = control byte (uses sb0 with mask)
///   output[1] = opcode ^ sb0   ← note: SAME seed byte as control
///   output[2] = payload0 ^ sb1
///   output[3] = payload1 ^ sb2
///   output[4] = payload2 ^ sb3
///   output[5] = payload3 ^ sb0 (wraps)
///
/// Pcap (Saleae Logic 2, Logic Pro 8):
///   Frame 15: EP1 OUT 5 bytes: [66, ec, 30, d2, e1]  (reseed)
///   Frame 17: EP1 OUT 3 bytes: [95, 06, b2]           (encrypted ping)
///   Frame 20: EP1 IN  1 byte:  [cd]                   (encrypted response)
///   Frame 21: EP1 OUT 2 bytes: [96, 42]               (encrypted cmd2)

fn main() {
    let reseed = [0x66u8, 0xec, 0x30, 0xd2, 0xe1];

    let seed_candidates: Vec<(u32, &str)> = vec![
        (u32::from_le_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] LE"),
        (u32::from_le_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] LE"),
        (u32::from_be_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] BE"),
        (u32::from_be_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] BE"),
    ];

    // ========== FORMAT A: control byte + opcode/payload XOR from sb0 ==========
    // From xor-obfuscation.md:
    //   output[0] = control (unknown formula of opcode + sb0)
    //   output[1] = opcode ^ sb0
    //   output[2] = payload0 ^ sb1
    //
    // For ping [0x7D, 0x55] -> 3 wire bytes [0x95, 0x06, 0xB2]:
    //   0x06 = 0x7D ^ sb0 => sb0 = 0x7B
    //   0xB2 = 0x55 ^ sb1 => sb1 = 0xE7
    //   Required state: sb0=0x7B, sb1=0xE7, sb2/sb3 unknown
    //
    // Response [0xCD] -> decrypted with next state sb0:
    //   0xCD ^ sb0' = 0xAA => sb0' = 0x67
    //
    // cmd2 [0x96, 0x42] -> 2 wire bytes, 1 plaintext byte [opcode]:
    //   0x42 = opcode ^ sb0'' (after 2nd advance)

    println!("=== FORMAT A: control + XOR from sb0 (RE notes format) ===");
    println!("  Required: sb0=0x7B, sb1=0xE7");
    println!();

    for (seed, desc) in &seed_candidates {
        for iters in 0..=3u32 {
            let mut state = *seed;
            for _ in 0..iters {
                advance_seed_corrected(&mut state);
            }

            let sb0 = (state & 0xFF) as u8;
            let sb1 = ((state >> 8) & 0xFF) as u8;

            if sb0 == 0x7B && sb1 == 0xE7 {
                let mut resp_state = state;
                advance_seed_corrected(&mut resp_state);
                let resp_sb0 = (resp_state & 0xFF) as u8;
                let resp_dec = 0xCD ^ resp_sb0;

                let mut cmd2_state = resp_state;
                advance_seed_corrected(&mut cmd2_state);
                let cmd2_sb0 = (cmd2_state & 0xFF) as u8;

                println!("  MATCH: {} seed={:#010x} iters={} state={:#010x}",
                    desc, seed, iters, state);
                println!("    resp: 0xCD ^ {:#04x} = {:#04x} (want 0xAA): {}",
                    resp_sb0, resp_dec, if resp_dec == 0xAA { "OK" } else { "WRONG" });
                println!("    cmd2: sb0={:#04x}, opcode would be {:#04x}",
                    cmd2_sb0, 0x42 ^ cmd2_sb0);
                println!();
            }
        }
    }

    // ========== FORMAT B: [0x00, opcode, payload] plain XOR ==========
    // output[0] = 0x00 ^ sb0
    // output[1] = opcode ^ sb1
    // output[2] = payload0 ^ sb2
    //
    // For ping [0x7D, 0x55] -> 3 wire bytes [0x95, 0x06, 0xB2]:
    //   sb0 = 0x95, sb1 = 0x7B, sb2 = 0xE7
    //   Response: 0xCD ^ sb0' = 0xAA => sb0' = 0x67

    println!("=== FORMAT B: [0x00, opcode, payload] plain XOR ===");
    println!("  Required: sb0=0x95, sb1=0x7B, sb2=0xE7");
    println!();

    for (seed, desc) in &seed_candidates {
        for iters in 0..=3u32 {
            let mut state = *seed;
            for _ in 0..iters {
                advance_seed_corrected(&mut state);
            }

            let sb0 = (state & 0xFF) as u8;
            let sb1 = ((state >> 8) & 0xFF) as u8;
            let sb2 = ((state >> 16) & 0xFF) as u8;

            if sb0 == 0x95 && sb1 == 0x7B && sb2 == 0xE7 {
                let mut resp_state = state;
                advance_seed_corrected(&mut resp_state);
                let resp_sb0 = (resp_state & 0xFF) as u8;
                let resp_dec = 0xCD ^ resp_sb0;

                println!("  MATCH: {} seed={:#010x} iters={} state={:#010x}",
                    desc, seed, iters, state);
                println!("    resp: 0xCD ^ {:#04x} = {:#04x} (want 0xAA): {}",
                    resp_sb0, resp_dec, if resp_dec == 0xAA { "OK" } else { "WRONG" });
                println!();
            }
        }
    }

    // ========== Brute force: all sb2/sb3 for FORMAT A ==========
    println!("=== Brute force FORMAT A: sb0=0x7B, sb1=0xE7, all sb2/sb3 ===");
    let mut format_a_matches = Vec::new();
    for sb2 in 0..=255u32 {
        for sb3 in 0..=255u32 {
            let state = 0x7B | (0xE7 << 8) | (sb2 << 16) | (sb3 << 24);
            let mut resp_state = state;
            advance_seed_corrected(&mut resp_state);
            let resp_sb0 = (resp_state & 0xFF) as u8;
            if resp_sb0 == 0x67 {
                let mut cmd2_state = resp_state;
                advance_seed_corrected(&mut cmd2_state);
                let cmd2_sb0 = (cmd2_state & 0xFF) as u8;
                let cmd2_opcode = 0x42 ^ cmd2_sb0;
                let cmd2_ctrl_ok = (cmd2_state & 0xFF) as u8;

                // Check if cmd2_sb0 produces a valid control byte (wire[0] = 0x96)
                // Control byte unknown formula, but wire[0] MUST be 0x96
                format_a_matches.push((state, cmd2_sb0, cmd2_opcode));
            }
        }
    }
    println!("  Found {} states with correct ping + response", format_a_matches.len());
    if format_a_matches.len() <= 20 {
        for (state, cmd2_sb0, cmd2_opcode) in &format_a_matches {
            // Check if reachable from any seed candidate
            let mut reachable = false;
            let mut reach_desc = String::new();
            for (seed, desc) in &seed_candidates {
                for steps in 0..=5u32 {
                    let mut s = *seed;
                    for _ in 0..steps {
                        advance_seed_corrected(&mut s);
                    }
                    if s == *state {
                        reachable = true;
                        reach_desc = format!("{} +{} advance(s)", desc, steps);
                    }
                }
            }
            println!("  {:#010x}: cmd2_sb0={:#04x} opcode={:#04x} reachable={}",
                state, cmd2_sb0, cmd2_opcode,
                if reachable { &reach_desc } else { "NO" });
        }
    } else {
        // Check reachability for all
        let mut any_reachable = false;
        for (state, cmd2_sb0, cmd2_opcode) in &format_a_matches {
            for (seed, desc) in &seed_candidates {
                for steps in 0..=5u32 {
                    let mut s = *seed;
                    for _ in 0..steps {
                        advance_seed_corrected(&mut s);
                    }
                    if s == *state {
                        println!("  REACHABLE: {:#010x} via {} +{} advance(s), cmd2_opcode={:#04x}",
                            state, desc, steps, cmd2_opcode);
                        any_reachable = true;
                    }
                }
            }
        }
        if !any_reachable {
            println!("  NONE are reachable from seed candidates");
        }
    }

    // ========== Also try with OLD (wrong) advance_seed (N steps) ==========
    println!();
    println!("=== Same but with OLD advance_seed (N steps) for comparison ===");
    for (seed, desc) in &seed_candidates {
        for iters in 0..=3u32 {
            let mut state = *seed;
            for _ in 0..iters {
                advance_seed_old(&mut state);
            }
            let sb0 = (state & 0xFF) as u8;
            let sb1 = ((state >> 8) & 0xFF) as u8;
            if sb0 == 0x7B && sb1 == 0xE7 {
                let mut resp_state = state;
                advance_seed_old(&mut resp_state);
                let resp_sb0 = (resp_state & 0xFF) as u8;
                println!("  OLD MATCH: {} seed={:#010x} iters={} resp_sb0={:#04x} (want 0x67)",
                    desc, seed, iters, resp_sb0);
            }
        }
    }
}

fn lfsr_step(state: &mut u32) {
    let s = *state;
    let shifted = s >> 1;
    let feedback = shifted ^ s;
    let tap21 = ((s >> 21) ^ feedback) << 31;
    let msb = (s & 0x8000_0000) ^ tap21;
    let combined = (msb.wrapping_add(shifted)) >> 1;
    let newval = ((shifted ^ (s >> 22)) ^ combined) << 31;
    *state = (newval ^ msb) | combined;
}

// CORRECTED: ceil(N/2) steps
fn advance_seed_corrected(state: &mut u32) {
    let n = (*state & 0x1F) as u32 + 0x23;
    let n_even = n & !1;
    for _ in 0..(n_even / 2) {
        lfsr_step(state);
    }
    if n & 1 != 0 {
        lfsr_step(state);
    }
}

// OLD (wrong): N steps
fn advance_seed_old(state: &mut u32) {
    let n = (*state & 0x1F) as u32 + 0x23;
    let mut i = 0u32;
    while i < n {
        lfsr_step(state);
        if i + 1 < n {
            lfsr_step(state);
        }
        i += 2;
    }
}
