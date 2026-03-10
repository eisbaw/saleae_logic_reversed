/// KEY INSIGHT: The reseed packet [66, ec, 30, d2, e1] is ENCRYPTED.
///
/// From ReseedDevice assembly (VA 0x2d91950):
///   1. Generate 4 random bytes (the new seed)
///   2. Call prepare_buffer(output, seed_vec, *(r12+0x60))  -- encrypt with CURRENT key
///   3. *(r12+0x64) = raw seed (the 4 random bytes)
///   4. *(r12+0x68) = 0
///   5. Send encrypted output via USB
///
/// prepare_buffer is at VA 0x2d99c10.
/// From disassembly:
///   output[0] = ((source[0] & 0xD7) | 0x20) ^ (key_byte0 & 0xD7)
///   output[i] = source[i-1] ^ key_byte[...] for i >= 1
///
/// Wait, prepare_buffer was analyzed differently from SimpleEncryption_Write.
/// Let me re-check: prepare_buffer has the |0x20 flag (firmware download),
/// and SimpleEncryption_Write has |0x08 for reseed.
///
/// Actually, the reseed uses prepare_buffer, NOT SimpleEncryption_Write.
/// And prepare_buffer uses |0x20 unconditionally.
///
/// So for the reseed packet:
///   source = [4 random seed bytes] = [s0, s1, s2, s3]
///   output[0] = ((s0 & 0xD7) | 0x20) ^ (old_sb0 & 0xD7)
///   output[1] = s0 ^ old_sb1  (or cycling key from position 1)
///   output[2] = s1 ^ old_sb2
///   output[3] = s2 ^ old_sb3
///   output[4] = s3 ^ old_sb0
///
/// Wait, but prepare_buffer might use a DIFFERENT XOR pattern.
/// Let me focus on what we KNOW works:
///   - State 0x6fe77bc8 correctly decrypts ping, cmd2, cmd3, and resp2 gives "Logi"
///   - This is the LFSR state used for the FIRST encrypted command after reseed
///   - The reseed packet is encrypted with the PREVIOUS state
///
/// The question is: what is the relationship between:
///   1. The old LFSR state (used to encrypt the reseed packet)
///   2. The new seed (4 random bytes inside the encrypted reseed)
///   3. The state 0x6fe77bc8 (used for the first command after reseed)
///
/// From ReseedDevice:
///   - Old state is at device+0x60
///   - New seed stored at device+0x64
///   - Flag at device+0x68 = 0
///
/// Maybe when the flag is 0, the next encryption call copies +0x64 to +0x60
/// (sets state = seed) before encrypting. Then advance_seed is called at the end.
///
/// But we can work backward! If the ping state is 0x6fe77bc8, we need to find
/// what state, when passed through advance_seed, gives 0x6fe77bc8.
/// Because prepare_buffer calls advance_seed at the end, after encrypting the reseed.
///
/// Wait, actually: prepare_buffer IS called for the reseed.
/// After prepare_buffer returns, the LFSR state has been advanced.
/// Then the raw seed is stored at +0x64.
/// Then *(+0x68) = 0.
///
/// For the NEXT command (ping), if the code checks +0x68 and copies +0x64 to +0x60:
///   state = seed (copied from +0x64)
///   This state IS the initial state for ping encryption.
///   No advance_seed between reseed and ping (that's done at END of ping encryption).
///
/// So: state for ping = raw seed bytes (as u32).
/// But we showed that no permutation of [ec, 30, d2, e1] matches 0x6fe77bc8.
///
/// Unless the raw seed is NOT [ec, 30, d2, e1]. The pcap shows the ENCRYPTED
/// reseed packet. We need to decrypt it to get the raw seed.
///
/// To decrypt, we need the OLD LFSR state. But we don't know that either.
///
/// However: if after reseed, state = raw_seed, and state_for_ping = 0x6fe77bc8,
/// then raw_seed = 0x6fe77bc8. And the encrypted reseed can tell us the old state.
///
/// Let's test: assuming the raw seed IS 0x6fe77bc8 = [c8, 7b, e7, 6f] LE.
/// The reseed packet source = [c8, 7b, e7, 6f] (the 4 seed bytes).
///
/// prepare_buffer produces 5-byte output from 4-byte source:
///   output[0] = ((c8 & 0xD7) | 0x20) ^ (old_sb0 & 0xD7)
///   output[1] = c8 ^ old_sb(pos1)
///   output[2] = 7b ^ old_sb(pos2)
///   output[3] = e7 ^ old_sb(pos3)
///   output[4] = 6f ^ old_sb(pos0)  [cycling back]
///
/// From pcap: output = [66, ec, 30, d2, e1]
///
/// Let me figure out the XOR pattern in prepare_buffer.
/// In SimpleEncryption_Write:
///   output[1] uses sb1, output[2] uses sb2, etc. (starts at sb1)
///
/// But prepare_buffer might be different. Let me check.

