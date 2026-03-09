# Firmware and Bitstream Extraction from libgraph_server_shared.so

Extraction date: 2026-03-09
Binary: libgraph_server_shared.so (49,762,176 bytes, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Tool: `re/scripts/extract-blobs.py`
Output: `firmware/` (gitignored)


## Summary

28 blobs extracted from the `.rodata` section of `libgraph_server_shared.so`:

| Category            | Count | Total Size   |
|---------------------|-------|-------------|
| FX3 firmware        | 3     | 239,665 B   |
| FX2 Intel HEX       | 4     | 81,445 B    |
| Xilinx 7-series     | 2     | 9,578,576 B |
| Xilinx Spartan-6    | 9     | 3,436,561 B |
| Lattice ECP5        | 10    | 3,043,411 B |
| **Total**           | **28**| **16,379,658 B (15.6 MB)** |

The firmware and bitstream data occupies approximately 33% of the 48 MB binary.


## Extraction Method

Blobs were located using:
1. **Signature scanning**: CY magic (Cypress FX3), AA995566 (Xilinx sync word),
   BB11220044 (Xilinx bus width detect), "Lattice Semiconductor" (ECP5 header),
   `:` records (Intel HEX)
2. **Format parsing**: Each blob was parsed using its native format to determine
   exact boundaries (section lengths for CY images, EOF records for Intel HEX,
   postamble markers for Lattice, inter-blob gaps for Xilinx)
3. **Cross-referencing**: Results validated against TASK-0007 (FX3 firmware
   protocol) and TASK-0008 (FPGA bitstream protocol) findings

All blobs are validated at extraction time:
- FX3 firmware: CY magic + image_type check
- Intel HEX: ASCII validity + EOF record presence
- Xilinx 7-series: bus width detect + sync word
- Xilinx Spartan-6: FFFFFFFF preamble + sync word
- Lattice ECP5: FF 00 prefix + "Lattice Semiconductor" header + CRC field


## FX3 Firmware (Cypress .img format)

| File                     | Offset       | Size    | SHA-256                                                          | Notes |
|--------------------------|-------------|---------|------------------------------------------------------------------|-------|
| fw-fx3-b2-wrapper.bin    | 0x0079E78F  | 17      | 7df4f7a123d97e2862beb6bbf94c71b5c50f9a98da780738d43391f6439e21bb | B2 SPI/I2C boot config header, VID 0x21A9 |
| fw-fx3-pro8.img          | 0x0079E7A0  | 119,824 | 428205eef55ad3e18ce4e5e9a3b595a955a2f1f3e650bc46c751b4ed0349a24e | Logic Pro 8 (21a9:1005), compiled 2022-10-25 16:42:00 |
| fw-fx3-pro16.img         | 0x007BBBB0  | 119,824 | 6b3c9b719a85c8e8493ad7129e4957b3561afb359d2bea6912a65ddcfa51190a | Logic Pro 16 (21a9:1006), compiled 2022-10-25 16:42:01 |

The two FX3 firmware images differ in only 7 bytes: USB PID (1005 vs 1006),
bMaxPower (124mA vs 200mA), compile timestamp, and checksum. They are
functionally identical and target FX3 volatile RAM only. See
`fx3-firmware-protocol.md` for full format analysis.


## Intel HEX Firmware (FX2/FX2LP, 8051)

| File           | Offset       | Size   | Records | SHA-256                                                          |
|----------------|-------------|--------|---------|------------------------------------------------------------------|
| fw-ihex-1.hex  | 0x0037E0AC  | 16,397 | 439     | 6410ba80c2284d7d5cb0a2a27bbaf8cd0339097b74ea40cd2e46f36bd1f31dd3 |
| fw-ihex-2.hex  | 0x0039743D  | 25,992 | 367     | 967e88b142d52b1ddd2a5f61fa8d841f6f5421dbebd96f6070a5abf402c0bda3 |
| fw-ihex-3.hex  | 0x003AC67E  | 15,549 | 428     | 4158efdad8a407860809207795160ea478263aeec926b97df5d3da6334fbb632 |
| fw-ihex-4.hex  | 0x003CB8ED  | 23,507 | 602     | 6ffb0158f016e018ae4418072b9728406047e56d8611981149c8d119b7a2667f |

These are firmware images for legacy FX2-based devices (Original Logic and
Logic 16). They target the 8051 ISA. The exact mapping to devices has not
been confirmed yet (see TASK-0007 open questions).


## Xilinx 7-Series Bitstreams

| File                               | Offset       | Size      | SHA-256                                                          |
|------------------------------------|-------------|-----------|------------------------------------------------------------------|
| bitstream-xilinx-7series-0.bin     | 0x007D9000  | 4,651,536 | b21e5600c2052f4df6bcf297f5d7bd700223dc381b2f5cff7235c8b6db42de33 |
| bitstream-xilinx-7series-1.bin     | 0x00C48A10  | 4,927,040 | 061e3c0756b291103676b2e22d9ec1e5b63db8a2c4faf69627c9388baf19b8b0 |

Both target IDCODE 0x04AC2093 (not in public Xilinx part lists). Raw .bin
format starting with bus width auto-detect (00 00 00 BB 11 22 00 44) then
FFFFFFFF preamble and AA995566 sync word. 32-bit configuration packets.

These likely correspond to an earlier hardware revision of Logic Pro devices.


## Xilinx Spartan-6 Bitstreams

| File                               | Offset       | Size    | Part    | SHA-256                                                          |
|------------------------------------|-------------|---------|---------|------------------------------------------------------------------|
| bitstream-spartan6-slx9-00.bin     | 0x010FB850  | 340,896 | XC6SLX9 | 565a9a9869ef6a317e5b7778a39d043c4ec11026064a4dee82161c3d95cb5657 |
| bitstream-spartan6-slx9-01.bin     | 0x0114EBF0  | 340,608 | XC6SLX9 | ba6d396e5b2b05774c80814757fd5ca28662d2d3912be7fb312cde8931839d70 |
| bitstream-spartan6-slx9-02.bin     | 0x011A1E70  | 340,608 | XC6SLX9 | 93da345aa3b4a59b6b37944ac349db8a5d2c5d4c026180ea61173c6b5b698557 |
| bitstream-spartan6-slx9-03.bin     | 0x011F50F0  | 340,608 | XC6SLX9 | bc7f7e99679bbbf483475d12e34fe8ebece33f55a9fa9df2e094249942ca2bcb |
| bitstream-spartan6-slx16-04.bin    | 0x01248370  | 464,208 | XC6SLX16| 0537bb1064e8d8ee237ada5a880896d7c00e97e7b5646fdc39821bddb2ca642f |
| bitstream-spartan6-slx16-05.bin    | 0x012B98C0  | 464,208 | XC6SLX16| 51e99160e17d4cae05916a139574f03958a3e90b7d1e3af35d7d6ed35e6c13e5 |
| bitstream-spartan6-slx9-06.bin     | 0x0132AE10  | 340,608 | XC6SLX9 | 88f4ef111c3f09798cb2bee1ddc0ad6c64b14846f39729e4a01637c4a738cec9 |
| bitstream-spartan6-slx9-07.bin     | 0x0137E090  | 340,608 | XC6SLX9 | 1326e2cefd11800df0e898f4725ffd2187cfc0234c0836bb77137b5163949425 |
| bitstream-spartan6-slx16-08.bin    | 0x013D1310  | 464,208 | XC6SLX16| 50433d047f397db854a0e525b418138d27a059ca14f25fca9d113c8eb56cc4f1 |

IDCODEs: XC6SLX9 = 0x04001093 (FLR=0x0380), XC6SLX16 = 0x04002093 (FLR=0x0430).
Raw .bin format with FFFFFFFF preamble and AA995566 sync. 16-bit configuration
packets (Spartan-6 format).

SLX9 variants come in pairs (A and B) with COR1 register differences (0x3D08
vs 0x3D00), likely for different IO voltage levels (1.8V vs 3.3V).

These are for Logic 16 (legacy) and possibly Logic 4 devices.


## Lattice ECP5 Bitstreams

| File                                | Offset       | Size    | Part                        | Design                  | CRC    | Date       | SHA-256 (first 16)      |
|-------------------------------------|-----------  |---------|-----------------------------|-----------------------  |--------|------------|--------------------------|
| bitstream-ecp5-lfe5um-25f-00.bit    | 0x01442860  | 217,760 | LFE5UM-25F-8CSFBGA285       | Logic8_Top_ADS528x      | 0xD8EC | 2022-04-12 | 20b7dd73d0ffdb18... |
| bitstream-ecp5-lfe5um-25f-01.bit    | 0x01477B00  | 254,784 | LFE5UM-25F-8CSFBGA285       | Logic_Pro_Top_ADS528x   | 0xCE24 | 2022-04-22 | b2404a6c2cf9073b... |
| bitstream-ecp5-lfe5um-25f-02.bit    | 0x014B5E40  | 358,144 | LFE5UM-25F-8CSFBGA285       | Logic_Pro16_Top_ADS528x | 0x36E3 | 2022-04-22 | 2b81d1ac43bbf1e8... |
| bitstream-ecp5-lfe5um5g-45f-03.bit  | 0x0150D540  | 284,640 | LFE5UM5G-45F-8CSFBGA285     | Logic8_Top_ADS528x      | 0xF147 | 2022-08-09 | 57528b5a01c2764f... |
| bitstream-ecp5-lfe5um5g-45f-04.bit  | 0x01552D20  | 327,120 | LFE5UM5G-45F-8CSFBGA285     | Logic_Pro_Top_ADS528x   | 0x25C9 | 2022-05-04 | de23e556c4d17f41... |
| bitstream-ecp5-lfe5um5g-45f-05.bit  | 0x015A2AF0  | 449,632 | LFE5UM5G-45F-8CSFBGA285     | Logic_Pro16_Top_ADS528x | 0x74EF | 2022-05-04 | 08680d6f74036d8c... |
| bitstream-ecp5-lfe5u-45f-06.bit     | 0x01610750  | 288,064 | LFE5U-45F-6CSFBGA285        | Logic8_Top_ADS528x      | 0xF4ED | 2023-01-03 | 160f55a6cdc5fb3e... |
| bitstream-ecp5-lfe5u-45f-07.bit     | 0x01656C90  | 387,296 | LFE5U-45F-7CSFBGA285        | Logic_Pro16_Top_ADS528x | 0x6391 | 2023-06-06 | 3a0b81a28058d5ee... |
| bitstream-ecp5-lfe5u-12f-08.bit     | 0x016B5570  | 216,272 | LFE5U-12F-7CSFBGA285        | Logic8_Top_ADS528x      | 0x80F3 | 2023-07-11 | 7252663ab8c27072... |
| bitstream-ecp5-lfe5u-12f-09.bit     | 0x016EA240  | 259,699 | LFE5U-12F-7CSFBGA285        | Logic_Pro_Top_ADS528x   | 0xAC6D | 2023-06-27 | fd702e4ebb7a104a... |

All generated by Lattice Diamond (64-bit) 3.12.1.454, all "Final Version
10.27". Bitstreams are compressed (stored sizes are much smaller than the
raw config bit count). Security: Off, Readback: Off. All in CSFBGA285
package.

