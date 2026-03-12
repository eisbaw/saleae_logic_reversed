---
id: TASK-0108
title: Fix encrypt_reseed to use initial seed instead of rolling seed
status: Done
assignee: []
created_date: '2026-03-11 02:03'
updated_date: '2026-03-11 19:46'
labels:
  - bug
  - encryption
  - ep1
dependencies: []
references:
  - re/notes/init-sequence-pcap.md
  - crates/saleae-proto/src/obfuscation.rs
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Pcap analysis of all 4 reseed operations (frames 15, 1627, 1639, 1651) proves that reseed packets are ALWAYS encrypted with the initial/default LFSR seed, NOT the current rolling seed. The current `encrypt_reseed` in `obfuscation.rs` uses `self.seed` which is the rolling state -- this works for the first reseed but breaks for subsequent ones.

The fix requires storing the initial seed separately in the `SimpleEncryption` struct and using it for `encrypt_reseed`. The `encrypt_command`/`decrypt_response` methods continue using the rolling seed as before.

Evidence: Decoding all 928 EP1 frames from usb.logic8pro.pcap with initial-seed-based reseed decryption produces 0 unknown opcodes. Using rolling-seed-based reseed decryption produces garbage after reseed #2.

See: re/notes/init-sequence-pcap.md for full analysis.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Fixed in commit 07d5c09. encrypt_reseed now stores initial_seed separately and always uses it for reseed encryption, regardless of rolling LFSR state. DEFAULT_INITIAL_SEED constant added in 34b72d0 with doc references.
<!-- SECTION:FINAL_SUMMARY:END -->
