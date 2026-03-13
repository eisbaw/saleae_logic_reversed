---
id: TASK-0191
title: Rename calibration.rs vs calibrate.rs for clarity
status: Done
assignee: []
created_date: '2026-03-13 05:36'
updated_date: '2026-03-13 09:17'
labels:
  - driver
  - quality
  - naming
dependencies: []
references:
  - crates/saleae-driver/src/calibration.rs
  - crates/saleae-driver/src/calibrate.rs
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
saleae-driver has two confusingly named calibration modules: calibration.rs (downloads .cal files from Saleae servers, caches, parses via proto) and calibrate.rs (applies biquad filter + voltage transform to disaggregated analog data). Rename to cal_download.rs and cal_pipeline.rs (or similar) to clarify their distinct responsibilities.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Renamed calibration.rs to cal_download.rs and calibrate.rs to cal_pipeline.rs for clarity.
<!-- SECTION:FINAL_SUMMARY:END -->
