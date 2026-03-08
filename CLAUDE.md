# Project Instructions

Read PRD.md at the start of every session to understand the project scope,
architecture, and current phase.

## Hardware Access

A Saleae Logic Pro 8 (21a9:1005) is physically attached to `nvdk-msdlab-09`.
Bus 002, USB 3.0 SuperSpeed, 3 bulk endpoints (EP1 OUT, EP1 IN, EP2 IN).
iSerial reports "SERNUM" (this is just Saleae's label, not a placeholder).
SSH access is available. Use SSHFS to mount remote filesystems when needed
(e.g., for USB captures, lsusb output, or running tools against the device).

```
ssh nvdk-msdlab-09 'lsusb -d 21a9:'     # quick check
ssh nvdk-msdlab-09 'lsusb -v -d 21a9:'  # full descriptors
```

## Documentation Rules

- Backlog tasks contain hypotheses, investigations, and leads to follow.
- `doc/` contains ONLY established, verified knowledge.
- `re/` contains working notes, USB captures, and Ghidra exports.
- When a hypothesis is confirmed, promote it from a backlog task to `doc/`.
- Whenever a new avenue of exploration is discovered (interesting symbol,
  unexplained data flow, unknown USB command, etc.), create a new backlog
  task immediately. Do not let leads go unrecorded.
