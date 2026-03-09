# FPGA Bitstream Download Protocol

Confidence: **High** -- findings are from direct binary analysis of
libgraph_server_shared.so v2.4.40, including embedded bitstream headers,
string references, and mangled C++ symbol names. Not yet validated against
USB captures.

## Overview

Saleae Logic 2 devices use FPGAs for data acquisition. The host downloads
a bitstream to the FPGA's volatile configuration memory on every power
cycle, through the FX3 USB microcontroller. Three FPGA families are
supported across hardware revisions.

All bitstreams are embedded in the `libgraph_server_shared.so` binary (no
external files). The correct bitstream is selected based on USB PID
(device type) and hardware revision read from the device's EEPROM.

## FPGA Parts

### Lattice ECP5 Family (Current Generation)

All current-generation devices (Logic 4, 8, Pro 8, Pro 16) use Lattice
ECP5 FPGAs in CSFBGA285 package. Three parts across hardware revisions:

| Part                      | LUTs  | Architecture | HW Revision |
|---------------------------|-------|--------------|-------------|
| LFE5UM-25F-8CSFBGA285    | 24K   | ECP5UM       | Older       |
| LFE5UM5G-45F-8CSFBGA285  | 44K   | ECP5UM5G     | Mid         |
| LFE5U-45F-6/7CSFBGA285   | 44K   | ECP5U        | Newer       |
| LFE5U-12F-7CSFBGA285     | 12K   | ECP5U        | Newest      |

All bitstreams were generated with Lattice Diamond 3.12.1.454, version
string "Final Version 10.27". Security and readback are disabled (no
bitstream encryption).

### Xilinx Spartan-6 (Legacy)

Legacy devices use Xilinx Spartan-6 FPGAs:

| Part      | IDCODE       | Bitstream Size | Devices          |
|-----------|-------------|----------------|------------------|
| XC6SLX9   | 0x04001093  | ~333 KB        | Logic 16, Logic 4|
| XC6SLX16  | 0x04002093  | ~453 KB        | Logic 16         |

Spartan-6 bitstreams use the 16-bit configuration packet format.

### Xilinx 7-Series (Unknown Part)

Two large (~4.5 MB) bitstreams with IDCODE 0x04AC2093 use the 7-series
32-bit configuration protocol. The part code is not in public Xilinx
documentation -- may be an early prototype or NDA-restricted variant.

## Embedded Bitstream Map

### 10 Lattice Bitstreams with CRC Values

| CRC    | Part                    | Design                   | Date       |
|--------|-------------------------|--------------------------|------------|
| 0xD8EC | LFE5UM-25F-8CSFBGA285   | Logic8_Top_ADS528x       | 2022-04-12 |
| 0xCE24 | LFE5UM-25F-8CSFBGA285   | Logic_Pro_Top_ADS528x    | 2022-04-22 |
| 0x36E3 | LFE5UM-25F-8CSFBGA285   | Logic_Pro16_Top_ADS528x  | 2022-04-22 |
| 0xF147 | LFE5UM5G-45F-8CSFBGA285 | Logic8_Top_ADS528x       | 2022-08-09 |
| 0x25C9 | LFE5UM5G-45F-8CSFBGA285 | Logic_Pro_Top_ADS528x    | 2022-05-04 |
| 0x74EF | LFE5UM5G-45F-8CSFBGA285 | Logic_Pro16_Top_ADS528x  | 2022-05-04 |
| 0xF4ED | LFE5U-45F-6CSFBGA285    | Logic8_Top_ADS528x       | 2023-01-03 |
| 0x6391 | LFE5U-45F-7CSFBGA285    | Logic_Pro16_Top_ADS528x  | 2023-06-06 |
| 0x80F3 | LFE5U-12F-7CSFBGA285    | Logic8_Top_ADS528x       | 2023-07-11 |
| 0xAC6D | LFE5U-12F-7CSFBGA285    | Logic_Pro_Top_ADS528x    | 2023-06-27 |

Design names reference ADS528x (TI ADS5282/ADS5287) ADC family, indicating
the FPGA interface is tailored to this specific ADC.

### 11 Xilinx Bitstreams

2 x 7-series (~4.5 MB each) + 6 x Spartan-6 SLX9 (~333 KB each) +
3 x Spartan-6 SLX16 (~453 KB each).

## Programming Sequence

### Key Functions

| Function                    | Role                                    |
|-----------------------------|-----------------------------------------|
| `ProgramFpgaBitstream`      | Top-level entry point                   |
| `ResetFpga`                 | Reset FPGA before programming           |
| `WriteConfigurationData`    | Send config data via USB                |
| `GetFpgaBitstreamVersion`   | Read back version after programming     |
| `IsFpgaResponsive`          | Verify FPGA is responding               |
| `GetFpgaImage`              | Select correct bitstream for device     |
| `CommonDeviceInit`          | Post-programming initialization         |

### Sequence

1. **Reset**: `ResetFpga` via USB control transfer
2. **Select**: `GetFpgaImage(MsoDeviceMetadata)` picks bitstream by
   device type and hardware revision
3. **Program**: `ProgramFpgaBitstream` sends data via `WriteConfigurationData`
   - Max chunk size: 62 bytes (USB 2.0) or 509 bytes (USB 3.0)
   - Logged: "Programming FPGA bitstream, {} bytes"
   - Optional chunked mode: `SALEAE_CHUNKED_FPGA_PROGRAMMING` env var
4. **Verify**: Read back register expecting 0xAA
   - "FPGA readback got {:#x} instead of 0xAA" on failure
5. **Init**: `CommonDeviceInit` sets up ADC, PLL, JESD links
   - Lattice: "Lattice PLL and SERDES reset complete"
   - JESD204B link establishment with retry logic

### FPGA Register Interface

Post-programming, FPGA is controlled via 16-bit addressed, 32-bit valued
registers, tunneled through FX3 USB:

- `FpgaDeviceFeatures::ReadRegisters()`
- `FpgaDeviceFeatures::WriteRegisters()`
- Log format: `FPGA WriteRegister 0x{:04x} = 0x{:08x} ({})`

## Bitstream Selection Logic

Selection is based on:

1. **USB PID** determines device type (Logic 4/8/Pro 8/Pro 16)
2. **EEPROM** stores board revision (`ParseBoardRevision`)
   - `FPGA HW Revision: {}.{}`
   - Includes ADC type: HMCAD1100 vs AD9637
3. **RevisionDetail** struct determines:
   - Which FPGA part is on this board
   - Which crypto interface to use
   - Whether new features are available

Override: `SALEAE_FPGA_IMAGE` env var loads bitstream from file path.

## Device Class Hierarchy

```
Logic2FpgaDevice (base for all FX3 devices)
  +-- LogicStudentDevice    (Logic 4,  PID 1003)
  +-- LogicGraduateDevice   (Logic 8,  PID 1004)
  +-- LogicProfessionalDevice
        +-- LogicProfessional8Device   (Pro 8,  PID 1005)
        +-- LogicProfessional16Device  (Pro 16, PID 1006)

Logic16Device (separate, FX2-based, PID 1001)
```

## Authentication

Devices include a hardware crypto IC (likely Microchip ATECC508A/608A):

- `CryptoInterfaceFpga` -- Accessed through FPGA registers
- `CryptoInterfaceI2c` -- Accessed via I2C (older devices)
- `CryptoInterfacePro` -- Pro device variant
- Uses ECDSA signing (`SignDigest`, `GetPublicKey`)
- `RevisionDetail::UsesNewCryptoInterface()` selects variant
