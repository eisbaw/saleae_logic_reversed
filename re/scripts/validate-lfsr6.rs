/// Validate LFSR against pcap - exhaustive XOR offset search.
///
/// From pcap:
///   ping plain (after SetControlByte): [0x55, 0x7D, 0x55]
///   ping wire: [0x95, 0x06, 0xB2]
///   XOR key bytes: [0xC0, 0x7B, 0xE7]
///
/// The LFSR state is a u32 with bytes [sb0, sb1, sb2, sb3].
/// The XOR key might start at any of the 4 byte offsets.
/// Try all 4 offsets × all sb unknown byte values.
///
/// Also try different plaintext models:
///   Model A: SetControlByte inserts ctrl byte, plain = [ctrl, opcode, data...]
///   Model B: No control byte, plain = [opcode, data...]
///   Model C: Control byte separately encrypted, data from next offset

fn main() {
    let reseed = [0x66u8, 0xec, 0x30, 0xd2, 0xe1];
    let ping_wire = [0x95u8, 0x06, 0xb2];
    let resp_wire = [0xCDu8];
    let cmd2_wire = [0x96u8, 0x42];
    let resp2_wire_first4 = [0x6au8, 0xb2, 0xe5, 0x0c];
    let cmd3_wire = [0xd3u8, 0x38, 0xbf, 0x43, 0xf4, 0x3f, 0x84, 0xc2];

    let ping_plain_expected = 0xAA; // Response to ping is complement of 0x55

    // Model A: SetControlByte gives [0x55, 0x7D, 0x55]
    // Model B: No control byte: [0x7D, 0x55]
    let models: Vec<(&str, Vec<u8>)> = vec![
        ("A: ctrl+data [0x55,0x7D,0x55]", vec![0x55, 0x7D, 0x55]),
        ("B: no-ctrl [0x7D,0x55]", vec![0x7D, 0x55]),
    ];

    // Seed candidates
    let seed_candidates: Vec<(u32, &str)> = vec![
        (u32::from_le_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] LE"),
        (u32::from_le_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] LE"),
        (u32::from_be_bytes([reseed[0], reseed[1], reseed[2], reseed[3]]), "bytes[0..4] BE"),
        (u32::from_be_bytes([reseed[1], reseed[2], reseed[3], reseed[4]]), "bytes[1..5] BE"),
    ];

    for (model_name, plain) in &models {
        println!("=== Model: {} ===", model_name);

        // XOR key bytes
        let xor_key: Vec<u8> = plain.iter().zip(ping_wire.iter())
            .map(|(&p, &w)| p ^ w)
            .collect();
        println!("  XOR key: {:02x?}", xor_key);

        // Try all 4 XOR phase offsets
        for phase in 0..4u8 {
            // The XOR key bytes correspond to LFSR bytes at (phase+0)%4, (phase+1)%4, (phase+2)%4
            // We know len(xor_key) bytes of the LFSR state
            // The unknown byte(s) need to be brute-forced

            // Construct partial state knowledge
            let mut known_bytes = [None::<u8>; 4];
            for (i, &kb) in xor_key.iter().enumerate() {
                known_bytes[((phase as usize) + i) % 4] = Some(kb);
            }

            println!("  Phase {}: LFSR bytes = [{}, {}, {}, {}]", phase,
                fmt_byte(known_bytes[0]), fmt_byte(known_bytes[1]),
                fmt_byte(known_bytes[2]), fmt_byte(known_bytes[3]));

            // Find which byte is unknown
            let unknown_positions: Vec<usize> = (0..4)
                .filter(|i| known_bytes[*i].is_none())
                .collect();

            if unknown_positions.is_empty() {
                // All 4 bytes known (ping has 4+ bytes)
                let state = build_state(&known_bytes);
                test_state(state, phase, &seed_candidates, model_name, &cmd2_wire, &resp2_wire_first4, &cmd3_wire);
            } else {
                // Brute force unknown byte(s)
                let mut match_count = 0;
                for unknown_val in 0..=255u8 {
                    let mut bytes = known_bytes;
                    for &pos in &unknown_positions {
                        bytes[pos] = Some(unknown_val);
                    }
                    let state = build_state(&bytes);

                    // Check response constraint
                    let mut resp_state = state;
                    advance_seed(&mut resp_state);

                    // Response XOR byte depends on phase for the response
                    // Response is 1 byte, XOR'd with the first key byte at the appropriate phase
                    // For simplicity, try resp_sb0 (the response uses sb0)
                    let resp_sb0 = (resp_state & 0xFF) as u8;
                    let resp_plain = resp_wire[0] ^ resp_sb0;

                    if resp_plain == 0xAA {
                        match_count += 1;
                        if match_count <= 3 {
                            test_state(state, phase, &seed_candidates, model_name, &cmd2_wire, &resp2_wire_first4, &cmd3_wire);
                        }
                    }
                }
                if match_count > 3 {
                    println!("    ... and {} more matches", match_count - 3);
                }
                if match_count == 0 {
                    // Try response at sb1
                    let mut match_count2 = 0;
                    for unknown_val in 0..=255u8 {
                        let mut bytes = known_bytes;
                        for &pos in &unknown_positions {
                            bytes[pos] = Some(unknown_val);
                        }
                        let state = build_state(&bytes);
                        let mut resp_state = state;
                        advance_seed(&mut resp_state);
                        let resp_sb1 = ((resp_state >> 8) & 0xFF) as u8;
                        if resp_wire[0] ^ resp_sb1 == 0xAA {
                            match_count2 += 1;
                            if match_count2 <= 2 {
                                println!("    RESP@sb1 match: state={:#010x}", state);
                            }
                        }
                    }
                    if match_count2 > 0 {
                        println!("    {} matches with resp@sb1", match_count2);
                    }
                }
            }
        }
        println!();
    }

    // Model C: XOR all bytes (including control byte) with cycling [sb0,sb1,sb2,sb3]
    // i.e. no separate control byte treatment
    println!("=== Model C: full-buffer XOR ===");
    // ping plain = [0x55, 0x7D, 0x55] (with control byte)
    // wire = [0x95, 0x06, 0xB2]
    // XOR = [0xC0, 0x7B, 0xE7]
    // Try: these ARE [sb0, sb1, sb2] with sb3 unknown
    for sb3 in 0..=255u8 {
        let state = 0xC0u32 | (0x7Bu32 << 8) | (0xE7u32 << 16) | ((sb3 as u32) << 24);
        let mut resp_state = state;
        advance_seed(&mut resp_state);
        let resp_sb0 = (resp_state & 0xFF) as u8;
        if resp_wire[0] ^ resp_sb0 == 0xAA {
            println!("  Full XOR match: state={:#010x} (sb3={:#04x})", state, sb3);
            test_reachability(state, &seed_candidates);
            test_cmd2_full(state, &cmd2_wire, &resp2_wire_first4, &cmd3_wire);
        }
    }
    println!();

    // Also try: what if the XOR doesn't use the LFSR state directly,
    // but the state bytes are reordered? (e.g., big-endian byte extraction)
    println!("=== Model D: BE byte extraction ===");
    // sb0 = (state >> 24) & 0xFF, sb1 = (state >> 16) & 0xFF, etc.
    for sb3 in 0..=255u8 {
        let state = (0xC0u32 << 24) | (0x7Bu32 << 16) | (0xE7u32 << 8) | (sb3 as u32);
        let mut resp_state = state;
        advance_seed(&mut resp_state);
        // If response uses BE byte extraction too:
        let resp_sb0 = ((resp_state >> 24) & 0xFF) as u8;
        if resp_wire[0] ^ resp_sb0 == 0xAA {
            println!("  BE match: state={:#010x} (sb3={:#04x})", state, sb3);
            test_reachability(state, &seed_candidates);
        }
    }
    // Also try resp at LE sb0
    for sb3 in 0..=255u8 {
        let state = (0xC0u32 << 24) | (0x7Bu32 << 16) | (0xE7u32 << 8) | (sb3 as u32);
        let mut resp_state = state;
        advance_seed(&mut resp_state);
        let resp_sb0 = (resp_state & 0xFF) as u8;
        if resp_wire[0] ^ resp_sb0 == 0xAA {
            println!("  BE/LE-resp match: state={:#010x}", state);
            test_reachability(state, &seed_candidates);
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

fn build_state(bytes: &[Option<u8>; 4]) -> u32 {
    (bytes[0].unwrap() as u32)
        | ((bytes[1].unwrap() as u32) << 8)
        | ((bytes[2].unwrap() as u32) << 16)
        | ((bytes[3].unwrap() as u32) << 24)
}

fn fmt_byte(b: Option<u8>) -> String {
    match b {
        Some(v) => format!("{:#04x}", v),
        None => "??".to_string(),
    }
}

fn test_reachability(target: u32, seed_candidates: &[(u32, &str)]) {
    for (seed, desc) in seed_candidates {
        let mut state = *seed;
        for steps in 0..=20u32 {
            if state == target {
                println!("    REACHABLE: via {} + {} advance(s)", desc, steps);
            }
            advance_seed(&mut state);
        }
        // Also try raw LFSR steps
        let mut state2 = *seed;
        for step in 0..200u32 {
            if state2 == target {
                println!("    REACHABLE: via {} + {} raw lfsr step(s)", desc, step);
            }
            lfsr_step(&mut state2);
        }
    }
}

fn test_state(state: u32, phase: u8, seed_candidates: &[(u32, &str)], model_name: &str, cmd2_wire: &[u8], resp2_first4: &[u8], cmd3_wire: &[u8]) {
    println!("    Match: state={:#010x} (phase {})", state, phase);

    // Check reachability
    test_reachability(state, seed_candidates);

    // Check cmd2 consistency
    let mut resp_state = state;
    advance_seed(&mut resp_state);
    let mut cmd2_state = resp_state;
    advance_seed(&mut cmd2_state);

    let c2_sb0 = (cmd2_state & 0xFF) as u8;
    let c2_sb1 = ((cmd2_state >> 8) & 0xFF) as u8;

    // cmd2 is 2 bytes on wire: [0x96, 0x42]
    // If model A (with control byte): plain = [ctrl, opcode], wire = [ctrl^key0, opcode^key1]
    // For full XOR: key0 = c2_sb0, key1 = c2_sb1
    let c2_plain0 = cmd2_wire[0] ^ c2_sb0;
    let c2_plain1 = cmd2_wire[1] ^ c2_sb1;
    // Check if plain[0] = plain[1] & 0xD7 (control byte consistency)
    let ctrl_ok = c2_plain0 == (c2_plain1 & 0xD7);
    println!("      cmd2: plain=[{:#04x},{:#04x}], ctrl_ok={}", c2_plain0, c2_plain1, ctrl_ok);
}

fn test_cmd2_full(state: u32, cmd2_wire: &[u8], resp2_first4: &[u8], cmd3_wire: &[u8]) {
    let mut resp_state = state;
    advance_seed(&mut resp_state);
    let mut cmd2_state = resp_state;
    advance_seed(&mut cmd2_state);

    let c2_sb = [
        (cmd2_state & 0xFF) as u8,
        ((cmd2_state >> 8) & 0xFF) as u8,
    ];
    let c2_plain: Vec<u8> = cmd2_wire.iter().enumerate()
        .map(|(i, &w)| w ^ c2_sb[i % 2])
        .collect();
    let ctrl_ok = c2_plain[0] == (c2_plain[1] & 0xD7);
    println!("    cmd2: plain=[{:#04x},{:#04x}], ctrl_ok={}", c2_plain[0], c2_plain[1], ctrl_ok);

    // resp2
    let mut resp2_state = cmd2_state;
    advance_seed(&mut resp2_state);
    let r2_sb = [
        (resp2_state & 0xFF) as u8,
        ((resp2_state >> 8) & 0xFF) as u8,
        ((resp2_state >> 16) & 0xFF) as u8,
        ((resp2_state >> 24) & 0xFF) as u8,
    ];
    let r2_plain: Vec<u8> = resp2_first4.iter().enumerate()
        .map(|(i, &w)| w ^ r2_sb[i % 4])
        .collect();
    println!("    resp2 first4 plain: [{:#04x},{:#04x},{:#04x},{:#04x}]",
        r2_plain[0], r2_plain[1], r2_plain[2], r2_plain[3]);

    // cmd3
    let mut cmd3_state = resp2_state;
    advance_seed(&mut cmd3_state);
    let c3_sb = [
        (cmd3_state & 0xFF) as u8,
        ((cmd3_state >> 8) & 0xFF) as u8,
        ((cmd3_state >> 16) & 0xFF) as u8,
        ((cmd3_state >> 24) & 0xFF) as u8,
    ];
    let c3_plain: Vec<u8> = cmd3_wire.iter().enumerate()
        .map(|(i, &w)| w ^ c3_sb[i % 4])
        .collect();
    let c3_ctrl_ok = c3_plain[0] == (c3_plain[1] & 0xD7);
    println!("    cmd3 plain: [{:#04x},{:#04x},{:#04x},{:#04x},{:#04x},{:#04x},{:#04x},{:#04x}] ctrl_ok={}",
        c3_plain[0], c3_plain[1], c3_plain[2], c3_plain[3],
        c3_plain[4], c3_plain[5], c3_plain[6], c3_plain[7], c3_ctrl_ok);
}
