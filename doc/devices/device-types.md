# Device Types

Status: Confirmed
Source: TASK-0089 (binary analysis of DeviceTypeToString and GetVidPidForDevice)
Binary: libgraph_server_shared.so v2.4.40
Corroboration: udev rules, Ghidra decompilation, jump table decoding, lookup table extraction


## DeviceType Enum

The Saleae software uses two representations of the DeviceType enum:

- **External** (sparse): used for USB enumeration, calibration IDs, wire format
- **Internal** (compact): used for string conversion, display, device initialization

### External DeviceType (USB/Calibration)

| Value | Name        | VID:PID     | USB IC    | Status     |
|-------|-------------|-------------|-----------|------------|
| 0     | Logic       | 0925:3881   | FX2       | Supported  |
| 1     | Logic16     | 21a9:1001   | FX2LP     | Supported  |
| 2-3   | --          | --          | --        | Gap (invalid) |
| 4     | Logic4      | 21a9:1003   | FX3       | Supported  |
| 5     | Logic8      | 21a9:1004   | FX3       | Supported  |
| 6     | LogicPro8   | 21a9:1005   | FX3       | Supported  |
| 7     | LogicPro16  | 21a9:1006   | FX3       | Supported  |
| 8-11  | --          | --          | --        | Gap (invalid) |
| 12    | Mso         | --          | --        | No USB VID:PID |

### Internal DeviceType (Display/String)

| Value | String          |
|-------|-----------------|
| 0     | Unknown         |
| 1     | Logic           |
| 2     | Logic16         |
| 3     | Logic 4         |
| 4     | Logic 8         |
| 5     | Logic Pro 8     |
| 6     | Logic Pro 16    |
| 7     | Mso             |


## VID:PID Table

Confirmed by decompiling `DevicesManager::GetVidPidForDevice()`.

| VID    | PID    | Device         | External Enum |
|--------|--------|----------------|---------------|
| 0x0925 | 0x3881 | Logic          | 0             |
| 0x21a9 | 0x1001 | Logic 16       | 1             |
| 0x21a9 | 0x1003 | Logic 4        | 4             |
| 0x21a9 | 0x1004 | Logic 8        | 5             |
| 0x21a9 | 0x1005 | Logic Pro 8    | 6             |
| 0x21a9 | 0x1006 | Logic Pro 16   | 7             |

No additional VID:PIDs exist beyond these six. The MSO device (external enum 12)
does not use USB VID:PID -- it connects via SPI flash.


## Device Class Implementations

| Device         | Class Name                    | Base Features         |
|----------------|-------------------------------|-----------------------|
| Logic          | (unnamed, FX2 path)           | Fx2DeviceFeatures     |
| Logic 16       | Logic16Device                 | Fx2DeviceFeatures     |
| Logic 4        | LogicStudentDevice            | FpgaDeviceFeatures    |
| Logic 8        | LogicGraduateDevice           | FpgaDeviceFeatures    |
| Logic Pro 8    | LogicProfessionalDevice       | ProFpgaDeviceFeatures |
| Logic Pro 16   | LogicProfessional16Device     | ProFpgaDeviceFeatures |
| Mso            | MsoDevice                     | (separate hierarchy)  |

Note: "Student" and "Graduate" are internal class names, not device type names.
They do not appear in the DeviceType enum or in DeviceTypeToString output.
