---
id: TASK-0049
title: RE SimpleEncryption XOR obfuscation scheme
status: To Do
assignee: []
created_date: '2026-03-09 06:11'
labels:
  - firmware
  - protocol
  - investigation
dependencies:
  - TASK-0028
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
ReseedDevice sends 4 random bytes as a shared XOR seed. The seed is used by SetControlByte() in SimpleEncryption.cpp to obfuscate EP1 command traffic. Need to reverse the exact XOR scheme: is it rolling XOR, PRNG-seeded, or cyclic over the 4 bytes? This affects whether our Rust driver needs to implement the obfuscation layer or if it can be bypassed (e.g., by not sending a reseed). Also need to determine which commands are obfuscated -- all EP1 traffic after reseed, or only specific commands?
<!-- SECTION:DESCRIPTION:END -->
