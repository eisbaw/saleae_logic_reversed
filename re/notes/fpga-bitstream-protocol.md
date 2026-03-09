# FPGA Bitstream Download Protocol Analysis

## Summary

The Saleae Logic 2 binary (`libgraph_server_shared.so` v2.4.40) embeds 21
FPGA bitstreams directly in the `.data` section: 11 Xilinx and 10 Lattice.
These cover three FPGA families across multiple hardware revisions of
Logic 4, Logic 8, Logic Pro 8, and Logic Pro 16.

The programming protocol uses USB bulk writes through the FX3
microcontroller, with a vendor-specific `WriteConfigurationData` command
that chunks the bitstream data. Device authentication uses an ECDSA-based
crypto interface (ATSHA/ATECC-style) accessed via I2C through FPGA registers.

## Embedded Bitstreams

### Xilinx Bitstreams (11 total)

All Xilinx bitstreams are in raw `.bin` format (no `.bit` header), embedded
directly at word-aligned offsets. Two distinct packet formats are present:

#### 7-Series Format (32-bit config packets) -- 2 bitstreams

These two large bitstreams use the Xilinx 7-series configuration protocol
with 32-bit packet words. Both share IDCODE `0x04AC2093` (Xilinx, part code
0x4AC2). This IDCODE does not appear in publicly-documented Xilinx part
lists -- it may be a custom or NDA-restricted variant.

| Index | Offset       | Size (approx) | IDCODE       |
|-------|-------------|----------------|--------------|
| X[0]  | 0x007D9010  | ~4.5 MB        | 0x04AC2093   |
| X[1]  | 0x00C48A20  | ~4.5 MB        | 0x04AC2093   |

These two likely correspond to two device types (Logic Pro 8 and Logic
Pro 16, or two voltage configurations) for an earlier hardware revision.
The config packet sequence includes writes to CTL1 (reg 0x11), WBSTAR
(reg 0x10), CMD, FAR, COR, COR1, IDCODE, MASK, CTL registers.

The `BB 11 22 00 44` bus width auto-detect pattern precedes the sync word,
indicating SelectMAP/SPI configuration mode support.

#### Spartan-6 Format (16-bit config packets) -- 9 bitstreams

These use the Xilinx Spartan-6 configuration protocol with 16-bit packet
words. Two FPGA parts are targeted:

**XC6SLX9** (IDCODE 0x04001093, FLR=0x0380=896) -- 6 bitstreams, ~340KB each:

| Index | Offset       | Bytes (approx) | Notes                    |
|-------|-------------|-----------------|--------------------------|
| X[2]  | 0x010FB860  | ~333 KB         | SLX9 variant A (COR1=3D08) |
| X[3]  | 0x0114EC00  | ~333 KB         | SLX9 variant A           |
| X[4]  | 0x011A1E80  | ~333 KB         | SLX9 variant B (COR1=3D00) |
| X[5]  | 0x011F5100  | ~333 KB         | SLX9 variant B           |
| X[8]  | 0x0132AE20  | ~333 KB         | SLX9 variant A           |
| X[9]  | 0x0137E0A0  | ~333 KB         | SLX9 variant B           |

**XC6SLX16** (IDCODE 0x04002093, FLR=0x0430=1072) -- 3 bitstreams, ~453KB each:

| Index | Offset       | Bytes (approx) | Notes              |
|-------|-------------|-----------------|---------------------|
| X[6]  | 0x01248380  | ~453 KB         | SLX16              |
| X[7]  | 0x012B98D0  | ~453 KB         | SLX16              |
| X[10] | 0x013D1320  | ~453 KB         | SLX16              |

