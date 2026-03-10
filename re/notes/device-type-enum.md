# DeviceType Enum and VID:PID Mapping

Status: Confirmed (multiple independent binary analysis methods)
Source: TASK-0089
Binary: libgraph_server_shared.so v2.4.40
Methods: string analysis, RIP-relative xref scanning, Ghidra headless decompilation, jump table decoding, relocation table analysis

## Summary

There are two DeviceType representations in the binary:

1. **External DeviceType** -- used by `GetVidPidForDevice`, calibration IDs, and USB enumeration
2. **Internal DeviceType** -- used by `DeviceTypeToString`, linked-list lookups, and display

A mapping function at ELF VA `0x1c11fa0` converts external to internal.


## External DeviceType Enum

Used by `DevicesManager::GetVidPidForDevice()` and USB device identification.
Source: `DevicesManager.cpp` (line ~254)

| Value | Name           | VID:PID     | USB Controller | Notes                    |
|-------|----------------|-------------|----------------|--------------------------|
| 0     | Logic          | 0925:3881   | Cypress FX2    | Original Saleae Logic    |
| 1     | Logic16        | 21a9:1001   | Cypress FX2LP  | Legacy Logic 16          |
| 2     | (gap)          | --          | --             | Invalid, assertion fires |
| 3     | (gap)          | --          | --             | Invalid, assertion fires |
| 4     | Logic4         | 21a9:1003   | Cypress FX3    | aka LogicStudentDevice   |
| 5     | Logic8         | 21a9:1004   | Cypress FX3    | aka LogicGraduateDevice  |
| 6     | LogicPro8      | 21a9:1005   | Cypress FX3    | aka LogicProfessionalDevice |
| 7     | LogicPro16     | 21a9:1006   | Cypress FX3    | aka LogicProfessional16Device |
| 8-11  | (gap)          | --          | --             | Invalid                  |
| 12    | Mso            | --          | --             | SPI flash firmware, no USB VID:PID |

The gaps at values 2-3 and 8-11 may represent removed device types (possible
early prototypes or engineering samples). The value 12 for Mso is far from the
others, suggesting it was added much later or intentionally separated.


## Internal DeviceType Enum

Used by `DeviceTypeToString()` and the device initialization switch statements.
Source: `device_type.cpp` (line ~63)

| Value | String          | Notes                       |
|-------|-----------------|-----------------------------|
| 0     | (empty)         | Unknown -- sentinel value   |
| 1     | "Logic"         | Original Saleae Logic       |
| 2     | "Logic16"       | Note: no space              |
| 3     | "Logic 4"       |                             |
| 4     | "Logic 8"       |                             |
| 5     | "Logic Pro 8"   |                             |
| 6     | "Logic Pro 16"  |                             |
| 7     | "Mso"           |                             |


## External-to-Internal Mapping Function

ELF VA: `0x1c11fa0` (Ghidra: `0x1d11fa0`)

```c
// Pseudocode from Ghidra decompilation
uint64_t ExternalToInternal(uint32_t external_type) {
    // Valid external types: bitmask 0x10f3 = bits {0,1,4,5,6,7,12}
    if (external_type < 13 && ((0x10f3 >> external_type) & 1)) {
        return LOOKUP_TABLE[external_type];  // table at ELF VA 0x471910
    }
    throw runtime_error("Unknown device type");
}
```

Lookup table at ELF VA `0x471910`:

| External | Internal | Device          |
|----------|----------|-----------------|
| 0        | 1        | Logic           |
| 1        | 2        | Logic16         |
| 4        | 3        | Logic 4         |
| 5        | 4        | Logic 8         |
| 6        | 5        | Logic Pro 8     |
| 7        | 6        | Logic Pro 16    |
| 12       | 7        | Mso             |


## GetVidPidForDevice

ELF VA: `0x1c13df0` (Ghidra: `0x1d13df0`)
Source: `DevicesManager.cpp`

