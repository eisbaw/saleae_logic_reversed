---
id: TASK-0101
title: RE calibration download mechanism -- how does the app get calibration data?
status: Done
assignee: []
created_date: '2026-03-10 23:14'
updated_date: '2026-03-10 23:30'
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
Calibration download is a split architecture:\n\n1. Electron frontend (CalibrationManager in bundle.js): POST https://api.saleae.com/graphql with GetDeviceCalibration(deviceId, deviceType, version) -> downloads .cal file from https://downloads.saleae.com/calibration/{hex_id}.cal -> parses Boost.Serialization XML via xml2json -> caches JSON to ~/.config/Logic/calibrations/{deviceId}-{lastUpdated}-1.cal\n\n2. Native library receives parsed calibration via SetCalibration JSON action (ELF 0x2cd0200). Extracts /deviceId, /fullScaleVoltageRanges, /calibrationSets, /hittiteAdcGainRegisterValue.\n\n3. MSO uses separate EEPROM-based path (SALEAE_MSO_CALIBRATION_PATH env var) -- not relevant to Logic devices.\n\nThe binary contains ZERO HTTP URLs -- all network ops are JS-side.\n\nOur existing saleae-driver/src/calibration.rs direct-download approach is correct.\n\nAnalysis: re/notes/calibration-download.md"
<!-- SECTION:FINAL_SUMMARY:END -->
