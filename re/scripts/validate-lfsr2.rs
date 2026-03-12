/// Validate LFSR against pcap ground truth with CORRECTED iteration count.
///
/// Key finding from disassembly at VA 0x2d99290:
/// The advance_seed loop does ONE lfsr_step per iteration with stride-2 counter.
/// Total steps = ceil(N/2), NOT N as our RE notes claimed.
///
/// Pcap data (Saleae Logic 2 v2.4.40, Logic Pro 8):
///   Frame 15: EP1 OUT 5 bytes: [66, ec, 30, d2, e1]  (reseed)
///   Frame 17: EP1 OUT 3 bytes: [95, 06, b2]           (encrypted ping)
///   Frame 20: EP1 IN  1 byte:  [cd]                   (encrypted response)
///   Frame 21: EP1 OUT 2 bytes: [96, 42]               (encrypted cmd2)
///   Frame 24: EP1 IN  128 bytes: [6a, b2, e5, 0c, ...] (response 2)
///   Frame 25: EP1 OUT 8 bytes: [d3, 38, bf, 43, f4, 3f, 84, c2] (cmd3)
///   Frame 28: EP1 IN  8 bytes: [09, a1, 9f, 5c, 4d, 29, 0b, 31] (response 3)

fn main() {
    let reseed = [0x66u8, 0xec, 0x30, 0xd2, 0xe1];

    // Seed candidates: which 4 bytes of the 5 are the actual seed?
    let seed_candidates: Vec<(u32, &str)> = vec![
        (u32::from_le_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] LE"),
        (u32::from_le_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] LE"),
        (u32::from_be_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] BE"),
        (u32::from_be_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] BE"),
        // Also try: all 5 bytes combined somehow
        (u32::from_le_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]) ^ (reseed[4] as u32), "LE[0..4] ^ byte4"),
        (u32::from_le_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]) ^ (reseed[0] as u32), "LE[1..5] ^ byte0"),
    ];

    println!("=== CORRECTED advance_seed: ceil(N/2) steps instead of N ===");
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
            let _sb3 = ((state >> 24) & 0xFF) as u8;

            // Format: [0x00, opcode, payload] plain XOR (3 plaintext -> 3 ciphertext)
            let enc = [0x00 ^ sb0, 0x7D ^ sb1, 0x55 ^ sb2];
            if enc == [0x95, 0x06, 0xB2] {
                // Check response after advance
                let mut resp_state = state;
                advance_seed_corrected(&mut resp_state);
                let resp_sb0 = (resp_state & 0xFF) as u8;
                let resp_dec = 0xCD ^ resp_sb0;

                // Check cmd2 after another advance
                let mut cmd2_state = resp_state;
                advance_seed_corrected(&mut cmd2_state);
                let c2_sb0 = (cmd2_state & 0xFF) as u8;
                let c2_sb1 = ((cmd2_state >> 8) & 0xFF) as u8;
                // cmd2 = [96, 42], if format [0x00, opcode]: 0x00^c2_sb0=0x96, op^c2_sb1=0x42
                let c2_opcode = 0x42 ^ c2_sb1;

                println!("*** MATCH [0x00+XOR]: {} seed={:#010x} iters={} state={:#010x}",
                    desc, seed, iters, state);
                println!("    resp_dec={:#04x} (want 0xAA): {}",
                    resp_dec, if resp_dec == 0xAA { "OK" } else { "WRONG" });
                println!("    cmd2: c2_sb0={:#04x} (want 0x96 for 0x00^sb0): {}",
                    c2_sb0, if c2_sb0 == 0x96 { "OK" } else { "WRONG" });
                println!("    cmd2 opcode would be: {:#04x}", c2_opcode);
                println!();
            }

            // Format: opcode is first byte, plain XOR (no prefix)
            // ping = [0x7D, 0x55] -> 2 bytes + control byte = 3 bytes
            // control byte = (opcode & mask) ^ (sb0 & mask)
            // But we proved this wrong for 0xD7 mask. Try other masks.
            for mask in [0xFF_u8, 0xFE, 0xFC, 0xF7, 0xEF, 0xDF, 0xBF, 0x00] {
                let ctrl = (0x7D & mask) ^ (sb0 & mask);
                // output[0] = ctrl, output[1] = 0x7D ^ sb0, output[2] = 0x55 ^ sb1
                let enc2 = [ctrl, 0x7D ^ sb0, 0x55 ^ sb1];
                if enc2 == [0x95, 0x06, 0xB2] {
                    let mut resp_state = state;
                    advance_seed_corrected(&mut resp_state);
                    let resp_sb0 = (resp_state & 0xFF) as u8;
                    let resp_dec = 0xCD ^ resp_sb0;

                    println!("*** MATCH [ctrl+XOR, mask={:#04x}]: {} seed={:#010x} iters={} state={:#010x}",
                        mask, desc, seed, iters, state);
                    println!("    resp_dec={:#04x} (want 0xAA): {}",
                        resp_dec, if resp_dec == 0xAA { "OK" } else { "WRONG" });
                    println!();
                }
            }
        }
    }

    // Also try: no advance after reseed (seed used directly, then advance after ping)
    println!("=== Also testing: seed used directly (no advance before ping) ===");
    println!();
    for (seed, desc) in &seed_candidates {
        let state = *seed;
        let sb0 = (state & 0xFF) as u8;
        let sb1 = ((state >> 8) & 0xFF) as u8;
        let sb2 = ((state >> 16) & 0xFF) as u8;

        let enc = [0x00 ^ sb0, 0x7D ^ sb1, 0x55 ^ sb2];
        if enc == [0x95, 0x06, 0xB2] {
            println!("  MATCH (direct): {} seed={:#010x}", desc, seed);
        }
    }

    // Brute force: find ANY 32-bit state that produces the right ping encoding
    // using our corrected LFSR, then check if it's reachable from any seed candidate
    println!();
    println!("=== Brute force: find states producing correct ping ===");
    println!("  Format: [0x00, 0x7D, 0x55] XOR'd with state bytes");
    println!("  Required: sb0=0x95, sb1=0x7B, sb2=0xE7");
    println!();

    // State must have sb0=0x95, sb1=0x7B, sb2=0xE7, sb3=unknown
    // After corrected advance, resp_sb0 must give 0xCD ^ resp_sb0 = 0xAA => resp_sb0 = 0x67
    let mut found_states: Vec<u32> = Vec::new();
    for sb3 in 0..=255u32 {
        let state = 0x95 | (0x7B << 8) | (0xE7 << 16) | (sb3 << 24);
        let mut resp_state = state;
        advance_seed_corrected(&mut resp_state);
        let resp_sb0 = (resp_state & 0xFF) as u8;
        if resp_sb0 == 0x67 {  // 0xCD ^ 0xAA
            // Also check cmd2
            let mut cmd2_state = resp_state;
            advance_seed_corrected(&mut cmd2_state);
            let c2_sb0 = (cmd2_state & 0xFF) as u8;

            println!("  State {:#010x}: resp OK, cmd2_sb0={:#04x} (want 0x96): {}",
                state, c2_sb0, if c2_sb0 == 0x96 { "MATCH" } else { "no" });
            found_states.push(state);
        }
    }

    if found_states.is_empty() {
        println!("  No states found with corrected LFSR. Trying old (sigrok) LFSR...");
        for sb3 in 0..=255u32 {
            let state = 0x95 | (0x7B << 8) | (0xE7 << 16) | (sb3 << 24);
            let mut resp_state = state;
            old_lfsr_iterate(&mut resp_state);
            let resp_sb0 = (resp_state & 0xFF) as u8;
            if resp_sb0 == 0x67 {
                let mut cmd2_state = resp_state;
                old_lfsr_iterate(&mut cmd2_state);
                let c2_sb0 = (cmd2_state & 0xFF) as u8;
                println!("  [OLD LFSR] State {:#010x}: resp OK, cmd2_sb0={:#04x} (want 0x96): {}",
                    state, c2_sb0, if c2_sb0 == 0x96 { "MATCH" } else { "no" });
            }
        }
    }

    // For each found state, check if it's reachable from any seed candidate
    if !found_states.is_empty() {
        println!();
        println!("=== Checking reachability from seed candidates ===");
        for (seed, desc) in &seed_candidates {
            for steps in 0..=5u32 {
                let mut state = *seed;
                for _ in 0..steps {
                    advance_seed_corrected(&mut state);
                }
                for target in &found_states {
                    if state == *target {
                        println!("  REACHABLE: {} ({:#010x}) -> {} advance(s) -> {:#010x}",
                            desc, seed, steps, target);
                    }
                }
            }
        }
    }
}

// Our LFSR step (verified against assembly at VA 0x2d99290)
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

// CORRECTED advance_seed: ceil(N/2) steps instead of N
// Assembly shows: loop counter starts at -(N & ~1), increments by 2, exits at 0
// That's (N & ~1)/2 iterations, each doing ONE step
// Plus one extra step if N is odd
fn advance_seed_corrected(state: &mut u32) {
    let n = (*state & 0x1F) as u32 + 0x23;
    let n_even = n & !1;
    // Main loop: (n_even / 2) steps
    for _ in 0..(n_even / 2) {
        lfsr_step(state);
    }
    // Odd handling: one extra step
    if n & 1 != 0 {
        lfsr_step(state);
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
