# Multi-Device Differences: Logic 4, Logic 8, Logic Pro 8, Logic Pro 16

Analysis date: 2026-03-10
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Task: TASK-0073
Method: Cross-referencing confirmed findings from TASK-0044, TASK-0045, TASK-0053,
        TASK-0066, TASK-0069, TASK-0070, TASK-0071, TASK-0089, TASK-0090, and
        TASK-0027 device class hierarchy, vtable, firmware, and protocol analyses


## Summary

The Saleae driver for FX3/FPGA-based devices (Logic 4, Logic 8, Logic Pro 8,
Logic Pro 16) shares a significant amount of common infrastructure. The
differences cluster in seven areas:

1. **USB controller and firmware** -- Logic 4 is FX2LP, the rest are FX3
2. **Channel counts and capabilities** -- 4, 8, 8, or 16 channels
3. **ADC hardware and bit depth** -- varies by hardware revision, not device model
4. **Sample rates** -- fixed for Logic 4, bandwidth-gated for the rest
5. **FPGA bitstream selection** -- per-device x per-FPGA-part matrix
6. **EP1 command encryption** -- Pro devices XOR all commands, others only firmware
7. **Analog front-end** -- Pro has VGA/attenuators/DAC; non-Pro has fixed gain

The most critical finding for our Rust driver: **Logic 4 is NOT FX3-based**.
It uses a Cypress FX2LP (8051) with Intel HEX firmware, NOT the FX3 CY
firmware format. Our FX3 driver code cannot talk to a Logic 4 without a
separate FX2LP code path.


## Device Compatibility Matrix

### Hardware Identity

| Property                | Logic 4            | Logic 8            | Logic Pro 8        | Logic Pro 16       |
|-------------------------|--------------------|--------------------|--------------------|--------------------|
| VID:PID                 | 21a9:1003          | 21a9:1004          | 21a9:1005          | 21a9:1006          |
| External DeviceType     | 4                  | 5                  | 6                  | 7                  |
| Internal DeviceType     | 3                  | 4                  | 5                  | 6                  |
| Class Name              | LogicStudentDevice | LogicGraduateDevice| LogicProfessional8Device | LogicProfessional16Device |
| Base Class              | LogicAnalyzerDevice| Logic2FpgaDevice   | LogicProfessionalDevice | LogicProfessionalDevice |
| USB Controller          | **FX2LP (8051)**   | FX3 (ARM9)         | FX3 (ARM9)         | FX3 (ARM9)         |
| DeviceFeatures          | FpgaDeviceFeatures | FpgaDeviceFeatures | ProFpgaDeviceFeatures | ProFpgaDeviceFeatures |
| Calibration ID          | 4                  | 5                  | 6                  | 7                  |
| Constructor VA          | 0x1c563e7          | 0x1c3d1f6          | 0x1c44961          | 0x1c446e1          |


### Channel Configuration

| Property                | Logic 4            | Logic 8            | Logic Pro 8        | Logic Pro 16       |
|-------------------------|--------------------|--------------------|--------------------|--------------------|
| Total channels          | 4                  | 8                  | 8                  | 16                 |
| Digital channels        | 4                  | 8                  | 8                  | 16                 |
| Analog channels         | 4 (shared w/ dig.) | 8 (shared w/ dig.) | 8                  | 16 (8 analog @ USB2) |
| USB 2.0 ch. 8-15        | N/A                | N/A                | N/A                | **Digital-only**   |
| Channel capability type | Mixed (1 toggleable, 3 analog-only) | All toggleable     | All toggleable     | All toggleable (USB3), upper 8 digital-only (USB2) |


### Sample Rates

