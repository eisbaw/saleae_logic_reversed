/// Validate LFSR against pcap - with CORRECT SetControlByte understanding.
///
/// KEY FINDINGS from assembly:
/// 1. advance_seed does ceil(N/2) steps, not N (VA 0x2d99290)
/// 2. SetControlByte (VA 0x2d987b0) does NOT XOR with seed.
///    It just: output[0] = opcode & 0xD7, output[1..] = source[0..]
///    The XOR with cycling seed bytes happens separately in XorEncrypt.
/// 3. XorEncrypt XORs byte[i] with seed_byte[i % 4]:
///    wire[0] = (opcode & 0xD7) ^ sb0  (control ^ seed byte 0)
///    wire[1] = opcode ^ sb1            (opcode ^ seed byte 1)
///    wire[2] = payload0 ^ sb2          (payload ^ seed byte 2)
///    wire[3] = payload1 ^ sb3          (payload ^ seed byte 3)
///
/// Pcap:
///   Frame 15: EP1 OUT 5 bytes: [66, ec, 30, d2, e1]  (reseed)
///   Frame 17: EP1 OUT 3 bytes: [95, 06, b2]           (encrypted ping)
///   Frame 20: EP1 IN  1 byte:  [cd]                   (encrypted response)
///   Frame 21: EP1 OUT 2 bytes: [96, 42]               (encrypted cmd2)