### Lattice Part to Device Mapping

| Part                      | Architecture | Devices                        | HW Rev  |
|---------------------------|-------------|--------------------------------|---------|
| LFE5UM-25F-8CSFBGA285    | ECP5UM 25K  | Logic 8, Pro 8, Pro 16         | Older   |
| LFE5UM5G-45F-8CSFBGA285  | ECP5UM5G 45K| Logic 8, Pro 8, Pro 16         | Mid     |
| LFE5U-45F-6/7CSFBGA285   | ECP5U 45K   | Logic 8, Pro 16                | Newer   |
| LFE5U-12F-7CSFBGA285     | ECP5U 12K   | Logic 8, Pro 8                 | Newest  |

Design names reference TI ADS5282/ADS5287 ADC family (ADS528x suffix).

### Missing Designs

No bitstreams found for:
- Logic 4 (any FPGA variant)
- Logic_Pro_Top for ECP5U-45F (only Logic8 and Pro16 designs exist)
- Logic_Pro16_Top for ECP5U-12F (only Logic8 and Pro designs exist)


## Binary Layout

The blobs are arranged sequentially in the `.rodata` section (starts at
0x0034A580). The layout from lowest to highest offset:

```
0x0034A580  .rodata section start
   ...
0x0037E0AC  Intel HEX firmware 1
0x003820B9  (end of IHex 1)
   ...      (gap: other rodata)
0x0039743D  Intel HEX firmware 2
0x0039D9C5  (end of IHex 2)
   ...      (gap)
0x003AC67E  Intel HEX firmware 3
0x003B033B  (end of IHex 3)
   ...      (gap)
0x003CB8ED  Intel HEX firmware 4
0x003D14C0  (end of IHex 4)
   ...      (large gap: ~4.1 MB of other rodata)
0x0079E78F  B2 wrapper header
0x0079E7A0  FX3 firmware Pro 8
0x007BBBB0  FX3 firmware Pro 16
0x007D8FC0  (end of FW, FF padding follows)
0x007D9000  Xilinx 7-series bitstream 0
0x00C48A10  Xilinx 7-series bitstream 1
0x010FB850  Xilinx Spartan-6 bitstream 0
   ...      (8 more Spartan-6 bitstreams, contiguous)
0x01442860  Lattice ECP5 bitstream 0
   ...      (9 more Lattice bitstreams, contiguous)
0x017298B3  (end of last Lattice bitstream)
   ...      (remaining rodata: ~1.5 MB)
0x0172EA20  .eh_frame_hdr section start
```