| Property                | Logic 4            | Logic 8            | Logic Pro 8        | Logic Pro 16       |
|-------------------------|--------------------|--------------------|--------------------|--------------------|
| Max digital rate        | 12 MHz             | 200 MHz            | 500 MHz            | 500 MHz            |
| Max analog rate         | 6 MHz              | 20 MHz             | 50 MHz (100 MHz USB3) | 50 MHz (100 MHz USB3) |
| Bits per ADC sample     | 8 (HMCAD1100 rev) / 12 (ADS528x rev) | 12 (stored as int16) | 12               | 12                 |
| Performance tier        | AnalogBasic        | AnalogBasic        | AnalogAdvanced     | AnalogAdvanced     |
| Rate options            | Fixed (10 presets) | Bandwidth-gated    | Bandwidth-gated    | Bandwidth-gated    |
| Analog downsampling     | Host-side, 6 rates | Host-side, 6 rates | Host-side, 6 rates | Host-side, 6 rates |
| USB2 default digital    | 12 MHz             | 100 MHz            | 100 MHz            | 100 MHz            |
| USB2 default analog     | 6 MHz              | 2.5 kHz            | 3.125 kHz          | 3.125 kHz          |
| USB3 default digital    | N/A (USB2 only)    | N/A                | 500 MHz            | 500 MHz            |
| USB3 default analog     | N/A                | N/A                | 50 MHz             | 50 MHz             |

Logic 4 has fixed sample rate options (10 presets hardcoded in the binary):

| # | Digital (Hz) | Analog Physical (Hz) | Analog Output (Hz) | Type |
|---|-------------|---------------------|--------------------|----- |
| 1 | 12,000,000  | 6,000,000           | 6,000,000          | Full rate |
| 2 | 12,000,000  | 6,000,000           | 125,000            | Downsampled |
| 3 | 12,000,000  | 6,000,000           | 5,000              | Downsampled |
| 4 | 12,000,000  | 6,000,000           | 1,000              | Downsampled |
| 5 | 12,000,000  | 6,000,000           | 100                | Downsampled |
| 6 | 12,000,000  | 6,000,000           | 10                 | Downsampled |
| 7 | 12,000,000  | --                  | --                 | Digital only |
| 8 | 6,000,000   | --                  | --                 | Digital only |
| 9 | 3,000,000   | --                  | --                 | Digital only |
|10 | 1,000,000   | --                  | --                 | Digital only |


### Firmware

| Property                | Logic 4            | Logic 8            | Logic Pro 8        | Logic Pro 16       |
|-------------------------|--------------------|--------------------|--------------------|--------------------|
| Firmware format         | **Intel HEX**      | FX3 CY image       | FX3 CY image       | FX3 CY image       |
| Firmware source         | IHex group 2 (embedded) | FX3 Pro 8 image (embedded) | FX3 Pro 16 image (embedded) | Custom file or assert |
| Firmware size           | 25,992 bytes (HEX) | 119,824 bytes      | 119,824 bytes      | 119,824 bytes      |
| Upload protocol         | FX2 vendor ctrl    | FX3 vendor ctrl    | FX3 vendor ctrl    | FX3 vendor ctrl    |
| Post-upload PID         | Stays 0x1003       | Re-enumerates 0x1005 | Re-enumerates 0x1006 | From custom FW     |
| Reseed after upload     | Yes (XOR seed)     | Yes (XOR seed)     | Yes (XOR seed)     | Yes (XOR seed)     |

Important: Logic 8 gets the Pro 8 FX3 firmware image. Logic Pro 8 gets the
Pro 16 FX3 firmware image. Logic Pro 16 requires a custom firmware file on disk
(no embedded fallback). The host identifies devices by their pre-firmware
VID:PID, not the post-upload re-enumerated PID.


### FPGA Bitstream Selection