fn main() {
    let reseed = [0x66u8, 0xec, 0x30, 0xd2, 0xe1];

    // Ping [0x7D, 0x55]:
    // SetControlByte produces: [0x7D & 0xD7, 0x7D, 0x55] = [0x55, 0x7D, 0x55]
    // XorEncrypt:
    //   wire[0] = 0x55 ^ sb0 = 0x95 => sb0 = 0xC0
    //   wire[1] = 0x7D ^ sb1 = 0x06 => sb1 = 0x7B
    //   wire[2] = 0x55 ^ sb2 = 0xB2 => sb2 = 0xE7
    //
    // Response [0xCD] = response[0] ^ sb0' => 0xAA ^ sb0' = 0xCD => sb0' = 0x67

    let required_sb0: u8 = 0xC0;
    let required_sb1: u8 = 0x7B;
    let required_sb2: u8 = 0xE7;

    println!("Required ping state: sb0={:#04x}, sb1={:#04x}, sb2={:#04x}",
        required_sb0, required_sb1, required_sb2);
    println!();

    let seed_candidates: Vec<(u32, &str)> = vec![
        (u32::from_le_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] LE"),
        (u32::from_le_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] LE"),
        (u32::from_be_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] BE"),
        (u32::from_be_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] BE"),
    ];

    println!("=== Direct seed candidate test (corrected advance) ===");
    for (seed, desc) in &seed_candidates {
        for iters in 0..=5u32 {
            let mut state = *seed;
            for _ in 0..iters {
                advance_seed_corrected(&mut state);
            }
            let sb0 = (state & 0xFF) as u8;
            let sb1 = ((state >> 8) & 0xFF) as u8;
            let sb2 = ((state >> 16) & 0xFF) as u8;

            if sb0 == required_sb0 && sb1 == required_sb1 && sb2 == required_sb2 {
                check_full_sequence(state, desc, *seed, iters, "corrected");
            }
        }
    }
    println!();

    // Brute force: find ALL states with sb0=0xC0, sb1=0x7B, sb2=0xE7
    // that also satisfy the response constraint
    println!("=== Brute force: all sb3 values ===");
    let mut found_states: Vec<u32> = Vec::new();
    for sb3 in 0..=255u32 {
        let state = (required_sb0 as u32) | ((required_sb1 as u32) << 8)
                  | ((required_sb2 as u32) << 16) | (sb3 << 24);
        let mut resp_state = state;
        advance_seed_corrected(&mut resp_state);
        let resp_sb0 = (resp_state & 0xFF) as u8;
        if resp_sb0 == 0x67 {
            found_states.push(state);
        }
    }
    println!("  {} states with correct ping + response", found_states.len());

    // Check reachability from seed candidates
    for target in &found_states {
        for (seed, desc) in &seed_candidates {
            for steps in 0..=10u32 {
                let mut state = *seed;
                for _ in 0..steps {
                    advance_seed_corrected(&mut state);
                }
                if state == *target {
                    println!("  REACHABLE: {:#010x} via {} + {} advance(s)", target, desc, steps);
                    check_full_sequence(*target, desc, *seed, steps, "corrected");
                }
            }
        }
    }

    // Also try with OLD (N steps) advance
    println!();
    println!("=== Brute force with OLD advance (N steps) ===");
    let mut found_old: Vec<u32> = Vec::new();
    for sb3 in 0..=255u32 {
        let state = (required_sb0 as u32) | ((required_sb1 as u32) << 8)
                  | ((required_sb2 as u32) << 16) | (sb3 << 24);
        let mut resp_state = state;
        advance_seed_old(&mut resp_state);
        let resp_sb0 = (resp_state & 0xFF) as u8;
        if resp_sb0 == 0x67 {
            found_old.push(state);
        }
    }
    println!("  {} states with OLD advance", found_old.len());
    for target in &found_old {
        for (seed, desc) in &seed_candidates {
            for steps in 0..=10u32 {
                let mut state = *seed;
                for _ in 0..steps {
                    advance_seed_old(&mut state);
                }
                if state == *target {
                    println!("  REACHABLE: {:#010x} via {} + {} old advance(s)", target, desc, steps);
                    check_full_sequence_old(*target, desc, *seed, steps);
                }
            }
        }
    }

    // Also try with sigrok LFSR
    println!();
    println!("=== Brute force with sigrok LFSR ===");
    let mut found_sigrok: Vec<u32> = Vec::new();
    for sb3 in 0..=255u32 {
        let state = (required_sb0 as u32) | ((required_sb1 as u32) << 8)
                  | ((required_sb2 as u32) << 16) | (sb3 << 24);
        let mut resp_state = state;
        sigrok_iterate(&mut resp_state);
        let resp_sb0 = (resp_state & 0xFF) as u8;
        if resp_sb0 == 0x67 {
            found_sigrok.push(state);
        }
    }
    println!("  {} states with sigrok LFSR", found_sigrok.len());
    for target in &found_sigrok {
        for (seed, desc) in &seed_candidates {
            for steps in 0..=10u32 {
                let mut state = *seed;
                for _ in 0..steps {
                    sigrok_iterate(&mut state);
                }
                if state == *target {
                    println!("  REACHABLE: {:#010x} via {} + {} sigrok iterate(s)", target, desc, steps);
                }
            }
        }
    }
}