The FX3 firmware and all FPGA bitstreams (7-series through Lattice) are
stored contiguously from 0x0079E78F to 0x017298B3 (approximately 15.5 MB).
The Intel HEX firmware is stored separately earlier in .rodata.


## Reproduction

To re-extract all blobs:
```
just extract-blobs
```

Or directly:
```
nix-shell --run 'python3 re/scripts/extract-blobs.py'
```

The script reads the binary from `$SALEAE_PATH` (set by shell.nix) and
writes extracted blobs to `firmware/`.


## Full SHA-256 Manifest

```
7df4f7a123d97e2862beb6bbf94c71b5c50f9a98da780738d43391f6439e21bb  fw-fx3-b2-wrapper.bin
428205eef55ad3e18ce4e5e9a3b595a955a2f1f3e650bc46c751b4ed0349a24e  fw-fx3-pro8.img
6b3c9b719a85c8e8493ad7129e4957b3561afb359d2bea6912a65ddcfa51190a  fw-fx3-pro16.img
6410ba80c2284d7d5cb0a2a27bbaf8cd0339097b74ea40cd2e46f36bd1f31dd3  fw-ihex-1.hex
967e88b142d52b1ddd2a5f61fa8d841f6f5421dbebd96f6070a5abf402c0bda3  fw-ihex-2.hex
4158efdad8a407860809207795160ea478263aeec926b97df5d3da6334fbb632  fw-ihex-3.hex
6ffb0158f016e018ae4418072b9728406047e56d8611981149c8d119b7a2667f  fw-ihex-4.hex
b21e5600c2052f4df6bcf297f5d7bd700223dc381b2f5cff7235c8b6db42de33  bitstream-xilinx-7series-0.bin
061e3c0756b291103676b2e22d9ec1e5b63db8a2c4faf69627c9388baf19b8b0  bitstream-xilinx-7series-1.bin
565a9a9869ef6a317e5b7778a39d043c4ec11026064a4dee82161c3d95cb5657  bitstream-spartan6-slx9-00.bin
ba6d396e5b2b05774c80814757fd5ca28662d2d3912be7fb312cde8931839d70  bitstream-spartan6-slx9-01.bin
93da345aa3b4a59b6b37944ac349db8a5d2c5d4c026180ea61173c6b5b698557  bitstream-spartan6-slx9-02.bin
bc7f7e99679bbbf483475d12e34fe8ebece33f55a9fa9df2e094249942ca2bcb  bitstream-spartan6-slx9-03.bin
0537bb1064e8d8ee237ada5a880896d7c00e97e7b5646fdc39821bddb2ca642f  bitstream-spartan6-slx16-04.bin
51e99160e17d4cae05916a139574f03958a3e90b7d1e3af35d7d6ed35e6c13e5  bitstream-spartan6-slx16-05.bin
88f4ef111c3f09798cb2bee1ddc0ad6c64b14846f39729e4a01637c4a738cec9  bitstream-spartan6-slx9-06.bin
1326e2cefd11800df0e898f4725ffd2187cfc0234c0836bb77137b5163949425  bitstream-spartan6-slx9-07.bin
50433d047f397db854a0e525b418138d27a059ca14f25fca9d113c8eb56cc4f1  bitstream-spartan6-slx16-08.bin
20b7dd73d0ffdb18a89716b24daac1c08c52c5882bc0293950c5784f82902ef3  bitstream-ecp5-lfe5um-25f-00.bit
b2404a6c2cf9073b6f385d7cb20df74c94f7f0cc58186ae30977f13870ee749c  bitstream-ecp5-lfe5um-25f-01.bit
2b81d1ac43bbf1e8064b079a71e63a3aa622fe5c17352c3894d49607313c1af7  bitstream-ecp5-lfe5um-25f-02.bit
57528b5a01c2764f5f57ce85dc67ae92668f54ab9ac620929d6aeb6c39039227  bitstream-ecp5-lfe5um5g-45f-03.bit
de23e556c4d17f411e9bccb1dec0d86c32bc327214e99301efcb5786d26153a3  bitstream-ecp5-lfe5um5g-45f-04.bit
08680d6f74036d8ce1b865a53be6e4b397f85210e118ed73c1f5a0e69c83e46e  bitstream-ecp5-lfe5um5g-45f-05.bit
160f55a6cdc5fb3eb643d895bfe43f5e5ed91024af97dab25ff98d00b0be8cf5  bitstream-ecp5-lfe5u-45f-06.bit
3a0b81a28058d5eebe8dee173079a4549ac75fc7a1a8486668869b23750765c9  bitstream-ecp5-lfe5u-45f-07.bit
7252663ab8c27072f33a13f7268227230cc187c0500b5ca0b114db1f7140ade1  bitstream-ecp5-lfe5u-12f-08.bit
fd702e4ebb7a104a2c7cef4b25dbba61bf0cfb0553c7755cef84aa02b0b9d5a8  bitstream-ecp5-lfe5u-12f-09.bit
```


## Open Questions

1. **Intel HEX mapping**: Which of the 4 Intel HEX images maps to which
   legacy device (Original Logic, Logic 16)? Needs Ghidra analysis of
   `IHexFirmwareImageDescriptor` path.

2. **Xilinx 7-series identity**: What FPGA part has IDCODE 0x04AC2093?
   Not in public documentation. May be a custom silicon or NDA-restricted
   variant.

3. **Logic 4 firmware**: No FX3 firmware specific to Logic 4 (21a9:1003)
   was found. The `GetFirmwareForDeviceType` function likely maps it to
   one of the Pro firmware images. Needs Ghidra confirmation.

4. **Logic 4 bitstream**: No Lattice bitstream has a "Logic4" design name.
   Logic 4 may share the Logic 8 bitstream or have channel masking in
   firmware rather than a separate FPGA design.

5. **Bitstream compression**: All Lattice ECP5 bitstreams appear to use
   Lattice-native compression (stored sizes are 40-65% of raw config bit
   count). The compression format is proprietary to Lattice.

6. **Spartan-6 variant assignment**: The COR1 register differences between
   variant A (0x3D08) and B (0x3D00) likely correspond to different IO
   voltage configurations. The exact mapping to device models is unconfirmed.