| Property                | Logic 4            | Logic 8            | Logic Pro 8        | Logic Pro 16       |
|-------------------------|--------------------|--------------------|--------------------|--------------------|
| Xilinx Spartan-6        | XC6SLX9 (shared)   | XC6SLX9 (shared)   | --                 | --                 |
| Xilinx 7-series         | --                 | --                 | Unknown IDCODE     | Unknown IDCODE     |
| Lattice ECP5 (older)    | **None found**     | Logic8_Top_ADS528x | Logic_Pro_Top_ADS528x | Logic_Pro16_Top_ADS528x |
| Lattice ECP5 (newer)    | **None found**     | Logic8_Top_ADS528x | Logic_Pro_Top_ADS528x | Logic_Pro16_Top_ADS528x |
| Design name pattern     | --                 | Logic8_Top_*       | Logic_Pro_Top_*    | Logic_Pro16_Top_*  |
| FPGA chunk (non-Pro)    | 62 bytes           | 62 bytes           | --                 | --                 |
| FPGA chunk (Pro USB2)   | --                 | --                 | 508 bytes          | 508 bytes          |
| FPGA chunk (Pro USB3)   | --                 | --                 | 1020 bytes         | 1020 bytes         |

No Logic 4-specific Lattice ECP5 bitstream exists in the binary. Logic 4
likely uses the Logic 8 bitstream with channels masked in software, or Logic 4
hardware only comes in Xilinx Spartan-6 variants.

The FPGA bitstream is selected by a combination of device type and hardware
revision (which determines the FPGA part):

| Lattice Part                  | Logic 8 | Pro 8 | Pro 16 | HW Rev   |
|-------------------------------|---------|-------|--------|----------|
| LFE5UM-25F-8CSFBGA285        | 00      | 01    | 02     | Oldest   |
| LFE5UM5G-45F-8CSFBGA285      | 03      | 04    | 05     | Mid      |
| LFE5U-45F-6/7CSFBGA285       | 06      | --    | 07     | Newer    |
| LFE5U-12F-7CSFBGA285         | 08      | 09    | --     | Newest   |

Numbers refer to the bitstream index in `firmware/bitstream-ecp5-*.bit`.


### ADC Hardware

| Property                | Logic 4            | Logic 8            | Logic Pro 8        | Logic Pro 16       |
|-------------------------|--------------------|--------------------|--------------------|--------------------|
| ADC (Xilinx rev)        | HMCAD1100 (8-bit)  | HMCAD1100 (8-bit)  | AD9637 (12-bit)    | AD9637 (12-bit)    |
| ADC (Lattice rev)       | ADS528x (12-bit)   | ADS528x (12-bit)   | ADS528x (12-bit)   | ADS528x (12-bit)   |
| ADC interface (Xilinx)  | Parallel LVDS      | Parallel LVDS      | Parallel LVDS      | Parallel LVDS      |
| ADC interface (Lattice) | JESD204B           | JESD204B           | JESD204B           | JESD204B           |
| Init function (Xilinx)  | InitHmcad1100      | InitHmcad1100      | SetAd9637Enabled   | SetAd9637Enabled   |
| ADC range (HMCAD)       | [0, 255] unsigned  | [-2048, 2047]*     | --                 | --                 |
| ADC range (AD9637/ADS)  | [-2048, 2047]      | [-2048, 2047]      | [-2048, 2047]      | [-2048, 2047]      |
| SPI address width       | 7-bit (HMCAD)      | 7-bit (HMCAD)      | 15-bit (AD9637)    | 15-bit (AD9637)    |

*Logic 8 with HMCAD1100 stores 8-bit values in int16 containers, normalized
to the 12-bit signed range. Exact normalization formula: hypothesized as
`(uint8 << 4) - 2048`, unconfirmed.

The ADC type is determined by `RevisionDetail::AdcType`, which is parsed from
the board revision data read from EEPROM. The device model does NOT determine
the ADC -- the hardware revision does.


### EP1 Command Encryption (SimpleEncryption)

