# PRD: Reverse Engineering the Saleae Logic 2 Application

## Goal

Fully reverse engineer the Saleae Logic 2 desktop application (v2.4.40) to
understand and document the USB protocol, FPGA bitstream upload, firmware
download, capture start/stop sequences, calibration, and all device I/O -- with
the aim of producing an open replacement library, written in Rust, capable of
driving Saleae hardware without the proprietary GUI.

The deliverable is ground truth data: correct digital and analog samples from
the device, with calibration properly applied.

### In Scope

- USB device enumeration, identification, state machine
- Cypress FX3 firmware upload protocol
- FPGA bitstream download protocol (Altera, Xilinx, Lattice variants)
- Capture start/stop, sample rate configuration, channel selection
- Digital sample streaming and decoding
- Analog sample streaming, ADC data decoding, and calibration application
- Analog IIR biquad filter reconstruction (calibration-derived, used to compensate
  analog front-end imperfections)
- Calibration data format, storage, and application
- Device boot sequence (FW upload -> FPGA program -> device ready)

### Out of Scope

- Web/cloud/telemetry features
- Protocol analyzers/decoders (SPI, I2C, I2S, UART, etc.) -- these are plugins
  with a separate API, not part of the core device driver
- GUI, rendering, graph display
- sigrok/libsigrok -- this is a clean-room reverse engineering effort

### Target Devices (priority order)

1. **Logic Pro 16** (21a9:1006) -- primary target, full analog + digital
2. **Logic Pro 8** (21a9:1005) -- same family, fewer channels
3. **Logic 8** (21a9:1004) -- non-Pro, simpler analog
4. **Logic 4** (21a9:1003) -- non-Pro, simplest
5. Logic 16 (21a9:1001) -- legacy FX2-based, lower priority
6. Original Logic (0925:3881) -- legacy FX2-based, lowest priority
7. Logic MSO -- separate advanced device with SPI flash firmware
   (`MsoSpiFlash::WriteFirmware`), oscilloscope features, distinct from Logic 8.
   Out of scope unless time permits.

## Implementation Target

- **Language**: Rust
- **Build/dev tooling**: `shell.nix` (all dependencies) + `justfile` (all commands)
- **USB**: `nusb` crate (pure Rust, async-native, no C dependency)
- **Output**: Rust workspace:
  - `saleae-proto` -- wire format types, packet parsing, testable without hardware
  - `saleae-usb` -- USB transport layer, mockable for testing
  - `saleae-driver` -- high-level device driver facade
  - `saleae-cli` -- thin CLI binary for testing and validation
- **No GUI** -- this is a headless driver library

## Target Application

- **Package**: `saleae-logic-2` from nixpkgs (unfree, binary-only)
- **Version**: 2.4.40 (Electron 19.x / Chromium 106)
- **Architecture**: Electron shell + native shared libraries
- **Primary binary**: `Logic` (ELF 64-bit, not stripped)
- **Key native library**: `libgraph_server_shared.so` (~48 MB, stripped, statically
  links libusb-1.0, OpenSSL, Python, Tcl, SQLite, BerkeleyDB, lzma, zlib)
- **Graph rendering**: `@saleae/graph-interface` native Node addon (`graph-interface.node`)
- **Electron app**: `resources/app.asar` (~107 MB)
- **Nix store path**: `/nix/store/brcaw41j62pjki70ahf7ffd2f3bhaa3d-saleae-logic-2-extracted/`

## Supported Hardware (from udev rules)

| VID:PID   | Device                       | USB Controller | FPGA           | Priority |
|-----------|------------------------------|----------------|----------------|----------|
| 0925:3881 | Saleae Logic (original)      | Cypress FX2    | None           | Low      |
| 21a9:1001 | Logic 16 (legacy)            | Cypress FX2LP  | Xilinx Spartan | Low      |
| 21a9:1003 | Logic 4                      | Cypress FX3    | Xilinx Spartan | Medium   |
| 21a9:1004 | Logic 8                      | Cypress FX3    | Xilinx Spartan | High     |
| 21a9:1005 | Logic Pro 8                  | Cypress FX3    | Xilinx Spartan | High     |
| 21a9:1006 | Logic Pro 16                 | Cypress FX3    | Xilinx Spartan | High     |

