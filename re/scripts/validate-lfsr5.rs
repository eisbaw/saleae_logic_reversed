/// Validate LFSR against pcap - with CORRECT single-step LFSR.
///
/// KEY INSIGHT from re-reading assembly at VA 0x2d99290:
/// - The main loop body (0x20-0x4d) does TWO LFSR steps per iteration
///   (optimized unrolled loop with stride-2 counter).
/// - The odd fixup (0x5b-0x74) does ONE LFSR step.
/// - Total real LFSR steps = N = (state & 0x1f) + 0x23.
/// - Each single step: new = (state >> 1) | (feedback << 31)
///   where feedback = bit0 ^ bit1 ^ bit21 ^ bit31.
///   This is IDENTICAL to sigrok's LFSR.
///
/// Our previous lfsr_step() was computing TWO steps at once, causing confusion.
///
/// Pcap (EP1 traffic after firmware load):
///   Frame 15: EP1 OUT 5 bytes: [66, ec, 30, d2, e1]  (reseed - raw, not encrypted)
///   Frame 17: EP1 OUT 3 bytes: [95, 06, b2]           (encrypted ping)
///   Frame 20: EP1 IN  1 byte:  [cd]                   (encrypted response)
///   Frame 21: EP1 OUT 2 bytes: [96, 42]               (encrypted cmd2)
///   Frame 24: EP1 IN 128 bytes: [6a, b2, e5, 0c, ...]  (response 2)
///   Frame 25: EP1 OUT 8 bytes:  [d3, 38, bf, 43, f4, 3f, 84, c2] (cmd3)
///   Frame 28: EP1 IN  8 bytes:  [09, a1, 9f, 5c, 4d, 29, 0b, 31] (response 3)
///
/// SetControlByte (from assembly at VA 0x2d987b0):
///   - output[0] = (opcode & 0xD7) [optionally | 0x20 for fw_dl, | 0x08 for reseed]
///   - output[1..N+1] = source[0..N]  (data shifted right by 1 byte)
///
/// XorEncrypt: wire[i] = plain[i] ^ seed_byte[i % 4]
///   seed_byte[0] = state & 0xFF, [1] = (state >> 8) & 0xFF, etc.
///
/// For ping [0x7D, 0x55]:
///   SetControlByte -> [0x55, 0x7D, 0x55]
///   wire = [0x55^sb0, 0x7D^sb1, 0x55^sb2] = [0x95, 0x06, 0xB2]
///   => sb0=0xC0, sb1=0x7B, sb2=0xE7