| Property                | Logic 4            | Logic 8            | Logic Pro 8        | Logic Pro 16       |
|-------------------------|--------------------|--------------------|--------------------|--------------------|
| Encryption mode         | Seed (FW only)     | Seed (FW only)     | **Seed (all cmds)**| **Seed (all cmds)**|
| XOR on normal commands  | No                 | No                 | **Yes**            | **Yes**            |
| XOR on firmware download| Yes                | Yes                | Yes                | Yes                |
| Control byte prepended  | During FW only     | During FW only     | **Always**         | **Always**         |
| LFSR seed advancement   | After FW ops only  | After FW ops only  | **After every op** | **After every op** |
| ReseedDevice required   | Yes                | Yes                | Yes                | Yes                |
| Register write verify   | No (fire-and-forget)| No (fire-and-forget)| **Yes (read-back)**| **Yes (read-back)**|

This is a critical difference for the Rust driver. Pro devices require full
SimpleEncryption implementation (control byte + XOR + LFSR) for every EP1
command and response. Non-Pro FX3 devices only need encryption during
firmware download.


### USB Speed and Transfer Parameters

| Property                | Logic 4            | Logic 8            | Logic Pro 8        | Logic Pro 16       |
|-------------------------|--------------------|--------------------|--------------------|--------------------|
| USB controller          | FX2LP (USB 2.0)    | FX3 (USB 2.0/3.0)  | FX3 (USB 2.0/3.0)  | FX3 (USB 2.0/3.0)  |
| Default forced speed    | 1 (USB 2.0)        | 1 (USB 2.0)        | 2 (USB 3.0)        | 2 (USB 3.0)        |
| Block alignment         | 4096 bytes         | 4096/16384         | 4096/16384         | 4096/16384         |
| Max transfer size       | 1,638,400 bytes    | 1,638,400 bytes    | 1,638,400 bytes    | 1,638,400 bytes    |
| Queue depth             | 128                | 128                | 128                | 128                |
| Timeout                 | 1000 ms            | 1000 ms            | 1000 ms            | 1000 ms            |
| Transfer size offset    | this + 0x1a0       | this + 0x288       | this + 0x28c       | this + 0x28c       |
| ComputeTransferSize     | Shared (slot 11)   | Shared (slot 11)   | Shared (slot 11)   | Shared (slot 11)   |


### Data Format and Deinterleaving

| Property                | Logic 4            | Logic 8            | Logic Pro 8        | Logic Pro 16       |
|-------------------------|--------------------|--------------------|--------------------|--------------------|
| Disaggregator type enum | 3                  | 1                  | 4                  | 5                  |
| Deinterleave function   | FUN_01bf2a90       | FUN_01bf21b0       | FUN_01bf1190       | FUN_01bf1190       |
| Error string            | "Logic4 data..."   | "Logic data..."    | "Logic8 data..."*  | "Logic8 data..."*  |
| Digital packet size     | 2 bytes (16-bit)   | 8 bytes (8 ch)     | 2 or 4 bytes       | 2 or 4 bytes       |
| Analog packing          | See ADC type       | 8 bytes/cycle      | 6 or 12 bytes/ch   | 6 or 12 bytes/ch   |
| Interleave slots        | N/A                | N/A                | 32                 | 16                 |
| GPIF logical width      | 8 or 16-bit        | 16 or 32-bit       | 16 or 32-bit       | 16 or 32-bit       |
| ContractReadData        | FX2-specific       | Yes (LowSpeedDivider) | Yes (LowSpeedDivider) | Yes (LowSpeedDivider) |
| Byte swap               | Yes (16-bit)       | Transform function | bswap32 (32-bit)   | bswap32 (32-bit)   |
| 12-bit ADC unpacking    | N/A (8-bit HMCAD)  | N/A (8-bit HMCAD)  | 4 samples/6 bytes or 8 samples/12 bytes | Same as Pro 8 |
| Sign extension          | N/A                | N/A                | psraw by 4 (SSE)   | psraw by 4 (SSE)   |