Note: Hardware revisions (5.0.0, 10.0.0) may use different FPGA parts due to
supply chain changes. String analysis shows references to Altera, Xilinx, and
Lattice -- Xilinx Spartan-6 is the confirmed primary, others may be alternate
revisions or legacy support. The driver must detect and handle FPGA variant
differences.

## Architecture Overview

```
 Electron (app.asar)
   |  IPC (named pipes / gRPC -- to be confirmed)
   v
 libgraph_server_shared.so        <-- The target. All business logic.
   |
   +-- libusb (statically linked)
   |     |
   |     v
   |   /dev/bus/usb (kernel USB)
   |
   +-- Device state machine
   |     (disconnected -> connected -> FW uploaded -> FPGA programmed
   |      -> idle -> configured -> capturing -> idle)
   |
   +-- Firmware upload (Cypress FX3, vendor control transfers)
   +-- FPGA bitstream download (vendor-specific per FPGA)
   +-- Capture engine
   |     +-- Configuration (sample rate, channels, triggers)
   |     +-- USB bulk transfer queue (async, dynamic sizing)
   |     +-- Digital deserializer
   |     +-- Analog deserializer + ADC decoding
   |     +-- On-device data compression (USB 2.0 bandwidth constraint)
   |     +-- Software-only trigger detection (no hardware trigger engine)
   |
   +-- Calibration
   |     +-- AFE (analog front-end) configuration
   |     +-- ADC calibration (gain, offset, INL/DNL)
   |     +-- IIR biquad filter coefficients (compensate analog path imperfections)
   |     +-- Per-channel, per-sample-rate calibration data
   |     +-- Calibration data source: server-hosted AND/OR device EEPROM (TBD)
   |
   +-- graph-interface.node (Node addon -- may be the Electron bridge)
```

Key architectural facts:
- **Streaming sampler**: devices have no significant on-device memory. All sample
  data streams over USB in real-time to the host.
- **Software triggers**: trigger detection runs on the host, not in the FPGA.
  Pre-trigger buffer is managed on host side.
- **Analog is imperfect**: the analog front-end introduces distortion that is
  compensated by IIR biquad filters derived from factory calibration data. Getting
  correct analog samples requires applying these filters.
- **USB 2.0 vs 3.0**: FX3-based devices support USB 3.0 but fall back to 2.0.
  Maximum sample rates depend on negotiated USB speed.

## Key Findings from Initial Recon

### Device Classes (from symbol/string analysis)
- `UsbDevice` -- base USB device abstraction
- `LinuxUsbDevice` -- Linux-specific USB backend
- `Logic2FpgaDevice` -- FPGA-based devices (the "Logic 2" software generation)
- `Fx2DeviceFeatures` -- original Logic / Logic16 (Cypress FX2 based)
- `FpgaDeviceFeatures` -- newer Logic 4/8 devices
- `ProFpgaDeviceFeatures` -- Logic Pro 8/16 devices

### FPGA/Firmware
- Multiple FPGA vendors in strings: Altera, Xilinx, Lattice (Xilinx primary)
- Bitstream CRCs embedded as constants (10 distinct CRC values found)
- `DownloadBitstream`, `LoadBitstream`, `ProgramFpgaBitstream` -- key entry points
- `DownloadFirmware` -- Cypress FX3 firmware upload
- Firmware signature check: expects "CY" header
- `MsoSpiFlash::WriteFirmware` -- MSO-specific firmware path (SPI flash, not USB)
- Bitstream version string: "Final Version 10.27"
- Multiple bitstream variants expected per device (for HW revisions, voltage levels)

