# Project Instructions

Read PRD.md at the start of every session to understand the project scope,
architecture, and current phase.

## Hardware Access

A Saleae Logic Pro 8 (21a9:1005) is physically attached to the local machine.
Bus 003 or 004 (USB 2.0 or 3.0 depending on port). After firmware upload:
3 bulk endpoints (EP1 OUT, EP1 IN, EP2 IN). In bootloader mode: iProduct "WestBridge".

```
lsusb -d 21a9:                # quick check
lsusb -v -d 21a9:             # full descriptors
target/release/saleae-cli list-devices  # after build (udev rules, no sudo)
```

## Documentation Rules

- Backlog tasks contain hypotheses, investigations, and leads to follow.
- `doc/` contains ONLY established, verified knowledge.
- `re/` contains working notes, USB captures, and Ghidra exports.
- When a hypothesis is confirmed, promote it from a backlog task to `doc/`.
- Whenever a new avenue of exploration is discovered (interesting symbol,
  unexplained data flow, unknown USB command, etc.), create a new backlog
  task immediately. Do not let leads go unrecorded.