*The Pro deinterleave function reuses the "Logic8" error string, which is
misleading. It handles all Pro devices (types 4, 5, 6 in the disaggregator
enum).

The interleave slot count (`((*this & 0xfffffffe) == 6) * 0x10 + 0x10`)
evaluates to 32 for Pro 8 (type 6) and 16 for Pro 16 (type 7), matching
the FPGA data interleaving width.


### Analog Front-End

| Property                | Logic 4            | Logic 8            | Logic Pro 8        | Logic Pro 16       |
|-------------------------|--------------------|--------------------|--------------------|--------------------|
| VGA (LMH6518)           | No                 | No                 | **Yes**            | **Yes**            |
| Attenuator stages       | No                 | No                 | **2 (100x + 10x)** | **2 (100x + 10x)** |
| Offset DAC              | No                 | No                 | **Yes (-2.5V..2.5V)** | **Yes (-2.5V..2.5V)** |
| IO expander AFE control | No                 | No                 | **Yes**            | **Yes**            |
| Coupling (AC/DC)        | DC only            | DC only            | AC and DC          | AC and DC          |
| Voltage threshold ctrl  | No                 | No                 | Yes (1.2V, 1.8V, 3.3V) | Yes (1.2V, 1.8V, 3.3V) |
| Default voltage range   | 0V to 5V           | 0V to 5V           | -10V to +10V       | -10V to +10V       |
| Max displayable range   | -2.5V to +7.5V     | -2.5V to +7.5V     | -15V to +15V       | -15V to +15V       |
| ADC-to-V transform      | (x/255)*5          | (x/4095)*5         | (x/4095)*20 - 10   | (x/4095)*20 - 10   |


### Crypto Authentication

| Property                | Logic 4            | Logic 8            | Logic Pro 8        | Logic Pro 16       |
|-------------------------|--------------------|--------------------|--------------------|--------------------|
| Crypto interface        | CryptoInterfaceFpga| CryptoInterfaceFpga| CryptoInterfacePro | CryptoInterfacePro |
| Crypto IC               | ATECC508A/608A     | ATECC508A/608A     | ATECC508A/608A     | ATECC508A/608A     |
| Auth enforcement        | Host-software only | Host-software only | Host-software only | Host-software only |
| Rust driver needs it    | **No**             | **No**             | **No**             | **No**             |


### GPIF State Machine Selection

| Device  | Bus Width | USB 2.0 | USB 3.0 |
|---------|-----------|---------|---------|
| Logic 4 | 4-bit     | 2       | 2       |
| Logic 4 | 14-bit    | 3       | 3       |
| Logic 8 | 4-bit     | 0       | 2       |
| Logic 8 | 8-bit     | --      | 2       |
| Logic 8 | 14-bit    | 3       | 3       |
| Pro 8/16| 4-bit     | 2       | 2       |
| Pro 8/16| 14-bit    | 3       | 3       |


## What Breaks If We Use Pro 8 Code Path on Other Devices

### Logic 4 -- WILL NOT WORK

1. **Wrong USB controller**: Logic 4 is FX2LP, not FX3. The firmware upload
   protocol is completely different (Intel HEX via vendor control transfers
   vs CY image via RAM download).
2. **Wrong firmware format**: Logic 4 needs Intel HEX firmware, not FX3 CY image.
3. **Wrong class hierarchy**: LogicStudentDevice inherits from
   LogicAnalyzerDevice, not Logic2FpgaDevice. Different vtable layout.
4. **Fixed sample rates**: Logic 4 has 10 hardcoded presets, not bandwidth-gated
   dynamic rate selection.
5. **Different data format**: Logic 4 uses disaggregator type 3 with shift mode,
   completely different from Pro type 4/5.
6. **8-bit ADC**: HMCAD1100 produces 8-bit unsigned values [0, 255], not 12-bit
   signed [-2048, 2047].
7. **No VGA/attenuators**: The analog signal path is completely different.

