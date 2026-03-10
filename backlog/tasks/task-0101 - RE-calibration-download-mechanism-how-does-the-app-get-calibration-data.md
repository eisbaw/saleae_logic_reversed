---
id: TASK-0101
title: RE calibration download mechanism -- how does the app get calibration data?
status: Done
assignee: []
created_date: '2026-03-10 23:14'
updated_date: '2026-03-10 23:29'
labels:
  - re
  - ghidra
  - calibration
  - blocking
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
We have calibration XML parsing (saleae-proto/src/calibration.rs) and know the pipeline (ADC -> VoltageTransform -> BiquadFilter), but we don't know HOW the app obtains calibration data.

Possible sources identified in RE:
1. HTTP download from Saleae servers (strings show HTTP/networking code)
2. EEPROM read from device (device has EEPROM for serial/device ID)
3. Local cache on disk
4. Bundled in the app or firmware

Investigation steps:
1. Search binary for calibration URL patterns, HTTP endpoints
2. Search for EEPROM read functions that return calibration data
3. Search for calibration file paths on disk (~/.config/Logic/, etc.)
4. Search Electron bundle.js for calibration API calls
5. Check if SALEAE_MSO_CALIBRATION_PATH env var (found in strings) applies to Logic devices too
6. Decompile the calibration download/load path

This is blocking for correct analog captures -- without calibration data, analog output will be uncalibrated.
<!-- SECTION:DESCRIPTION:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Completed reverse engineering of the calibration download mechanism. Key findings:

1. **The native library does NOT download calibration data.** The binary contains zero HTTP URLs to Saleae servers. All download logic is in the Electron frontend (bundle.js CalibrationManager class).

2. **Flow: Cloud -> JS Frontend -> Graph Action JSON -> Native Library**
   - JS queries GraphQL API at `https://api.saleae.com/graphql` for download URL
   - JS downloads `.cal` file from `https://downloads.saleae.com/calibration/{hex_id}.cal`
   - JS parses Boost.Serialization XML via xml2json library
   - JS caches parsed data as JSON in `~/.config/Logic/calibrations/{id}-{ts}-1.cal`
   - JS sends calibration to native via graph node actions (SetCalibration, SetChannel)
   - Native library deserializes JSON using pointer paths (`/calibrationSets`, `/fullScaleVoltageRanges`, etc.)

3. **Ghidra xref analysis confirmed:**
   - SetCalibration deserializer at ELF 0x2cd0200 extracts 4 JSON fields
   - Coefficient lookup at ELF 0x1ad2aa0 matches sample rate to calibration set
   - `reading calibration from {}` is MSO-only (same function as `SALEAE_MSO_CALIBRATION_PATH`)
   - Attenuator transforms (`/attn10Transform`, `/attn100Transform`) are separate from main calibration

4. **Rust implementation confirmed correct:** Direct download from `downloads.saleae.com` (already implemented in `saleae-driver/src/calibration.rs`) is the simplest and correct approach.

Artifacts: `re/notes/calibration-download.md` (full analysis with function map)
<!-- SECTION:FINAL_SUMMARY:END -->