### Capture Engine
- Pre/post trigger buffer management with FPGA-specific constraints
- `SamplesPerPacketMultiple_WithStats` -- packet alignment requirement
- Separate Altera vs Xilinx min/max packet sizes (confirms multi-FPGA support)
- Analog: AFE channel config, calibration, decimation
- `AnalogDeserializer`, `AnalogMeasurerV1`, `AnalogMeasurerV2` -- analog pipeline
- Digital: deserialization, smart cable support
- Transfer queue: async USB bulk transfers with dynamic queue sizing
- Sample rate vs channel count tradeoff (USB bandwidth limited)
- `Analog Fifo overflow` -- confirms streaming architecture with overflow risk

### Calibration
- `afeConfigCalibrations` -- analog front-end calibration data
- ADC foreground calibration
- Per-channel calibration
- IIR biquad filter coefficients for analog path compensation
- Calibration applied per-channel and per-sample-rate

### Communication Protocol
- Named pipes for terminal/capture data streaming
- IPC mechanism between Electron and native lib (to be confirmed by asar analysis)

## RE Methodology

### Ghidra as Primary Tool

The 48MB `libgraph_server_shared.so` is the primary RE target. Strategy:

1. **Library carve-out**: Use FLIRT/FunctionID signatures to identify and label
   statically-linked libraries (libusb, OpenSSL, Python, Tcl, SQLite, BerkeleyDB,
   lzma, zlib). This eliminates ~80% of functions as noise.

2. **Boundary-inward analysis**: Start at `libusb_*` call sites. Every
   `libusb_control_transfer`, `libusb_bulk_transfer`, `libusb_submit_transfer`
   is a protocol boundary. Trace backward to find packet constructors, forward
   to find response parsers.

3. **C code export**: Export Ghidra decompiled C code so Claude Code can traverse
   and analyze it alongside the human analyst. This enables AI-assisted pattern
   recognition across the codebase.

4. **Symbol-driven exploration**: When interesting symbols, class names, or string
   references are found, create backlog tasks to explore and trace their data and
   control flow. Each task captures the hypothesis being investigated.

### Dynamic Analysis

- **usbmon + Wireshark/tshark**: capture real USB traffic during all device
  operations (enumeration, FW upload, FPGA program, capture, calibration read)
- **Differential analysis**: capture USB traffic from the original app, then
  compare against the Rust implementation's traffic byte-for-byte

### Documentation Strategy

- **Backlog tasks**: contain hypotheses, investigations, leads to follow.
  Tasks are where "I think register 0x42 controls sample rate" lives.
- **`doc/`**: contains only established, verified knowledge. Protocol specs,
  confirmed packet formats, validated state machines. If it is in `doc/`, it
  has been tested against hardware or confirmed through multiple independent
  evidence sources.
- **Confidence levels** in doc/ files:
  - **Confirmed**: observed in USB capture AND matches Ghidra AND tested in Rust
  - **High**: matches two of the three above
  - **Hypothesis**: single source, noted as such, should not be in doc/ yet

### USB Captures

Naming convention: `captures/{device}_{operation}_{date}.pcapng`
Example: `captures/logic-pro-16_firmware-upload_2026-03-10.pcapng`

Each capture gets a companion `.md` file describing:
- Device model, serial, HW revision
- Operation performed
- Saleae Logic 2 version
- Key observations

## Tools

All tools managed via `shell.nix`; all commands via `justfile`.

### Reverse Engineering
- **Ghidra**: primary static analysis, C code export for AI traversal
- **binwalk**: firmware/bitstream extraction from the .so
- **Wireshark + tshark + usbmon**: USB protocol capture and analysis
- **frida-tools**: dynamic instrumentation of `libgraph_server_shared.so`
- **ltrace/strace**: library and syscall tracing
- **asar**: Electron app.asar extraction and JS analysis
- **binutils**: nm, readelf, objdump for ELF analysis
- **python3**: scripting for data analysis, Ghidra scripts
- **xxd**: hex inspection of firmware blobs and USB payloads

### Rust Development
- **rustc / cargo**: Rust toolchain (stable)
- **clippy / rustfmt**: linting and formatting
- **cargo-nextest**: test runner
- **cargo-fuzz**: parser fuzzing (mandatory for all packet parsers)
- **nusb**: USB device access (pure Rust, async)

## Vertical Slices