fn main() {
    let reseed = [0x66u8, 0xec, 0x30, 0xd2, 0xe1];

    let required_sb0: u8 = 0xC0;
    let required_sb1: u8 = 0x7B;
    let required_sb2: u8 = 0xE7;

    // Required response: 0xAA ^ sb0' = 0xCD => sb0' = 0x67
    let required_resp_sb0: u8 = 0x67;

    println!("Required ping state: sb0={:#04x}, sb1={:#04x}, sb2={:#04x}",
        required_sb0, required_sb1, required_sb2);
    println!("Required resp state: sb0'={:#04x}", required_resp_sb0);
    println!();

    // All reasonable seed derivations from the 5 reseed bytes
    let seed_candidates: Vec<(u32, &str)> = vec![
        // 4-byte subsets as LE
        (u32::from_le_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] LE"),
        (u32::from_le_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] LE"),
        // 4-byte subsets as BE
        (u32::from_be_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] BE"),
        (u32::from_be_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] BE"),
        // Maybe the first byte is XOR'd with 0x66 as a command byte, seed = next 4
        // Let's include raw 4-byte seed as various interpretations
        (u32::from_le_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "seed=bytes[1..5] LE"),
        // Maybe seed bytes are stored differently
        (u32::from_le_bytes([reseed[4], reseed[3], reseed[2], reseed[1]]), "bytes[4..1] LE (reversed)"),
        (u32::from_le_bytes([reseed[3], reseed[2], reseed[1], reseed[0]]), "bytes[3..0] LE (reversed)"),
    ];

    println!("Seed candidates:");
    for (seed, desc) in &seed_candidates {
        println!("  {:#010x} = {}", seed, desc);
    }
    println!();

    // Test 1: Direct match from seed candidates with 0-5 advances
    println!("=== Test 1: Direct seed candidate -> advance -> check ===");
    let mut found_direct = false;
    for (seed, desc) in &seed_candidates {
        for advances in 0..=5u32 {
            let mut state = *seed;
            for _ in 0..advances {
                advance_seed(&mut state);
            }
            let sb0 = (state & 0xFF) as u8;
            let sb1 = ((state >> 8) & 0xFF) as u8;
            let sb2 = ((state >> 16) & 0xFF) as u8;

            if sb0 == required_sb0 && sb1 == required_sb1 && sb2 == required_sb2 {
                found_direct = true;
                check_full_sequence(state, desc, *seed, advances);
            }
        }
    }
    if !found_direct { println!("  No direct matches found."); }
    println!();

    // Test 2: Brute force all sb3, check ping + response constraint
    println!("=== Test 2: Brute force sb3 with correct advance ===");
    let mut found_states: Vec<u32> = Vec::new();
    for sb3 in 0..=255u32 {
        let state = (required_sb0 as u32) | ((required_sb1 as u32) << 8)
                  | ((required_sb2 as u32) << 16) | (sb3 << 24);
        let mut resp_state = state;
        advance_seed(&mut resp_state);
        let resp_sb0 = (resp_state & 0xFF) as u8;
        if resp_sb0 == required_resp_sb0 {
            found_states.push(state);
        }
    }
    println!("  {} states with correct ping + response", found_states.len());

    // Check reachability from seed candidates (up to 20 advances)
    for target in &found_states {
        for (seed, desc) in &seed_candidates {
            for steps in 0..=20u32 {
                let mut state = *seed;
                for _ in 0..steps {
                    advance_seed(&mut state);
                }
                if state == *target {
                    println!("  REACHABLE: state={:#010x} via {} + {} advance(s)", target, desc, steps);
                    check_full_sequence(*target, desc, *seed, steps);
                }
            }
        }
    }
    println!();

    // Test 3: What if the seed IS the state directly (no advance before first command)?
    // i.e., after reseed, the device stores the seed and the first encrypt uses it directly
    println!("=== Test 3: Seed as initial state (no advance before first use) ===");
    for (seed, desc) in &seed_candidates {
        let sb0 = (*seed & 0xFF) as u8;
        let sb1 = ((*seed >> 8) & 0xFF) as u8;
        let sb2 = ((*seed >> 16) & 0xFF) as u8;
        if sb0 == required_sb0 && sb1 == required_sb1 && sb2 == required_sb2 {
            println!("  DIRECT MATCH: {} = {:#010x}", desc, seed);
            check_full_sequence(*seed, desc, *seed, 0);
        }
    }
    println!();

    // Test 4: Maybe the advance happens with the SEED value, not current state
    // i.e., N is computed from the initial seed, not the running state
    println!("=== Test 4: Single LFSR steps from each seed (0..100 steps) ===");
    for (seed, desc) in &seed_candidates {
        let mut state = *seed;
        for step in 0..100u32 {
            let sb0 = (state & 0xFF) as u8;
            let sb1 = ((state >> 8) & 0xFF) as u8;
            let sb2 = ((state >> 16) & 0xFF) as u8;
            if sb0 == required_sb0 && sb1 == required_sb1 && sb2 == required_sb2 {
                println!("  MATCH at {} + {} raw steps, state={:#010x}", desc, step, state);
                check_full_sequence(state, desc, *seed, step);
            }
            lfsr_step(&mut state);
        }
    }
    println!();

    // Test 5: Extended single-step search (up to 10000 steps)
    println!("=== Test 5: Extended single-step search (up to 10000 steps) ===");
    for (seed, desc) in &seed_candidates {
        let mut state = *seed;
        for step in 0..10000u32 {
            let sb0 = (state & 0xFF) as u8;
            let sb1 = ((state >> 8) & 0xFF) as u8;
            let sb2 = ((state >> 16) & 0xFF) as u8;
            if sb0 == required_sb0 && sb1 == required_sb1 && sb2 == required_sb2 {
                // Also check response
                let mut resp_state = state;
                advance_seed(&mut resp_state);
                let resp_sb0 = (resp_state & 0xFF) as u8;
                if resp_sb0 == required_resp_sb0 {
                    println!("  MATCH+RESP at {} + {} raw steps, state={:#010x}", desc, step, state);
                    check_full_sequence(state, desc, *seed, step);
                }
            }
            lfsr_step(&mut state);
        }
    }
}

fn lfsr_step(state: &mut u32) {
    let s = *state;
    let feedback = (s ^ (s >> 1) ^ (s >> 21) ^ (s >> 31)) & 1;
    *state = (s >> 1) | (feedback << 31);
}

fn advance_seed(state: &mut u32) {
    let n = (*state & 0x1F) as u32 + 0x23;
    for _ in 0..n {
        lfsr_step(state);
    }
}