**Verdict**: Logic 4 requires a separate driver implementation. Sharing Pro 8
code is not possible without fundamental restructuring.

### Logic 8 -- MOSTLY WORKS WITH CHANGES

1. **Same USB controller**: Both are FX3. FX3 firmware upload works.
2. **Same firmware image**: Logic 8 actually uses the Pro 8 FX3 firmware.
3. **No EP1 encryption on commands**: Logic 8 does NOT XOR EP1 commands. If we
   send XOR'd commands, the firmware will decode them wrong. Must skip XOR for
   normal commands (only use during firmware download).
4. **Register writes are fire-and-forget**: Logic 8 uses FpgaDeviceFeatures
   (base class), not ProFpgaDeviceFeatures. Register writes do NOT read back
   a response. If we try to read a response, we will get desynchronized.
5. **Different deinterleaver**: Logic 8 uses disaggregator type 1
   (FUN_01bf21b0), not type 4/5 (FUN_01bf1190). The byte layout is different.
6. **Simpler ADC**: HMCAD1100 (Xilinx rev) or ADS528x (Lattice rev). Init
   sequence differs from AD9637.
7. **No VGA/attenuator control**: No LMH6518, no attenuator stages, no DAC
   offset. AFE configuration commands would fail or be ignored.
8. **Different FPGA bitstream**: Logic8_Top_ADS528x, not Logic_Pro_Top_ADS528x.
9. **Different sample rates**: Max 200 MHz digital / 20 MHz analog vs
   500 MHz / 50 MHz.
10. **FPGA chunk size**: Always 62 bytes for Logic 8 (non-Pro) vs 508/1020
    bytes for Pro devices.

**Verdict**: Significant code can be shared (FX3 firmware upload, USB transport,
ComputeTransferSize, queue management), but the device initialization,
encryption, register I/O, deinterleaving, and AFE paths must be device-specific.

### Logic Pro 16 -- ALMOST IDENTICAL, MINOR CHANGES

1. **Same USB controller**: FX3.
2. **Different firmware**: Logic Pro 16 has no embedded firmware fallback.
   Requires a custom firmware file on disk.
3. **Same EP1 encryption**: Full SimpleEncryption XOR on all commands.
4. **Same register protocol**: ProFpgaDeviceFeatures with response verification.
5. **16 channels vs 8**: The biggest difference. Affects:
   - Channel enumeration (16 instead of 8)
   - Bandwidth allocation (more channels sharing bandwidth)
   - Interleave slot count: 16 (Pro 16) vs 32 (Pro 8)
   - USB 2.0: channels 8-15 become digital-only
6. **Different FPGA bitstream**: Logic_Pro16_Top_ADS528x, not Logic_Pro_Top_ADS528x.
7. **Same ADC hardware**: AD9637 (Xilinx) or ADS528x (Lattice).
8. **Same AFE**: VGA, attenuators, DAC all identical.
9. **Same sample rates**: 500 MHz digital / 50 MHz analog.

**Verdict**: Pro 16 support requires: (a) firmware loading from a file path,
(b) channel count parameterization (16 vs 8), (c) correct bitstream selection,
(d) interleave slot count adjustment. Most code is shared.


## Vtable Slot Comparison

From TASK-0070 relocation analysis:

| Slot | Function                    | Shared? | Notes |
|------|----------------------------|---------|-------|
| 0    | Destructor (D0)            | Per-device | Compiler-generated |
| 1    | Destructor (D1)/GetPublicKey | Per-device | Pro has GetPublicKey |
| 2    | Unknown                    | Per-device | Grad/Student differ from Pro |
| 3    | Shared helper              | Yes (0x1c4f910) | Calls vtable[12] |
| 4    | GetForcedSpeedFlag         | Yes (0x1c4e7c0) | Returns bool at +0xa0 |
| 5    | GetSomeConfig              | Yes (0x1c4fa90) | Returns bytes at +0xa8 |
| 6    | **GetUsbSpeed**            | **Per-device** | Logic4/8: default 1, Pro: default 2 |
| 7    | Unknown                    | Per-device | |
| 8    | Unknown                    | Per-device | |
| 9    | **PerformanceOptions**     | **Per-device** | Rate selection |
| 10   | **StartSampling**          | **Per-device** | Capture start |
| 11   | **ComputeTransferSize**    | **Shared** (0x1c50230) | All devices |
| 12   | Unknown (called by slot 3) | Per-device | |
| 13   | **GetDeviceSettings**      | **Per-device** | Device capabilities |
| 14   | Unknown                    | Mixed | Shared for Pro, different for Student |
| 15   | Unknown                    | Per-device | |
| 16   | Unknown                    | Per-device | Grad/Pro only, not Student |


## Rust Driver Architecture Implications

### Shared Code (all FX3 devices)

- USB device enumeration and VID:PID identification
- `ComputeTransferSize` formula (bandwidth / 240, block alignment)
- `AdjustTransferSize` (environment variable override, max cap)
- Transfer queue construction (128 depth, 1000ms timeout, 5GB max)
- LFSR seed advancement algorithm
- `ReseedDevice` protocol

### Per-Device Parameterization (trait or enum dispatch)

- Channel count (4, 8, 16)
- Max digital/analog sample rates
- FPGA bitstream selection (device type x HW revision)
- ADC initialization sequence (HMCAD1100 vs AD9637 vs ADS528x)
- Deinterleave function selection
- EP1 encryption mode (XOR all vs XOR firmware only)
- Register write protocol (fire-and-forget vs response verification)
- FPGA programming chunk size (62 vs 508/1020)
- AFE configuration (VGA/attenuators/DAC vs fixed gain)
- Voltage threshold options (Pro only)
- USB speed default (1 vs 2 when forced)

### Separate Implementation Required

- Logic 4: FX2LP firmware upload (completely different protocol)
- Logic 4: Intel HEX firmware format parsing
- Logic 4: Fixed sample rate table
- Logic 4: Disaggregator type 3 (with shift mode)

### Recommended Trait Design

```
DeviceType enum { Logic4, Logic8, LogicPro8, LogicPro16 }

trait DeviceConfig {
    fn channel_count(&self) -> u8;
    fn max_digital_rate(&self) -> u32;
    fn max_analog_rate(&self) -> u32;
    fn uses_full_encryption(&self) -> bool;
    fn fpga_chunk_size(&self, usb_speed: UsbSpeed) -> usize;
    fn register_write_verifies(&self) -> bool;
    fn default_usb_speed(&self) -> UsbSpeed;
    fn bitstream_design_name(&self) -> &str;
    fn disaggregator_type(&self) -> u8;
    fn interleave_slots(&self) -> u8;
    fn has_vga(&self) -> bool;
    fn has_attenuators(&self) -> bool;
    fn adc_bit_depth(&self, hw_revision: HwRevision) -> u8;
}
```


## Confidence Assessment