fn check_full_sequence(state: u32, desc: &str, seed: u32, iters: u32, label: &str) {
    // Ping state
    let sb0 = (state & 0xFF) as u8;
    let sb1 = ((state >> 8) & 0xFF) as u8;
    let sb2 = ((state >> 16) & 0xFF) as u8;
    println!("  *** MATCH ({}) {} seed={:#010x} +{} iters, state={:#010x}",
        label, desc, seed, iters, state);
    println!("      ping: sb0={:#04x} sb1={:#04x} sb2={:#04x}", sb0, sb1, sb2);

    // Response
    let mut resp_state = state;
    advance_seed_corrected(&mut resp_state);
    let resp_sb0 = (resp_state & 0xFF) as u8;
    let resp_dec = 0xCD ^ resp_sb0;
    println!("      resp: sb0'={:#04x}, 0xCD ^ sb0' = {:#04x} (want 0xAA): {}",
        resp_sb0, resp_dec, if resp_dec == 0xAA { "OK" } else { "WRONG" });

    // cmd2
    let mut cmd2_state = resp_state;
    advance_seed_corrected(&mut cmd2_state);
    let c2_sb0 = (cmd2_state & 0xFF) as u8;
    let c2_sb1 = ((cmd2_state >> 8) & 0xFF) as u8;
    // cmd2 wire = [0x96, 0x42]
    // If it's a 1-byte command [opcode], SetControlByte produces [opcode&0xD7, opcode]
    // wire[0] = (opcode&0xD7) ^ c2_sb0 = 0x96
    // wire[1] = opcode ^ c2_sb1 = 0x42
    // So: opcode = 0x42 ^ c2_sb1
    let cmd2_opcode = 0x42 ^ c2_sb1;
    let cmd2_ctrl = (cmd2_opcode & 0xD7) ^ c2_sb0;
    println!("      cmd2: c2_sb0={:#04x} c2_sb1={:#04x}", c2_sb0, c2_sb1);
    println!("      cmd2: opcode={:#04x}, ctrl_check={:#04x} (want 0x96): {}",
        cmd2_opcode, cmd2_ctrl, if cmd2_ctrl == 0x96 { "CONSISTENT" } else { "INCONSISTENT" });

    // cmd3
    let mut cmd3_state = cmd2_state;
    advance_seed_corrected(&mut cmd3_state);
    // Read response 2 (128 bytes) first
    let mut resp2_state = cmd3_state;
    advance_seed_corrected(&mut resp2_state);
    // cmd3 wire: [d3, 38, bf, 43, f4, 3f, 84, c2]
    let cmd3_wire = [0xd3u8, 0x38, 0xbf, 0x43, 0xf4, 0x3f, 0x84, 0xc2];
    let r2_sb0 = (cmd3_state & 0xFF) as u8;
    println!("      resp2 first byte: 0x6a ^ {:#04x} = {:#04x}",
        r2_sb0, 0x6a ^ r2_sb0);
    println!();
}

fn check_full_sequence_old(state: u32, desc: &str, seed: u32, iters: u32) {
    println!("  *** MATCH (old) {} seed={:#010x} +{} iters, state={:#010x}",
        desc, seed, iters, state);
    let mut resp_state = state;
    advance_seed_old(&mut resp_state);
    let resp_sb0 = (resp_state & 0xFF) as u8;
    let resp_dec = 0xCD ^ resp_sb0;
    println!("      resp: 0xCD ^ {:#04x} = {:#04x} (want 0xAA): {}",
        resp_sb0, resp_dec, if resp_dec == 0xAA { "OK" } else { "WRONG" });

    let mut cmd2_state = resp_state;
    advance_seed_old(&mut cmd2_state);
    let c2_sb0 = (cmd2_state & 0xFF) as u8;
    let c2_sb1 = ((cmd2_state >> 8) & 0xFF) as u8;
    let cmd2_opcode = 0x42 ^ c2_sb1;
    let cmd2_ctrl = (cmd2_opcode & 0xD7) ^ c2_sb0;
    println!("      cmd2: opcode={:#04x}, ctrl_check={:#04x} (want 0x96): {}",
        cmd2_opcode, cmd2_ctrl, if cmd2_ctrl == 0x96 { "CONSISTENT" } else { "INCONSISTENT" });
    println!();
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

fn advance_seed_old(state: &mut u32) {
    let n = (*state & 0x1F) as u32 + 0x23;
    let mut i = 0u32;
    while i < n {
        lfsr_step(state);
        if i + 1 < n { lfsr_step(state); }
        i += 2;
    }
}

fn sigrok_step(state: &mut u32) {
    let bit = (*state ^ (*state >> 1) ^ (*state >> 21) ^ (*state >> 31)) & 1;
    *state = (*state >> 1) | (bit << 31);
}

fn sigrok_iterate(state: &mut u32) {
    if *state == 0 { *state = 1; return; }
    let max = (*state & 0x1f) + 34;
    for _ in 0..=max {
        sigrok_step(state);
    }
}