Work is organized as vertical slices, not waterfall phases. Each slice includes
RE + implementation + testing for one subsystem. Each produces a working,
testable increment.

### Slice 1: Decomposition and Environment Setup
- Extract and catalog all binaries, shared libraries, and resources
- Set up shell.nix, justfile, Ghidra project
- Run FLIRT/FunctionID to label known library functions in Ghidra
- Export decompiled C code from Ghidra for AI-assisted analysis
- Extract app.asar, identify IPC mechanism
- Create backlog tasks for every interesting symbol cluster found
- **Deliverable**: annotated Ghidra project, exported C code, working dev env

### Slice 2: Device Enumeration and Identification
- RE USB descriptors, device identification, feature detection
- RE the device state machine (states and transitions)
- Implement `saleae-usb` crate: device discovery, USB descriptor parsing
- Implement `saleae-proto`: device identification packet types
- **Deliverable**: `saleae-cli list-devices` prints connected Saleae devices

### Slice 3: Firmware and Bitstream Upload
- RE Cypress FX3 firmware upload protocol (vendor control transfers)
- RE FPGA bitstream download protocol
- Extract embedded firmware/bitstream images from the .so
- Implement firmware upload in Rust, test against hardware
- **Deliverable**: `saleae-cli upload-firmware` boots a device to ready state

### Slice 4: Digital Capture
- RE capture configuration for digital channels
- RE capture start/stop command sequence
- RE digital data streaming format (bulk transfer packets)
- RE trigger mechanism (software-only, host-side)
- Implement digital capture in Rust
- **Deliverable**: `saleae-cli capture-digital` produces correct digital samples

### Slice 5: Analog Capture and Calibration
- RE analog data streaming format, ADC encoding
- RE calibration data format (AFE config, IIR biquad filter coefficients)
- RE calibration data source and download mechanism
- RE IIR biquad filter application pipeline
- Implement analog capture with calibration in Rust
- **Deliverable**: `saleae-cli capture-analog` produces calibrated analog samples

### Slice 6: Multi-device and Validation
- Test against all available device variants
- Handle hardware revision differences (FPGA variants, bitstream selection)
- Handle USB 2.0 vs 3.0 speed negotiation and sample rate limits
- End-to-end validation: compare output against Saleae Logic 2 captures
- **Deliverable**: validated driver library with full test suite

## Project Structure

```
saleae_reverse/
  PRD.md                    # This file
  shell.nix                 # All dependencies
  justfile                  # All commands
  .gitignore                # target/, firmware/, ghidra projects, etc.
  doc/                      # ESTABLISHED knowledge only
    protocol/               # Verified protocol specifications
    calibration/            # Verified calibration format docs
    state-machine/          # Verified state machine diagrams
  re/                       # Working RE artifacts
    ghidra/                 # Ghidra scripts, exported C code
    captures/               # USB pcap files + companion .md notes
    notes/                  # Scratch analysis notes (ephemeral)
  firmware/                 # Extracted firmware/bitstreams (gitignored)
  crates/
    saleae-proto/           # Wire format types, packet parsing
    saleae-usb/             # USB transport layer (mockable)
    saleae-driver/          # High-level driver facade
    saleae-cli/             # CLI binary
```

## Risks and Mitigations

- **48MB stripped binary**: mitigated by FLIRT library identification + boundary-inward
  analysis from libusb call sites + AI-assisted C code analysis
- **Firmware/bitstream redistribution**: `.gitignore` from day one, extraction tool
  that pulls from local Saleae installation
- **Hardware bricking**: FX3 firmware upload targets RAM (volatile, safe on power
  cycle). FPGA bitstreams are volatile. `MsoSpiFlash` writes to non-volatile
  storage -- defer MSO support until protocol is fully understood
- **Analog accuracy**: IIR biquad filter reconstruction is critical and complex. Validate
  against known signals (1kHz square wave, sine sweep) comparing Saleae app
  output vs our output sample-by-sample
- **Multiple FPGA revisions**: detect via device descriptors or firmware response,
  select correct bitstream. Create backlog tasks as variants are discovered
