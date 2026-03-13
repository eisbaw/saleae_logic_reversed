---
id: TASK-0190
title: Gate MockTransport behind cfg(test) or feature flag
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 09:17'
labels:
  - quality
  - testing
  - usb
dependencies: []
references:
  - crates/saleae-usb/src/mock.rs
  - 'crates/saleae-driver/src/transfer_queue.rs:264'
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
MockTransport in saleae-usb/src/mock.rs is pub and compiled into release builds. Its .lock().unwrap() calls on Mutex can panic on poisoned mutex. Either gate behind #[cfg(test)] or add a "test-support" feature flag. Also: env var SALEAE_USB_TRANSFER_LIMIT in transfer_queue.rs silently ignores garbage values (parse().ok()?) -- should log a warning.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Added test-support feature flag to saleae-usb. Gated mock modules behind cfg(any(feature=test-support, test)). Added tracing::warn for unparseable SALEAE_USB_TRANSFER_LIMIT.
<!-- SECTION:FINAL_SUMMARY:END -->
