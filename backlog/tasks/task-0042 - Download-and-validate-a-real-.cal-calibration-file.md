---
id: TASK-0042
title: Download and validate a real .cal calibration file
status: Done
assignee: []
created_date: '2026-03-09 01:02'
updated_date: '2026-03-09 07:29'
labels:
  - calibration
  - validation
dependencies:
  - TASK-0013
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Download a real calibration file from downloads.saleae.com/calibration/{deviceIdHex}.cal using the Logic Pro 8 device connected to nvdk-msdlab-09. Parse the Boost.Serialization XML and validate:
1. XML structure matches documented format
2. Count biquad sections per channel per sample rate (expected: 5)
3. Verify a0 is always 1.0
4. Document available sample rates in the calibration data
5. Verify fullScaleVoltageRanges per channel
6. Check mAdcGain value
7. This validates TASK-0013 findings against real data
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Downloaded and validated a real calibration file for a Logic Pro 8 device (ID: 8520900333282843121, hex: 764051B51BD4D9F1).

## Key Findings

### Download Mechanism Validated
- GraphQL API at api.saleae.com/graphql works with the documented query format
- The API requires newlines in the query string (returns 400 without them)
- Download URL is returned without https:// prefix
- Direct download from downloads.saleae.com/calibration/{hex}.cal confirmed working
- The URL for the nvdk-msdlab-09 device (serial 0000000004BE) returned null -- the calibration device ID is NOT the USB serial number

### Format Validated Against TASK-0013
- XML structure matches documentation (with corrections needed for _text wrappers)
- 48 entries (8 channels x 6 sample rates) confirmed
- 30 coefficients per entry (5 sections x 6 values) confirmed
- a0 is always exactly 1.0 across all 48 entries confirmed
- mAdcGain = 0 for this device

### Documentation Corrections Required
1. _text wrappers are xml2json artifacts, NOT in actual XML
2. Filter is effectively 9th order (section 0 is first-order: b2=0, a2=0)
3. Added undocumented fields: mCalibrationType, mFirTaps, mDataType
4. mFirTaps duplicates mDf1SosCoefficients (historical artifact)

### New Insights
- Sections 1-4 are IDENTICAL across all channels at a given sample rate
- Only section 0 varies per channel (per-channel AFE correction)
- Sample rates are power-of-2 divisions of 50 MHz: 50M, 12.5M, 6.25M, 3.125M, 1.5625M, 781.25k
- 25 MHz rate is absent
- Group delay is 3 at 50 MHz, 5 at all other rates

### Files
- re/calibration-764051b51bd4d9f1.cal -- raw Boost.Serialization XML
- re/notes/calibration-validation.md -- full validation report
- doc/calibration/calibration-format.md -- updated with corrections
<!-- SECTION:FINAL_SUMMARY:END -->
