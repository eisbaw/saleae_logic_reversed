---
id: TASK-0166
title: 'Add integration test with EncryptionMode::XorAll for encrypted protocol flow'
status: Done
assignee: []
created_date: '2026-03-12 22:28'
updated_date: '2026-03-13 00:11'
labels:
  - testing
  - driver
  - encryption
dependencies: []
references:
  - crates/saleae-driver/tests/integration.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
All integration tests use EncryptionMode::None. No test verifies the full encrypted roundtrip (reseed with non-zero seed, encrypted ping, encrypted register read/write). The obfuscation.rs unit tests cover the crypto, but the integration path through Ep1CommandChannel with XorAll is untested. Seed desync after timeout is a real failure mode that needs coverage.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added 7 integration tests exercising EncryptionMode::XorAll through the full Ep1CommandChannel stack. Extracted `initialize_with_seed()` from `initialize()` to enable deterministic testing with pre-computed encrypted mock responses.

Tests added:
- `encrypted_initialize_sends_reseed_then_encrypted_ping` -- verifies 5-byte encrypted reseed + 3-byte encrypted ping wire format
- `encrypted_ping_roundtrip` -- full encrypted ping through channel
- `encrypted_register_write_then_read_roundtrip` -- write/read registers through encrypted channel
- `encrypted_multiple_pings_stay_in_sync` -- 5 consecutive pings verify LFSR lockstep
- `encrypted_seed_desync_causes_ping_failure` -- wrong encryption key detected
- `encrypted_initialize_rejects_zero_seed` -- zero seed validation
- `encrypted_write_read_mixed_sequence` -- batch write + batch read + ping

Each test uses a parallel SimpleEncryption firmware simulator to pre-compute encrypted responses for the mock transport.

Commit: 2d17ba3
<!-- SECTION:FINAL_SUMMARY:END -->