| Finding | Confidence | Source Tasks |
|---------|-----------|-------------|
| Logic 4 is FX2LP, not FX3 | **Confirmed** | TASK-0027 (firmware mapping) |
| Logic 8 uses Pro 8 FX3 firmware | **Confirmed** | TASK-0027 (GetFirmwareForDeviceType) |
| Pro devices XOR all EP1 commands | **Confirmed** | TASK-0066 (pcap validation) |
| Non-Pro FX3 only XOR during FW download | **High** | TASK-0066 (code analysis) |
| Pro uses response verification on reg writes | **Confirmed** | TASK-0045 (vtable analysis) |
| Channel counts (4/8/8/16) | **Confirmed** | TASK-0044 (constructor MOV immediates) |
| Max rates (12/200/500/500 MHz dig.) | **Confirmed** | TASK-0044 (constructor MOV immediates) |
| ADC type varies by HW rev, not device model | **Confirmed** | TASK-0044 (RevisionDetail::AdcType) |
| Deinterleaver dispatch by device type enum | **Confirmed** | TASK-0069 (decompilation) |
| Pro 16 ch. 8-15 digital-only at USB 2.0 | **Confirmed** | TASK-0071 (GetDeviceSettings) |
| FPGA chunk sizes (62/508/1020) | **Confirmed** | TASK-0071 (DownloadBitstream) |
| No Logic 4-specific Lattice bitstream | **Confirmed** | TASK-0027 (firmware extraction) |
| Crypto auth is host-software-only enforcement | **High** | crypto-authentication.md |
| ComputeTransferSize shared across all devices | **Confirmed** | TASK-0070 (vtable analysis) |
| Interleave slots: 32 (Pro 8) vs 16 (Pro 16) | **Confirmed** | TASK-0069 (decompilation) |


## Open Questions

1. **Logic 4 Lattice bitstream**: Does Logic 4 hardware exist with Lattice
   ECP5 FPGA? If so, which bitstream does it use? No Logic 4-specific Lattice
   design was found in the binary.

2. **Logic 4 GPIF width combos**: What are the valid (analog_count, gpif_width)
   pairs for LogicStudentDevice? The "No valid analog and gpif width combo
   given" error implies a validation table exists.

3. **Logic Pro 16 firmware source**: Where does the custom firmware file come
   from? Is it extracted from the .so at first run? Is it cached on disk?

4. **HMCAD1100 normalization**: How exactly does the driver normalize 8-bit
   HMCAD1100 values to the 12-bit signed range for Logic 8? `(uint8 << 4) - 2048`
   is hypothesized but unconfirmed.

5. **Exact max bitrate values**: The USB bandwidth table at ELF VA 0x5748f0
   is XOR-obfuscated and could not be read statically. These values gate
   which sample rates are available.

6. **Logic 4 FX2LP GPIF width**: The FX2LP has a physical 8-bit or 16-bit
   GPIF (not 32-bit like FX3). How does this affect the data format?

7. **Spartan-6 bitstream-to-device mapping**: Which of the 9 Spartan-6
   bitstreams map to which devices? The SLX9/SLX16 distinction and COR1
   register differences suggest different voltage configurations.


## References

- `re/notes/device-class-hierarchy.md` -- RTTI class hierarchy (TASK-0089)
- `re/notes/device-type-enum.md` -- DeviceType enum mapping (TASK-0089)
- `re/notes/logic4-8-firmware.md` -- Firmware mapping (TASK-0027)
- `re/notes/firmware-extraction.md` -- All extracted blobs (TASK-0027)
- `re/notes/sample-rate-options.md` -- SampleRateOption struct (TASK-0090)
- `re/notes/transfer-queue-sizing.md` -- ComputeTransferSize (TASK-0070)
- `re/notes/usb2-fallback.md` -- USB 2.0 vs 3.0 differences (TASK-0071)
- `re/notes/xor-obfuscation.md` -- SimpleEncryption (TASK-0066)
- `re/notes/fpga-features-vtable.md` -- FpgaDeviceFeatures vtable (TASK-0045)
- `re/notes/disaggregator-interleaving.md` -- Deinterleaving (TASK-0069)
- `re/notes/gpif-bus-width.md` -- GPIF width analysis (TASK-0053)
- `re/notes/adc-initialization.md` -- ADC init sequences
- `re/notes/afe-voltage-ranges.md` -- Analog front-end (TASK-0040)
- `re/notes/crypto-authentication.md` -- Crypto IC analysis
- `re/notes/device-settings.md` -- Device settings and JS bundle (TASK-0044)
- `doc/devices/device-capabilities.md` -- Confirmed device capabilities
- `doc/devices/device-types.md` -- Confirmed device types