fn main() {
    let target_state: u32 = 0x6fe77bc8;
    let reseed_wire = [0x66u8, 0xec, 0x30, 0xd2, 0xe1];

    // If the raw seed (stored as the new LFSR state) IS 0x6fe77bc8 = [c8, 7b, e7, 6f] LE
    let seed_bytes_le = target_state.to_le_bytes(); // [c8, 7b, e7, 6f]
    println!("Assuming raw seed = {:#010x} = {:02x?}", target_state, seed_bytes_le);

    // prepare_buffer: output[0] = ((seed[0] & 0xD7) | 0x20) ^ (old_sb0 & 0xD7)
    // For seed[0] = 0xc8: 0xc8 & 0xD7 = 0xc0; | 0x20 = 0xe0
    let ctrl_plain = (seed_bytes_le[0] & 0xD7) | 0x20;
    println!("ctrl_plain = {:#04x}", ctrl_plain);  // 0xe0

    // output[0] = 0xe0 ^ (old_sb0 & 0xD7) = 0x66
    // old_sb0 & 0xD7 = 0xe0 ^ 0x66 = 0x86
    let old_sb0_d7 = ctrl_plain ^ reseed_wire[0];
    println!("old_sb0 & 0xD7 = {:#04x}", old_sb0_d7);  // 0x86

    // Now for the data bytes, assuming SimpleEncryption_Write-style XOR:
    // output[1] = seed[0] ^ old_sb1
    // output[2] = seed[1] ^ old_sb2
    // output[3] = seed[2] ^ old_sb3
    // output[4] = seed[3] ^ old_sb0
    let old_sb1 = seed_bytes_le[0] ^ reseed_wire[1]; // c8 ^ ec
    let old_sb2 = seed_bytes_le[1] ^ reseed_wire[2]; // 7b ^ 30
    let old_sb3 = seed_bytes_le[2] ^ reseed_wire[3]; // e7 ^ d2
    let old_sb0_from_data = seed_bytes_le[3] ^ reseed_wire[4]; // 6f ^ e1

    println!("old_sb1 = {:#04x}", old_sb1);  // 0x24
    println!("old_sb2 = {:#04x}", old_sb2);  // 0x4b
    println!("old_sb3 = {:#04x}", old_sb3);  // 0x35
    println!("old_sb0 (from data) = {:#04x}", old_sb0_from_data);  // 0x8e

    // Check: old_sb0_from_data & 0xD7 should = old_sb0_d7
    let consistent = (old_sb0_from_data & 0xD7) == old_sb0_d7;
    println!("Consistency check: {:#04x} & 0xD7 = {:#04x} (want {:#04x}): {}",
        old_sb0_from_data, old_sb0_from_data & 0xD7, old_sb0_d7,
        if consistent { "OK" } else { "FAIL" });

    if consistent {
        let old_state = (old_sb0_from_data as u32) | ((old_sb1 as u32) << 8)
            | ((old_sb2 as u32) << 16) | ((old_sb3 as u32) << 24);
        println!("Old LFSR state = {:#010x}", old_state);

        // After prepare_buffer encrypts the reseed, advance_seed is called
        let mut after_advance = old_state;
        advance_seed(&mut after_advance);
        println!("Old state after advance_seed = {:#010x}", after_advance);

        // The new state for ping should be the raw seed = target_state
        // This is independent of the advance_seed result, because the reseed
        // stores the seed at +0x64 and it gets loaded to +0x60 before ping.

        // Let me also check: what if the first command (this reseed) has NO
        // prior encryption state (device just booted, state = 0)?
        println!();
        println!("What if initial state is 0x00000000?");
        let init_state = 0u32;
        // output[0] = ((c8 & 0xD7) | 0x20) ^ (0 & 0xD7) = 0xe0 ^ 0 = 0xe0
        println!("  output[0] would be {:#04x} (want 0x66): {}",
            ctrl_plain ^ (init_state as u8 & 0xD7),
            if (ctrl_plain ^ (init_state as u8 & 0xD7)) == reseed_wire[0] { "OK" } else { "FAIL" });

        println!();
        println!("What if initial state is 0x00000001?");
        let init_state = 1u32;
        let init_sb0 = (init_state & 0xFF) as u8;
        println!("  output[0] would be {:#04x} (want 0x66): {}",
            ctrl_plain ^ (init_sb0 & 0xD7),
            if (ctrl_plain ^ (init_sb0 & 0xD7)) == reseed_wire[0] { "OK" } else { "FAIL" });
    }

    println!();
    println!("=== Now try: maybe prepare_buffer uses different XOR pattern ===");
    println!("=== Maybe data XOR starts at sb0, not sb1 ===");

    // If prepare_buffer XORs output[1..] starting with sb0 (not sb1):
    // output[1] = seed[0] ^ old_sb0
    // output[2] = seed[1] ^ old_sb1
    // output[3] = seed[2] ^ old_sb2
    // output[4] = seed[3] ^ old_sb3
    let old_sb0_v2 = seed_bytes_le[0] ^ reseed_wire[1]; // c8 ^ ec = 0x24
    let old_sb1_v2 = seed_bytes_le[1] ^ reseed_wire[2]; // 7b ^ 30 = 0x4b
    let old_sb2_v2 = seed_bytes_le[2] ^ reseed_wire[3]; // e7 ^ d2 = 0x35
    let old_sb3_v2 = seed_bytes_le[3] ^ reseed_wire[4]; // 6f ^ e1 = 0x8e

    println!("With sb0-start: old state = [{:#04x}, {:#04x}, {:#04x}, {:#04x}]",
        old_sb0_v2, old_sb1_v2, old_sb2_v2, old_sb3_v2);

    // Check ctrl consistency
    let ctrl_consistent_v2 = (old_sb0_v2 & 0xD7) == (ctrl_plain ^ reseed_wire[0]);
    println!("Ctrl consistency: ({:#04x} & 0xD7 = {:#04x}) vs ({:#04x}): {}",
        old_sb0_v2, old_sb0_v2 & 0xD7, ctrl_plain ^ reseed_wire[0],
        if ctrl_consistent_v2 { "OK" } else { "FAIL" });

    // old_sb0_v2 & 0xD7 = 0x24 & 0xD7 = 0x04
    // ctrl_plain ^ reseed_wire[0] = 0xe0 ^ 0x66 = 0x86
    // 0x04 != 0x86: FAIL

    println!();
    println!("=== Maybe raw seed is BE: 0x6fe77bc8 -> [6f, e7, 7b, c8] ===");
    let seed_bytes_be = target_state.to_be_bytes(); // [6f, e7, 7b, c8]
    println!("Seed bytes BE: {:02x?}", seed_bytes_be);

    // With SimpleEncryption_Write-style XOR (data starts at sb1):
    let ctrl_plain_be = (seed_bytes_be[0] & 0xD7) | 0x20;
    println!("ctrl_plain = {:#04x}", ctrl_plain_be);  // (0x6f & 0xD7) | 0x20 = 0x47 | 0x20 = 0x67
    // 0x67 != 0xe0

    let old_sb1_be = seed_bytes_be[0] ^ reseed_wire[1]; // 6f ^ ec
    let old_sb2_be = seed_bytes_be[1] ^ reseed_wire[2]; // e7 ^ 30
    let old_sb3_be = seed_bytes_be[2] ^ reseed_wire[3]; // 7b ^ d2
    let old_sb0_be = seed_bytes_be[3] ^ reseed_wire[4]; // c8 ^ e1

    println!("old_sb0_d7 = ctrl_plain ^ wire[0] = {:#04x} ^ 0x66 = {:#04x}",
        ctrl_plain_be, ctrl_plain_be ^ reseed_wire[0]);
    println!("old_sb0 (from data) = {:#04x}, & 0xD7 = {:#04x}",
        old_sb0_be, old_sb0_be & 0xD7);

    let consistent_be = (old_sb0_be & 0xD7) == (ctrl_plain_be ^ reseed_wire[0]);
    println!("Consistency: {}", if consistent_be { "OK" } else { "FAIL" });

    println!();
    println!("=== Try: reseed uses SimpleEncryption_Write, not prepare_buffer ===");
    println!("=== With |0x08 flag instead of |0x20 ===");
    // ctrl = (seed[0] & 0xD7) | 0x08
    // For seed[0] = 0xc8: ctrl = 0xc0 | 0x08 = 0xc8
    let ctrl_reseed = (seed_bytes_le[0] & 0xD7) | 0x08;
    println!("ctrl_reseed = {:#04x}", ctrl_reseed);  // 0xc8
    // output[0] = 0xc8 ^ (old_sb0 & 0xD7) = 0x66
    // old_sb0 & 0xD7 = 0xc8 ^ 0x66 = 0xae
    let old_sb0_d7_v3 = ctrl_reseed ^ reseed_wire[0];
    println!("old_sb0 & 0xD7 = {:#04x}", old_sb0_d7_v3);

    // Same data XOR:
    let old_sb1_v3 = seed_bytes_le[0] ^ reseed_wire[1]; // 0x24
    let old_sb2_v3 = seed_bytes_le[1] ^ reseed_wire[2]; // 0x4b
    let old_sb3_v3 = seed_bytes_le[2] ^ reseed_wire[3]; // 0x35
    let old_sb0_v3 = seed_bytes_le[3] ^ reseed_wire[4]; // 0x8e
    let consist_v3 = (old_sb0_v3 & 0xD7) == old_sb0_d7_v3;
    println!("old_sb0 from data = {:#04x}, & 0xD7 = {:#04x} (want {:#04x}): {}",
        old_sb0_v3, old_sb0_v3 & 0xD7, old_sb0_d7_v3,
        if consist_v3 { "OK" } else { "FAIL" });
    // 0x8e & 0xD7 = 0x86, want 0xae: FAIL

    println!();
    println!("=== Try: NO control byte at all, full 5-byte XOR ===");
    println!("=== (reseed has no SetControlByte, just raw XOR) ===");
    // wire[i] = seed[i] ^ old_sb[i % 4]
    // wire = [66, ec, 30, d2, e1]
    // seed = target.to_le_bytes() padded = [c8, 7b, e7, 6f, ??]
    // Hmm, but then source is 5 bytes. The seed is only 4 bytes.
    // Maybe the command structure is [opcode, seed0, seed1, seed2, seed3]
    // where opcode indicates "reseed".
    // Before encryption: [reseed_opcode, s0, s1, s2, s3]
    // After full XOR: wire[i] = plain[i] ^ sb[i % 4]
    //   wire[0] = opcode ^ sb0 = 0x66
    //   wire[1] = s0 ^ sb1 = 0xec
    //   wire[2] = s1 ^ sb2 = 0x30
    //   wire[3] = s2 ^ sb3 = 0xd2
    //   wire[4] = s3 ^ sb0 = 0xe1
    // If seed = [c8, 7b, e7, 6f]:
    //   sb1 = 0xec ^ 0xc8 = 0x24
    //   sb2 = 0x30 ^ 0x7b = 0x4b
    //   sb3 = 0xd2 ^ 0xe7 = 0x35
    //   sb0 = 0xe1 ^ 0x6f = 0x8e
    //   opcode = 0x66 ^ 0x8e = 0xe8
    // Reseed opcode might not be 0xe8. Common reseed opcode might be 0x7D or similar.
    // Actually, looking at sigrok and common patterns, maybe reseed is a special command byte.
    // Let's just check if this old state is reachable.
    let old_state_fullxor = 0x354b248eu32; // [8e, 24, 4b, 35] LE
    println!("Full XOR model: old_state = {:#010x}", old_state_fullxor);
    println!("  Implies reseed opcode = {:#04x}", reseed_wire[0] ^ 0x8e);

    // Maybe the reseed is NOT encrypted at all!
    // Frame 15 is the FIRST EP1 OUT command. If the device just booted,
    // there's no LFSR state yet. The first command must establish the LFSR.
    // So maybe the reseed is sent UNENCRYPTED.
    println!();
    println!("=== CRITICAL: Maybe reseed is NOT encrypted ===");
    println!("Frame 15 is the FIRST EP1 command. No prior LFSR state exists.");
    println!("The reseed must establish the initial LFSR state.");
    println!();

    // If unencrypted: [66, ec, 30, d2, e1] is the raw command.
    // 0x66 might be a command identifier
    // [ec, 30, d2, e1] might be the seed
    // But we showed that no derivation of these bytes gives 0x6fe77bc8.
    //
    // UNLESS: the reseed command byte is NOT 0x66 but is part of the seed too?
    // Or: maybe 0x66 is a marker and the seed is processed differently.
    //
    // What if the device processes the reseed like this:
    //   seed = first_byte XOR with some constant, combined with other bytes?
    //   Or: seed = all 5 bytes hashed/mixed somehow?
    //
    // Or what if the LFSR is seeded with the COMPLEMENT or REVERSE bit order?

    // Try bit-reversal
    fn reverse_bits_32(mut n: u32) -> u32 {
        let mut result = 0u32;
        for _ in 0..32 {
            result = (result << 1) | (n & 1);
            n >>= 1;
        }
        result
    }

    let seed_candidates_raw: Vec<(u32, &str)> = vec![
        (u32::from_le_bytes([0xec, 0x30, 0xd2, 0xe1]), "[ec,30,d2,e1] LE"),
        (u32::from_be_bytes([0xec, 0x30, 0xd2, 0xe1]), "[ec,30,d2,e1] BE"),
        (u32::from_le_bytes([0x66, 0xec, 0x30, 0xd2]), "[66,ec,30,d2] LE"),
        (u32::from_be_bytes([0x66, 0xec, 0x30, 0xd2]), "[66,ec,30,d2] BE"),
    ];

    for (seed, desc) in &seed_candidates_raw {
        let rev = reverse_bits_32(*seed);
        if rev == target_state {
            println!("  BIT-REVERSED MATCH: {} reversed = {:#010x}", desc, rev);
        }

        // XOR with 0xFFFFFFFF (complement)
        let comp = !*seed;
        if comp == target_state {
            println!("  COMPLEMENT MATCH: !{} = {:#010x}", desc, comp);
        }

        // Byte-swap
        let swapped = seed.swap_bytes();
        if swapped == target_state {
            println!("  BYTE-SWAP MATCH: {}.swap_bytes() = {:#010x}", desc, swapped);
        }
    }

    // What if the command byte 0x66 is NOT a separate opcode but encodes something?
    // 0x66 in binary: 0110_0110
    // The SetControlByte formula: ctrl = opcode & 0xD7
    // So if ctrl = 0x66, then opcode & 0xD7 = 0x66
    // 0x66 & 0xD7: check - 0x66 = 0110_0110, 0xD7 = 1101_0111
    // AND = 0100_0110 = 0x46. So 0x66 is NOT a valid control byte (0x66 & 0xD7 = 0x46 != 0x66).
    //
    // WAIT. 0x66 & 0xD7 = 0x46, not 0x66. So 0x66 cannot be produced by opcode & 0xD7.
    // This means the first byte IS affected by encryption (or it's not a control byte).

    // NEW THEORY: Maybe the device starts with LFSR state = 0,
    // and the reseed packet IS encrypted with this initial state.
    // If state = 0:
    //   XOR key = [00, 00, 00, 00]
    //   output = plain ^ [00,00,00,00] = plain
    //   So wire = plain
    //   Control byte: ((opcode & 0xD7) [| flag]) ^ (0 & 0xD7) = (opcode & 0xD7) [| flag]
    //   
    // For wire = [66, ec, 30, d2, e1]:
    //   wire[0] = ctrl = (opcode & 0xD7) | flag
    //   wire[1..5] = source[0..4] ^ 0 = source[0..4]
    //   So source = [ec, 30, d2, e1]
    //   And ctrl = 0x66
    //   Since source[0] = opcode = 0xec:
    //   ctrl = (0xec & 0xD7) | flag = 0xc4 | flag
    //   For flag=0x00: ctrl = 0xc4
    //   For flag=0x08: ctrl = 0xcc
    //   For flag=0x20: ctrl = 0xe4
    //   None equals 0x66.
    //   
    // Hmm. 0x66 still doesn't match. Unless source[0] != wire[1] when state = 0.
    // With state = 0, sb1 = 0, so output[1] = source[0] ^ 0 = source[0] = wire[1] = 0xec.
    // And ctrl = (0xec & 0xD7) = 0xc4 | flag. Not 0x66.

    // What if state = 1?
    println!();
    println!("=== Test with initial state = 1 ===");
    let init = 1u32;
    let init_sb = [
        (init & 0xFF) as u8,
        ((init >> 8) & 0xFF) as u8,
        ((init >> 16) & 0xFF) as u8,
        ((init >> 24) & 0xFF) as u8,
    ];
    println!("init state = {:#010x}, sbs = {:02x?}", init, init_sb);
    // ctrl = ((source[0] & 0xD7) | 0x20) ^ (init_sb[0] & 0xD7)
    // For any source[0], the ctrl result depends on source[0].
    // wire[0] = ctrl = ((wire[1] ^ init_sb[1]) & 0xD7 | 0x20) ^ (init_sb[0] & 0xD7)
    // With init_sb[1] = 0: source[0] = wire[1] = 0xec
    // ctrl = (0xec & 0xD7 | 0x20) ^ (1 & 0xD7) = (0xc4 | 0x20) ^ (0x01 & 0xD7)
    //       = 0xe4 ^ 0x01 = 0xe5
    // Wire[0] = 0xe5, but pcap has 0x66. Doesn't match.

    // What if the initial state is not 0 or 1, but something else entirely?
    // The device might initialize the LFSR from firmware or a hardware register.
    //
    // Let me take a different approach: BRUTE FORCE the old state.
    // We have 5 constraints from the encrypted reseed:
    //   - The raw seed must produce state 0x6fe77bc8 for the next command
    //   - The 5 wire bytes must decrypt correctly
    //   - The control byte formula must hold
    //
    // If we assume the raw seed = 0x6fe77bc8 = [c8, 7b, e7, 6f]:
    //   wire[1] = c8 ^ (old_state >> 8) & 0xFF = ec => old_sb1 = 0x24
    //   wire[2] = 7b ^ (old_state >> 16) & 0xFF = 30 => old_sb2 = 0x4b
    //   wire[3] = e7 ^ (old_state >> 24) & 0xFF = d2 => old_sb3 = 0x35
    //   wire[4] = 6f ^ (old_state >> 0) & 0xFF = e1 => old_sb0 = 0x8e
    //
    // old_state = 0x354b248e
    //
    // Now check: what ctrl byte formula gives wire[0] = 0x66?
    // With |0x20: ctrl = (c8 & 0xD7 | 0x20) ^ (0x8e & 0xD7) = 0xe0 ^ 0x86 = 0x66
    // YES! THAT WORKS!

    println!();
    println!("=== VERIFICATION: prepare_buffer with |0x20 ===");
    let raw_seed = 0x6fe77bc8u32;
    let s = raw_seed.to_le_bytes(); // [c8, 7b, e7, 6f]
    let old_state_val = 0x354b248eu32;
    let old_sb = old_state_val.to_le_bytes(); // [8e, 24, 4b, 35]

    let ctrl = ((s[0] & 0xD7) | 0x20) ^ (old_sb[0] & 0xD7);
    println!("ctrl = (({:#04x} & 0xD7) | 0x20) ^ ({:#04x} & 0xD7)", s[0], old_sb[0]);
    println!("     = ({:#04x} | 0x20) ^ {:#04x}", s[0] & 0xD7, old_sb[0] & 0xD7);
    println!("     = {:#04x} ^ {:#04x} = {:#04x} (want 0x66): {}",
        (s[0] & 0xD7) | 0x20, old_sb[0] & 0xD7, ctrl,
        if ctrl == 0x66 { "OK!!!" } else { "FAIL" });

    // Data bytes (XOR starting at sb1):
    for i in 0..4 {
        let sb_idx = ((i + 1) % 4) as usize;
        let wire_byte = s[i] ^ old_sb[sb_idx];
        let expected = reseed_wire[i + 1];
        println!("wire[{}] = s[{}] ^ old_sb[{}] = {:#04x} ^ {:#04x} = {:#04x} (want {:#04x}): {}",
            i + 1, i, sb_idx, s[i], old_sb[sb_idx], wire_byte, expected,
            if wire_byte == expected { "OK" } else { "FAIL" });
    }

    // Also check: does advance_seed(old_state) give something sensible?
    let mut advanced = old_state_val;
    advance_seed(&mut advanced);
    println!();
    println!("advance_seed({:#010x}) = {:#010x}", old_state_val, advanced);
    println!("This is the state AFTER prepare_buffer processes the reseed.");
    println!("But it gets overwritten by the new seed {:#010x} for the next command.", raw_seed);

    // FULL VALIDATION: now verify entire pcap sequence
    println!();
    println!("======= FULL PCAP VALIDATION =======");
    println!();

    let mut state = raw_seed;
    println!("Initial state (from reseed): {:#010x}", state);

    // Ping: source = [0x7D, 0x55], SetControlByte -> [0x55, 0x7D, 0x55]
    let ping_source = [0x7Du8, 0x55];
    let ping_ctrl = ping_source[0] & 0xD7; // 0x55
    let sb = state.to_le_bytes();
    let ping_wire_calc = [
        ping_ctrl ^ (sb[0] & 0xD7),
        ping_source[0] ^ sb[1],
        ping_source[1] ^ sb[2],
    ];
    let ping_wire_pcap = [0x95u8, 0x06, 0xb2];
    println!("PING:");
    println!("  state = {:#010x}, sb = {:02x?}", state, sb);
    for i in 0..3 {
        println!("  wire[{}] = {:#04x} (want {:#04x}): {}",
            i, ping_wire_calc[i], ping_wire_pcap[i],
            if ping_wire_calc[i] == ping_wire_pcap[i] { "OK" } else { "FAIL" });
    }
    advance_seed(&mut state);

    // Response: [cd], decrypt as plain[0] = wire[0] ^ sb0
    let resp_sb = state.to_le_bytes();
    let resp_plain = 0xCD ^ resp_sb[0];
    println!("RESP1:");
    println!("  state = {:#010x}, sb0 = {:#04x}", state, resp_sb[0]);
    println!("  plain = 0xCD ^ {:#04x} = {:#04x} (want 0xAA): {}",
        resp_sb[0], resp_plain, if resp_plain == 0xAA { "OK" } else { "FAIL" });
    advance_seed(&mut state);

    // cmd2: [96, 42]
    let cmd2_sb = state.to_le_bytes();
    let cmd2_opcode = 0x42 ^ cmd2_sb[1]; // source[0] ^ sb1
    let cmd2_ctrl_expected = (cmd2_opcode & 0xD7) ^ (cmd2_sb[0] & 0xD7);
    println!("CMD2:");
    println!("  state = {:#010x}, sb = {:02x?}", state, cmd2_sb);
    println!("  opcode = 0x42 ^ {:#04x} = {:#04x}", cmd2_sb[1], cmd2_opcode);
    println!("  ctrl = ({:#04x} & 0xD7) ^ ({:#04x} & 0xD7) = {:#04x} (want 0x96): {}",
        cmd2_opcode, cmd2_sb[0], cmd2_ctrl_expected,
        if cmd2_ctrl_expected == 0x96 { "OK" } else { "FAIL" });
    advance_seed(&mut state);

    // resp2: [6a, b2, e5, 0c, ...] - 128 bytes, decrypt first few
    let r2_sb = state.to_le_bytes();
    let r2_first4 = [0x6au8, 0xb2, 0xe5, 0x0c];
    let r2_plain: Vec<u8> = r2_first4.iter().enumerate()
        .map(|(i, &w)| w ^ r2_sb[i % 4]).collect();
    println!("RESP2:");
    println!("  state = {:#010x}, sb = {:02x?}", state, r2_sb);
    println!("  first 4 plain: {:02x?} = \"{}\"",
        r2_plain,
        r2_plain.iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' }).collect::<String>());
    advance_seed(&mut state);

    // cmd3: [d3, 38, bf, 43, f4, 3f, 84, c2]
    let c3_sb = state.to_le_bytes();
    let cmd3_src0 = 0x38 ^ c3_sb[1];
    let cmd3_ctrl_expected = (cmd3_src0 & 0xD7) ^ (c3_sb[0] & 0xD7);
    println!("CMD3:");
    println!("  state = {:#010x}, sb = {:02x?}", state, c3_sb);
    println!("  src0 = 0x38 ^ {:#04x} = {:#04x}", c3_sb[1], cmd3_src0);
    println!("  ctrl = ({:#04x} & 0xD7) ^ ({:#04x} & 0xD7) = {:#04x} (want 0xd3): {}",
        cmd3_src0, c3_sb[0], cmd3_ctrl_expected,
        if cmd3_ctrl_expected == 0xd3 { "OK" } else { "FAIL" });
    // Decrypt full cmd3
    let cmd3_wire = [0xd3u8, 0x38, 0xbf, 0x43, 0xf4, 0x3f, 0x84, 0xc2];
    let cmd3_plain: Vec<u8> = (1..8).map(|i| {
        let bit_off = ((i * 8) & 0x18) as u32;
        let sb_byte = ((state >> bit_off) & 0xFF) as u8;
        cmd3_wire[i] ^ sb_byte
    }).collect();
    println!("  cmd3 source: [{:#04x}, {:#04x}, {:#04x}, {:#04x}, {:#04x}, {:#04x}, {:#04x}]",
        cmd3_plain[0], cmd3_plain[1], cmd3_plain[2], cmd3_plain[3],
        cmd3_plain[4], cmd3_plain[5], cmd3_plain[6]);
    advance_seed(&mut state);

    // resp3: [09, a1, 9f, 5c, 4d, 29, 0b, 31]
    let r3_sb = state.to_le_bytes();
    let resp3_wire = [0x09u8, 0xa1, 0x9f, 0x5c, 0x4d, 0x29, 0x0b, 0x31];
    let r3_plain: Vec<u8> = resp3_wire.iter().enumerate()
        .map(|(i, &w)| w ^ r3_sb[i % 4]).collect();
    println!("RESP3:");
    println!("  state = {:#010x}, sb = {:02x?}", state, r3_sb);
    println!("  plain: {:02x?}", r3_plain);
    advance_seed(&mut state);

    // cmd4: [45, 2d, 4a, 6b, 62, 2a, 7f, ea]
    let c4_sb = state.to_le_bytes();
    let cmd4_wire = [0x45u8, 0x2d, 0x4a, 0x6b, 0x62, 0x2a, 0x7f, 0xea];
    let cmd4_src0 = cmd4_wire[1] ^ c4_sb[1];
    let cmd4_ctrl_expected = (cmd4_src0 & 0xD7) ^ (c4_sb[0] & 0xD7);
    println!("CMD4:");
    println!("  state = {:#010x}, sb = {:02x?}", state, c4_sb);
    println!("  src0 = {:#04x} ^ {:#04x} = {:#04x}", cmd4_wire[1], c4_sb[1], cmd4_src0);
    println!("  ctrl = {:#04x} (want {:#04x}): {}",
        cmd4_ctrl_expected, cmd4_wire[0],
        if cmd4_ctrl_expected == cmd4_wire[0] { "OK" } else { "FAIL" });
    let cmd4_plain: Vec<u8> = (1..8).map(|i| {
        let bit_off = ((i * 8) & 0x18) as u32;
        let sb_byte = ((state >> bit_off) & 0xFF) as u8;
        cmd4_wire[i] ^ sb_byte
    }).collect();
    println!("  cmd4 source: {:02x?}", cmd4_plain);
    advance_seed(&mut state);

    // resp4: [ad, 08, 25, 37, d1, 4f]
    let r4_sb = state.to_le_bytes();
    let resp4_wire = [0xadu8, 0x08, 0x25, 0x37, 0xd1, 0x4f];
    let r4_plain: Vec<u8> = resp4_wire.iter().enumerate()
        .map(|(i, &w)| w ^ r4_sb[i % 4]).collect();
    println!("RESP4:");
    println!("  state = {:#010x}, sb = {:02x?}", state, r4_sb);
    println!("  plain: {:02x?}", r4_plain);
    advance_seed(&mut state);

    // cmd5: [01, 6c]
    let c5_sb = state.to_le_bytes();
    let cmd5_wire = [0x01u8, 0x6c];
    let cmd5_src0 = cmd5_wire[1] ^ c5_sb[1];
    let cmd5_ctrl_expected = (cmd5_src0 & 0xD7) ^ (c5_sb[0] & 0xD7);
    println!("CMD5:");
    println!("  state = {:#010x}, sb = {:02x?}", state, c5_sb);
    println!("  src0 = {:#04x}", cmd5_src0);
    println!("  ctrl = {:#04x} (want {:#04x}): {}",
        cmd5_ctrl_expected, cmd5_wire[0],
        if cmd5_ctrl_expected == cmd5_wire[0] { "OK" } else { "FAIL" });
    advance_seed(&mut state);

    // resp5: [a6]
    let r5_sb = state.to_le_bytes();
    let resp5_plain = 0xa6 ^ r5_sb[0];
    println!("RESP5:");
    println!("  state = {:#010x}, sb0 = {:#04x}", state, r5_sb[0]);
    println!("  plain = {:#04x}", resp5_plain);
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