fn check_full_sequence(state: u32, desc: &str, seed: u32, steps: u32) {
    let sb0 = (state & 0xFF) as u8;
    let sb1 = ((state >> 8) & 0xFF) as u8;
    let sb2 = ((state >> 16) & 0xFF) as u8;
    let sb3 = ((state >> 24) & 0xFF) as u8;
    println!("  *** MATCH {} seed={:#010x} +{} steps, state={:#010x}",
        desc, seed, steps, state);
    println!("      ping: sb=[{:#04x}, {:#04x}, {:#04x}, {:#04x}]", sb0, sb1, sb2, sb3);

    // Verify ping: SetControlByte([0x7D, 0x55]) -> [0x55, 0x7D, 0x55]
    // XOR: [0x55^sb0, 0x7D^sb1, 0x55^sb2] should = [0x95, 0x06, 0xB2]
    let ping_ok = (0x55 ^ sb0 == 0x95) && (0x7D ^ sb1 == 0x06) && (0x55 ^ sb2 == 0xB2);
    println!("      ping wire: [{:#04x}, {:#04x}, {:#04x}] (want [0x95, 0x06, 0xB2]): {}",
        0x55 ^ sb0, 0x7D ^ sb1, 0x55 ^ sb2, if ping_ok { "OK" } else { "WRONG" });

    // Response: advance once, decrypt 0xCD
    let mut resp_state = state;
    advance_seed(&mut resp_state);
    let r_sb0 = (resp_state & 0xFF) as u8;
    let resp_plain = 0xCD ^ r_sb0;
    println!("      resp: 0xCD ^ {:#04x} = {:#04x} (want 0xAA): {}",
        r_sb0, resp_plain, if resp_plain == 0xAA { "OK" } else { "WRONG" });

    // cmd2: advance again, wire = [0x96, 0x42]
    let mut cmd2_state = resp_state;
    advance_seed(&mut cmd2_state);
    let c2_sb0 = (cmd2_state & 0xFF) as u8;
    let c2_sb1 = ((cmd2_state >> 8) & 0xFF) as u8;
    // SetControlByte: [ctrl, opcode] where ctrl = opcode & 0xD7
    // wire[0] = ctrl ^ c2_sb0 = 0x96
    // wire[1] = opcode ^ c2_sb1 = 0x42
    // opcode = 0x42 ^ c2_sb1
    let cmd2_opcode = 0x42 ^ c2_sb1;
    let cmd2_ctrl_expected = cmd2_opcode & 0xD7;
    let cmd2_ctrl_actual = 0x96 ^ c2_sb0;
    let cmd2_ok = cmd2_ctrl_expected == cmd2_ctrl_actual;
    println!("      cmd2: opcode={:#04x}, ctrl_actual={:#04x}, ctrl_expected={:#04x}: {}",
        cmd2_opcode, cmd2_ctrl_actual, cmd2_ctrl_expected,
        if cmd2_ok { "CONSISTENT" } else { "INCONSISTENT" });

    // resp2: advance once, decrypt 128-byte response starting [0x6a, 0xb2, 0xe5, 0x0c, ...]
    let mut resp2_state = cmd2_state;
    advance_seed(&mut resp2_state);
    let r2_sb = [
        (resp2_state & 0xFF) as u8,
        ((resp2_state >> 8) & 0xFF) as u8,
        ((resp2_state >> 16) & 0xFF) as u8,
        ((resp2_state >> 24) & 0xFF) as u8,
    ];
    let resp2_wire = [0x6au8, 0xb2, 0xe5, 0x0c];
    let resp2_plain: Vec<u8> = resp2_wire.iter().enumerate()
        .map(|(i, &w)| w ^ r2_sb[i % 4]).collect();
    println!("      resp2 first 4 plain: [{:#04x}, {:#04x}, {:#04x}, {:#04x}]",
        resp2_plain[0], resp2_plain[1], resp2_plain[2], resp2_plain[3]);

    // cmd3: advance, wire = [d3, 38, bf, 43, f4, 3f, 84, c2]
    let mut cmd3_state = resp2_state;
    advance_seed(&mut cmd3_state);
    let c3_sb = [
        (cmd3_state & 0xFF) as u8,
        ((cmd3_state >> 8) & 0xFF) as u8,
        ((cmd3_state >> 16) & 0xFF) as u8,
        ((cmd3_state >> 24) & 0xFF) as u8,
    ];
    let cmd3_wire = [0xd3u8, 0x38, 0xbf, 0x43, 0xf4, 0x3f, 0x84, 0xc2];
    let cmd3_plain: Vec<u8> = cmd3_wire.iter().enumerate()
        .map(|(i, &w)| w ^ c3_sb[i % 4]).collect();
    // cmd3 plain[0] should be ctrl byte, plain[1] should be opcode
    let c3_opcode = cmd3_plain[1];
    let c3_ctrl_expected = c3_opcode & 0xD7;
    let c3_ctrl_ok = cmd3_plain[0] == c3_ctrl_expected;
    println!("      cmd3 plain: [{:#04x}, {:#04x}, {:#04x}, {:#04x}, {:#04x}, {:#04x}, {:#04x}, {:#04x}] ctrl_ok={}",
        cmd3_plain[0], cmd3_plain[1], cmd3_plain[2], cmd3_plain[3],
        cmd3_plain[4], cmd3_plain[5], cmd3_plain[6], cmd3_plain[7],
        c3_ctrl_ok);
    println!();
}
