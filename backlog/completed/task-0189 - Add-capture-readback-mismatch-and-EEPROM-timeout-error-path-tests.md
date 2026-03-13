---
id: TASK-0189
title: Add capture readback mismatch and EEPROM timeout error path tests
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 07:46'
labels:
  - testing
  - driver
  - error-handling
dependencies: []
references:
  - crates/saleae-driver/src/capture.rs
  - crates/saleae-driver/src/eeprom.rs
  - crates/saleae-driver/src/firmware.rs
  - crates/saleae-proto/src/sal.rs
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Several critical error paths have zero test coverage: (1) configure_capture readback mismatch -- verification code exists but failure path untested. (2) EEPROM poll MAX_POLL_ATTEMPTS timeout -- when I2C poll loop exhausts all attempts, error path never exercised. (3) firmware upload partial failure -- no test for control transfer failing mid-upload. (4) SAL reader corrupt ZIP / missing entries -- no adversarial tests.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added 11 error path tests across 4 crates covering previously untested failure scenarios:

1. **capture.rs** (2 tests): `configure_capture_readback_mismatch_returns_error` and `configure_capture_readback_short_response_returns_mismatch` -- verify ReadbackMismatch when FPGA registers read back wrong values or too few bytes.

2. **eeprom.rs** (2 tests): `polled_read_timeout_after_max_attempts` and `polled_read_timeout_with_empty_responses` -- verify PollTimeout after 20 consecutive not-ready or empty I2C responses.

3. **firmware.rs** (2 tests): `upload_fails_mid_transfer` and `upload_fails_on_entry_trigger` -- verify firmware upload propagates USB errors mid-upload. Required adding `push_control_out_error`/`push_control_out_success` to MockTransport.

4. **sal.rs** (5 tests): corrupt ZIP, empty input, missing meta.json, invalid JSON, truncated digital header/body, truncated analog header -- all return appropriate io::Error variants.

MockTransport gained `ControlOutResult` enum and `control_out_results` queue for error injection on control OUT transfers. Backward compatible (empty queue defaults to success).

895 workspace tests pass. Commit eaf5cbd.
<!-- SECTION:FINAL_SUMMARY:END -->