```c
// Pseudocode from Ghidra decompilation
void GetVidPidForDevice(DevicesManager* this, DeviceType type, U16* vid, U16* pid) {
    *vid = 0x21a9;  // default VID for Saleae (21a9)
    *pid = 0;
    switch (type) {
    case 0:  // Logic (original)
        *vid = 0x0925;
        *pid = 0x3881;
        return;
    case 1:  // Logic16
        *pid = 0x1001;
        return;
    case 4:  // Logic4
        *pid = 0x1003;
        return;
    case 5:  // Logic8
        *pid = 0x1004;
        return;
    case 6:  // LogicPro8
        *pid = 0x1005;
        return;
    case 7:  // LogicPro16
        *pid = 0x1006;
        return;
    default: // 2,3,8-11,12 -- Unknown, Mso, gaps
        assert(false, "Invalid device type");
    }
}
```


## DeviceTypeToString

ELF VA: `0x2ddb140` (Ghidra: `0x2edb140`)
Source: `device_type.cpp`

Uses a linked list (head at Ghidra DAT `0x030b0350`) populated at startup.
Each node is 40 bytes: `{next_ptr, unknown, string_ptr, string_len, enum_value}`.
Iterates the list comparing the stored enum_value to the parameter.

String table backing data in `.data.rel.ro` at ELF VA `0x2ee6988` (7 entries, 24 bytes each in compact form, expanded to 40-byte nodes at runtime).


## Device Class Hierarchy

From the device initialization switch (`assertion_func_2` at ELF VA `0x1aea5d0`):

| Internal Type | Subtype Field | Handler            | Device Class              |
|---------------|---------------|---------------------|---------------------------|
| 0 (Unknown)   | --            | throws error        | --                        |
| 1 (Logic)     | 0             | FUN_01bf21b0        | Fx2DeviceFeatures         |
| 2 (Logic16)   | 1             | FUN_01bf37e0        | Fx2DeviceFeatures (16ch)  |
| 3 (Logic 4)   | 2             | FUN_01bf2a90        | FpgaDeviceFeatures        |
| 4,5,6         | 3             | FUN_01bf1190        | ProFpgaDeviceFeatures     |
| 7 (Mso)       | --            | throws error        | MsoDevice (separate path) |

Implementation class names (from binary strings):
- Logic 4: `LogicStudentDevice`
- Logic 8: `LogicGraduateDevice`
- Logic Pro 8: `LogicProfessionalDevice`
- Logic Pro 16: `LogicProfessional16Device` (shares `LogicProfessionalDevice` base)
- Mso: `MsoDevice`, `MsoSpiFlash`, `MsoFpga`


## Environment Variable

`SALEAE_IGNORE_DEVICE` -- checked in `AddNewDevice` factory. When set and the
device type name matches the comma-separated value, the device is ignored.
The comparison uses `DeviceTypeToString` output (internal names).


## Notable Findings

1. **No "Graduate" or "Student" in the DeviceType enum** -- these are internal
   class names (`LogicStudentDevice`, `LogicGraduateDevice`), not device types.
   They do not appear in DeviceTypeToString.

2. **No "Logic 16" (with space)** -- the string is "Logic16" (no space).
   This is different from "Logic Pro 16" (with spaces).

3. **Gaps in the external enum** (values 2-3, 8-11) suggest removed device types.
   Possibly early hardware prototypes that were removed from production code.

4. **MSO at value 12** -- far from the main range (0-7), indicating it was added
   much later. The MSO has no USB VID:PID because it uses SPI flash firmware
   (`MsoSpiFlash::WriteFirmware`) rather than USB firmware upload.

5. **Two enum representations** is unusual. The external enum preserves backward
   compatibility (possibly with calibration server or saved files), while the
   internal enum is compact and sequential for efficient switch statements.


## Cross-Reference with Known Information

| Device         | udev VID:PID | GetVidPidForDevice | Confirmed |
|----------------|-------------|--------------------|-----------|
| Logic          | 0925:3881   | 0925:3881          | Yes       |
| Logic 16       | 21a9:1001   | 21a9:1001          | Yes       |
| Logic 4        | 21a9:1003   | 21a9:1003          | Yes       |
| Logic 8        | 21a9:1004   | 21a9:1004          | Yes       |
| Logic Pro 8    | 21a9:1005   | 21a9:1005          | Yes       |
| Logic Pro 16   | 21a9:1006   | 21a9:1006          | Yes       |

All six USB device types from the udev rules are confirmed in the binary.
No additional USB VID:PIDs were discovered.