The COR1 register differences (0x3D08 vs 0x3D00) between variant A and B
suggest different startup clock or configuration options -- likely one
per voltage level (1.8V vs 3.3V IO, matching the "Logic16 loading {}
voltage bitstream" log message).

### Lattice ECP5 Bitstreams (10 total)

All Lattice bitstreams have full headers with metadata. They are generated
by Lattice Diamond 3.12.1.454, all tagged "Final Version 10.27".

The 10 CRC values from the task description map 1:1 to these bitstreams:

| Index | Offset       | CRC    | Part                       | Design                        | Architecture | Date              | Bits     |
|-------|-------------|--------|----------------------------|-------------------------------|--------------|-------------------|----------|
| L[0]  | 0x01442862  | 0xD8EC | LFE5UM-25F-8CSFBGA285      | Logic8_Top_ADS528x            | sa5p00m      | 2022-04-12        | 4476704  |
| L[1]  | 0x01477B02  | 0xCE24 | LFE5UM-25F-8CSFBGA285      | Logic_Pro_Top_ADS528x         | sa5p00m      | 2022-04-22        | 4476704  |
| L[2]  | 0x014B5E42  | 0x36E3 | LFE5UM-25F-8CSFBGA285      | Logic_Pro16_Top_ADS528x       | sa5p00m      | 2022-04-22        | 4476704  |
| L[3]  | 0x0150D542  | 0xF147 | LFE5UM5G-45F-8CSFBGA285    | Logic8_Top_ADS528x            | sa5p00g      | 2022-08-09        | 8011620  |
| L[4]  | 0x01552D22  | 0x25C9 | LFE5UM5G-45F-8CSFBGA285    | Logic_Pro_Top_ADS528x         | sa5p00g      | 2022-05-04        | 8011620  |
| L[5]  | 0x015A2AF2  | 0x74EF | LFE5UM5G-45F-8CSFBGA285    | Logic_Pro16_Top_ADS528x       | sa5p00g      | 2022-05-04        | 8011620  |
| L[6]  | 0x01610752  | 0xF4ED | LFE5U-45F-6CSFBGA285       | Logic8_Top_ADS528x            | sa5p00       | 2023-01-03        | 8011620  |
| L[7]  | 0x01656C92  | 0x6391 | LFE5U-45F-7CSFBGA285       | Logic_Pro16_Top_ADS528x       | sa5p00       | 2023-06-06        | 8011620  |
| L[8]  | 0x016B5572  | 0x80F3 | LFE5U-12F-7CSFBGA285       | Logic8_Top_ADS528x            | sa5p00       | 2023-07-11        | 4476704  |
| L[9]  | 0x016EA242  | 0xAC6D | LFE5U-12F-7CSFBGA285       | Logic_Pro_Top_ADS528x         | sa5p00       | 2023-06-27        | 4476704  |

Key observations:
- All in CSFBGA285 package
- Security: Off, Readback: Off (no bitstream encryption)
- Three distinct Lattice ECP5 parts across hardware revisions
- Design names use `ADS528x` suffix = TI ADS5282/ADS5287 ADC family
- sa5p00m = ECP5UM, sa5p00g = ECP5UM5G, sa5p00 = ECP5U

#### Lattice Part-to-Device Mapping

| Part                    | Architecture | Devices                              | Notes          |
|-------------------------|--------------|---------------------------------------|----------------|
| LFE5UM-25F-8CSFBGA285   | ECP5UM 25K   | Logic 8, Pro 8, Pro 16               | Older HW rev   |
| LFE5UM5G-45F-8CSFBGA285 | ECP5UM5G 45K | Logic 8, Pro 8, Pro 16               | Mid HW rev     |
| LFE5U-45F-6/7CSFBGA285  | ECP5U 45K    | Logic 8, Pro 16                      | Newer HW rev   |
| LFE5U-12F-7CSFBGA285    | ECP5U 12K    | Logic 8, Pro 8                       | Newest HW rev  |

Missing designs (no bitstream found): Logic 4, Logic_Pro_Top for ECP5U-45F,
Logic_Pro16_Top for ECP5U-12F. These may use the same bitstream as Logic 8
or may not have been shipped in this hardware revision combination.

## FPGA Vendor Summary

The PRD mentions "Altera, Xilinx, Lattice" but actual analysis shows:

| Vendor  | Family      | Parts              | Devices (estimated)           |
|---------|-------------|--------------------|-----------------------------|
| Xilinx  | 7-Series    | Unknown (0x4AC2)   | Likely early Logic Pro       |
| Xilinx  | Spartan-6   | XC6SLX9, XC6SLX16 | Logic 16 (legacy), Logic 4?  |
| Lattice | ECP5/ECP5UM | LFE5UM/U 12-45F    | All current-gen devices      |

The string "CYCLONE" appears in the binary but no Altera/Intel bitstreams
were found embedded. The Altera constants (`MaxSamplesPerPacket_Altera`,
`MinTriggerBufferIndex_Altera`) are in assertion messages, suggesting
Altera support exists in the codebase but may not ship in v2.4.40.

## Programming Protocol

### Key Functions (from string analysis)

1. `ProgramFpgaBitstream` -- Top-level FPGA programming entry point
2. `DownloadBitstream` -- Sends bitstream data over USB
3. `LoadBitstream` -- Higher-level loader (Logic16-specific variant exists)
4. `GetFpgaBitstreamVersion` -- Reads back programmed version
5. `ResetFpga` -- Resets FPGA before/after programming
6. `IsFpgaResponsive` -- Checks if FPGA is ready
7. `WriteConfigurationData` -- Low-level USB transfer of config data
8. `CommonDeviceInit` -- Post-programming initialization
9. `Saleae::GetFpgaImage(Saleae::MsoDeviceMetadata)` -- Image selection

### Programming Sequence (Reconstructed)

```
1. ResetFpga
   - Issues USB control transfer to reset FPGA
   - Logs "ResetFpga response"

2. ProgramFpgaBitstream
   - Selects bitstream based on device type + hardware revision
   - Log: "Programming FPGA bitstream, {} bytes"
   - Calls WriteConfigurationData in chunks

3. WriteConfigurationData(const U8*, U32)
   - "does not support lengths larger than 62/509"
   - This suggests two chunk sizes:
     - USB 2.0: max 62 bytes per control transfer (vendor request)
     - USB 3.0: max 509 bytes per control transfer
   - Or: 62 bytes for FPGA config register writes, 509 for bulk data

4. FPGA readback verification
   - "FPGA readback got {:#x} instead of 0xAA"
   - Reads a known register expecting 0xAA to confirm programming

5. IsFpgaResponsive check
   - "FPGA Programmed? {}"

6. CommonDeviceInit
   - Lattice: "Lattice PLL and SERDES reset complete"
   - Xilinx: HMCAD1100 ADC init, AD9637 init
   - FPGA register configuration via ReadRegister/WriteRegister

7. CryptoInterface authentication
   - ECDSA-based device authentication
   - Three implementations: CryptoInterfaceFpga, CryptoInterfaceI2c, CryptoInterfacePro
   - "bool RevisionDetail::UsesNewCryptoInterface()"
```

### Chunked Programming Mode

The environment variable `SALEAE_CHUNKED_FPGA_PROGRAMMING` enables a
chunked transfer mode, suggesting the default is a single large bulk write
but a fallback chunked mode exists for compatibility.

### USB Transfer Mechanism

From the libusb call analysis and string evidence:

- **Control transfers**: Used for `ResetFpga`, `WriteConfigurationData`
  (small config writes), register read/write
- **Bulk transfers**: Used for large bitstream data transfer (via EP1 OUT)
- `UsbControlSetupPacket` is the control transfer wrapper
- `LinuxUsbDevice::BulkWrite(UsbEndpoint &, uint8_t *, uint32_t, uint64_t)`
  for bulk data
- `LinuxUsbDevice::SendControlTransfer(const UsbControlSetupPacket &, ...)`
  for control commands

### FPGA Register Interface

Post-programming, the host communicates with the FPGA via register reads
and writes tunneled through the FX3:

```
FPGA ReadRegister 0x{:04X} = 0x{:08X}
FPGA WriteRegister 0x{:04x} = 0x{:08x} ({})
```

These are 16-bit addressed, 32-bit valued registers, accessed via
`FpgaDeviceFeatures::ReadRegisters` and `FpgaDeviceFeatures::WriteRegisters`.

### FPGA Image Selection

The function `Saleae::GetFpgaImage(Saleae::MsoDeviceMetadata)` selects the
correct bitstream based on device metadata. The selection criteria include:

1. **Device type**: Logic 8, Pro 8, Pro 16 (from USB PID)
2. **Hardware revision**: Read from EEPROM via `ParseBoardRevision`
   - `FPGA HW Revision: {}.{}`
   - `Device hardware revision not supported: {}`
   - Environment override: `SALEAE_REVISION_OVERRIDE`
3. **Board revision details**: `RevisionDetail` struct contains:
   - `AdcType` (AD9637 vs HMCAD1100)
   - Whether to use new crypto interface
4. **Image type**: `/fpgaImageType` field, with "ImageType 1" referenced
5. **Environment override**: `SALEAE_FPGA_IMAGE` to load from file

The `/fpgaImageType` and `/boardRevision` paths suggest these are JSON
fields in a device descriptor or configuration response.

### Voltage-Specific Bitstreams (Legacy)

For Logic 16 (legacy FX2-based device):
- `Logic16 loading {} voltage bitstream`
- Separate high/low voltage bitstreams exist

### Debug/Override Environment Variables

| Variable                         | Purpose                               |
|---------------------------------|---------------------------------------|
| `SALEAE_FPGA_IMAGE`            | Override FPGA image with file path    |
| `SALEAE_SKIP_PROGRAM_FPGA`     | Skip programming entirely             |
| `SALEAE_CHUNKED_FPGA_PROGRAMMING` | Use chunked transfer mode           |
| `SALEAE_REVISION_OVERRIDE`     | Override hardware revision detection  |

## Device Class Hierarchy

From source paths and mangled symbols:

```
GenericDevice
  LogicDevice (abstract)
    LogicAnalyzerDevice
      Logic16Device          -- FX2-based Logic 16
        LoadBitstream(bool)  -- Xilinx Spartan-6
      Logic2FpgaDevice       -- FX3-based devices (common base)
        CommonDeviceInit()
        InitHmcad1100()
        SetAd9637Enabled()
        FakeReadThread()
        LogicStudentDevice   -- Logic 4 (21a9:1003)
        LogicGraduateDevice  -- Logic 8 (21a9:1004)
        LogicProfessionalDevice -- Logic Pro 8/16 (abstract)
          LogicProfessional8Device  -- Logic Pro 8 (21a9:1005)
          LogicProfessional16Device -- Logic Pro 16 (21a9:1006)
  MsoDeviceInterface         -- MSO (out of scope)
    BuildFpga()
    BuildCryptoInterface()
    BuildAfeEeprom()
```

Source files involved:
- `devices/logic_interface/src/Logic2FpgaDevice.cpp` -- Core FPGA device
- `devices/logic_interface/src/Logic16Device.cpp` -- Legacy device
- `devices/logic_interface/src/LogicGraduateDevice.cpp` -- Logic 8
- `devices/logic_interface/src/LogicProfessionalDevice.cpp` -- Logic Pro
- `devices/logic_interface/src/LogicStudentDevice.cpp` -- Logic 4
- `devices/logic_interface/src/crypto_interface_devices.cpp` -- Auth
- `devices/usb_device/src/UsbDevice.cpp` -- USB abstraction
- `devices/firmware/src/firmware/firmware_helpers.cpp` -- FW parsing
- `devices/crypto_device/src/crypto_device/crypto_interface.cpp` -- Crypto

## CryptoInterface and Authentication

Saleae devices include a hardware authentication IC. Three variants exist:

1. **CryptoInterfaceFpga** -- Accessed through FPGA registers (newer devices)
2. **CryptoInterfaceI2c** -- Accessed via I2C (older FX2 devices)
3. **CryptoInterfacePro** -- Pro-specific variant

Operations:
- `ReadConfig32Bytes`, `ReadConfigFourBytes` -- Read config zones
- `WriteConfigFourBytes`, `WriteData32Bytes` -- Write data
- `WriteOTP32Bytes` -- Write one-time-programmable zone
- `ReadTransmission` -- Read with MAC verification
- `SignDigest` -- ECDSA P-256/P-521 signing
- `GetPublicKey`, `NewPrivateKey` -- Key management

This is likely an Microchip ATECC508A/608A or similar crypto IC. The
`CryptoInterface::BlockSize` constant and operations match the ATECC
command set. Authentication probably prevents third-party firmware from
using official Saleae software and vice versa.

## ADC Types

Two ADC families are referenced in connection with FPGA bitstream variants:

1. **HMCAD1100** (Analog Devices) -- Used in older Logic 4/8/Pro devices
   - `InitHmcad1100(Hmcad1100Settings)`
   - `SetTestPattenHmcad1100(Hmcad1100Settings::AdcTestPattern)`

2. **AD9637** (Analog Devices) -- Used in newer revisions
   - `SetAd9637Enabled(bool)`
   - `WriteAd9637Register(U16, U16)`
   - `mBoardRevision.mAdcType == RevisionDetail::AdcType::AD9637`

3. **ADS528x** (Texas Instruments) -- Lattice bitstream design names
   - All Lattice bitstreams reference ADS528x in the design name
   - ADS5282/ADS5287: 8-channel, 12-bit, pipeline ADC

The ADC type determines which FPGA bitstream variant to use, as the FPGA
data acquisition interface differs per ADC family.

## JESD Interface (Lattice ECP5 devices)

The Lattice ECP5 bitstreams use JESD204B high-speed serial interface to
connect to the ADC:

- `Lattice PLL and SERDES reset complete`
- `FPGA JESD lock=0x{:x} synced=0x{:x} error=0x{:x}`
- `JESD reset started, waiting for lock...`
- `Locked JESD after {} ms, {} retries`
- `Failed to reset JESD transceivers`
- `bitslip error detected after SERDES reset`

This confirms the newer hardware revisions use high-speed serial ADC
interfaces rather than parallel LVDS used with older Spartan-6 designs.

## Open Questions

1. What exactly is the USB vendor request code for `WriteConfigurationData`?
   Needs Ghidra analysis of the function to extract the bRequest value.

2. What is the exact format of the chunked programming protocol when
   `SALEAE_CHUNKED_FPGA_PROGRAMMING` is enabled?

3. How does the FPGA programming differ between Xilinx and Lattice devices?
   Lattice ECP5 uses SPI programming mode; Xilinx Spartan-6 uses SelectMAP.
   Both are tunneled through the FX3, but the exact USB-level sequence differs.

4. What are the two 7-series bitstreams (X[0], X[1]) actually for?
   The IDCODE 0x04AC2093 doesn't match public Xilinx docs.

5. What is the complete `RevisionDetail` struct layout?

6. Is the crypto authentication required before or after FPGA programming?
