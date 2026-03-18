# Saleae Logic 2 -- Open-Source Rust Driver

A clean-room reverse engineering effort to build an open-source Rust driver for
Saleae Logic 2 USB logic analyzers, without the proprietary GUI.

The goal is ground truth: correct digital and analog samples from the device,
with calibration properly applied.

## Supported Hardware

| Device | VID:PID | Controller | Max Digital | Max Analog | Channels | Status |
|--------|---------|------------|-------------|------------|----------|--------|
| Logic Pro 16 | 21a9:1006 | Cypress FX3 | 500 MHz | 50 MHz | 16 | Primary target |
| Logic Pro 8 | 21a9:1005 | Cypress FX3 | 500 MHz | 50 MHz | 8 | Tested on hardware |
| Logic 8 | 21a9:1004 | Cypress FX3 | 200 MHz | 20 MHz | 8 | Supported |
| Logic 4 | 21a9:1003 | Cypress FX3 | 12 MHz | 6 MHz | 4 | Supported |
| Logic 16 | 21a9:1001 | Cypress FX2LP | Legacy | Legacy | 16 | Low priority |
| Original Logic | 0925:3881 | Cypress FX2 | Legacy | N/A | 8 | Lowest priority |

## What Works

- Device enumeration and hotplug detection
- Cypress FX3 firmware upload
- FPGA bitstream programming (Lattice ECP5 + Xilinx variants)
- Digital capture up to 500 MHz, 1-8 channels, with RLE storage
- Analog capture up to 50 MHz with HMCAD1100/AD9637 ADC support
- Analog front-end configuration (LMH6518 VGA)
- IIR biquad filter compensation from factory calibration data
- Per-device calibration download from Saleae servers
- Trigger detection (edge/pulse)
- Export to CSV, binary, and .sal formats
- EEPROM access for device identity and calibration pointers

## Architecture

Four Rust crates in a workspace:

```
crates/
  saleae-proto/    Wire format types, packet parsing, calibration, biquad filters
  saleae-usb/      USB transport layer (nusb backend + mock for testing)
  saleae-driver/   High-level driver: init, capture, FPGA, ADC, calibration pipeline
  saleae-cli/      CLI binary
```

Protocol and hardware documentation lives in `doc/`, organized by subsystem.
Reverse engineering artifacts (Ghidra exports, USB captures, scripts) live in `re/`.

## Quick Start

Requires [Nix](https://nixos.org/download.html) for reproducible dependencies.

```bash
# Enter dev environment (pulls Ghidra, Wireshark, Rust toolchain, etc.)
nix-shell

# Verify tools and USB access
just check

# Build
just build

# Run tests
just test

# Lint
just lint

# End-to-end tests (requires hardware)
just e2e
```

### Udev Rules (Linux)

Required for non-root USB access:

```bash
just udev-install
```

### Hardware Commands

```bash
just hw-list                # List connected Saleae devices
just hw-capture-digital     # 8-channel digital capture
just hw-capture-analog      # Analog capture
just hw-capture-matrix      # Comprehensive test matrix (14 configurations)
just hw-upload-fw           # Upload firmware only
just hw-program-fpga        # Program FPGA only
```

### Reverse Engineering

```bash
just extract-asar           # Extract Electron app.asar
just extract-firmware       # Extract firmware/bitstreams from .so
just ghidra-import          # Import libgraph_server_shared.so into Ghidra
just ghidra-export          # Export decompiled C code
just strings-search PATTERN # Search Saleae binaries
just usb-capture-quick 10   # 10-second USB traffic capture
```

## Documentation

All verified protocol knowledge is in `doc/`:

- `doc/protocol/ep1-commands.md` -- USB command protocol (opcodes 0x01-0x8B)
- `doc/protocol/digital-capture.md` -- Digital sample streaming format
- `doc/protocol/analog-capture.md` -- Analog ADC data format, AFE pipeline
- `doc/protocol/fpga-bitstream.md` -- FPGA bitstream upload (10 Lattice + 11 Xilinx)
- `doc/protocol/capture-sequence.md` -- Full capture start/stop sequence
- `doc/protocol/fx3-firmware.md` -- Cypress FX3 firmware upload
- `doc/calibration/calibration-format.md` -- .cal file format (IIR biquad, ADC transforms)
- `doc/devices/device-types.md` -- Device enumeration, VID:PID mapping
- `doc/devices/device-capabilities.md` -- Sample rates, ADC specs, channels

Documents are tagged with confidence levels: **Confirmed** (validated against
captures + Ghidra + hardware), **High** (two sources), or **Hypothesis** (single
source, kept in backlog until promoted).

## Project Layout

```
PRD.md              Product requirements document
doc/                Established, verified protocol knowledge
re/                 Reverse engineering artifacts
  ghidra/           Ghidra project and scripts
  captures/         USB pcap files and analysis
  scripts/          Python/Rust RE tools
  extracted-asar/   Electron app extraction
crates/             Rust workspace
justfile            All build/test/RE commands
shell.nix           Nix dev environment
```

## Out of Scope

- Protocol analyzers/decoders (SPI, I2C, UART, etc.) -- these are GUI plugins
- GUI, rendering, graph display
- Web/cloud/telemetry features
- sigrok/libsigrok compatibility (this is a clean-room effort)

## License

This is a reverse engineering research project. No Saleae proprietary code is
included in this repository. Firmware and bitstream blobs are extracted at build
time and excluded from version control.
