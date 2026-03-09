# Device Class Hierarchy Analysis

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Method: C++ RTTI extraction from `.data.rel.ro` via relocation table analysis


## Summary

The Saleae binary contains two parallel class hierarchies for device
abstraction:

1. **Device hierarchy** -- models the physical device (GenericDevice down to
   LogicProfessional16Device). This hierarchy owns the state machine,
   capture lifecycle, and device-specific startup sequences.

2. **DeviceFeatures hierarchy** -- models the USB/FPGA transport layer
   capabilities. The Fx2/Fx3 branch handles EEPROM and low-level USB
   operations. The Fpga/ProFpga branch handles FPGA register I/O and
   configuration data writes.

These hierarchies are composed, not inherited from each other. A
LogicProfessionalDevice *has* a ProFpgaDeviceFeatures (and likely an
Fx3DeviceFeatures), but does not inherit from either.


## RTTI-Confirmed Class Hierarchy

### Device Hierarchy

Extracted from Itanium ABI typeinfo structures in `.data.rel.ro`, with
inheritance confirmed via `__si_class_type_info` base class pointers.

```
GenericDevice                          [ROOT, __class_type_info]
  +-- LogicAnalyzerDevice              [__si_class_type_info]
  |     +-- LogicDevice                [__si_class_type_info]
  |     |     (original Logic, 0925:3881, FX2)
  |     |
  |     +-- Logic16Device              [__si_class_type_info]
  |     |     (Logic 16, 21a9:1001, FX2LP)
  |     |
  |     +-- Logic2FpgaDevice           [__si_class_type_info]
  |     |     (common base for FX3/FPGA devices)
  |     |     +-- LogicStudentDevice   [__si_class_type_info]
  |     |     |     (Logic 4, 21a9:1003, FX3)
  |     |     |
  |     |     +-- LogicGraduateDevice  [__si_class_type_info]
  |     |     |     (Logic 8, 21a9:1004, FX3)
  |     |     |
  |     |     +-- LogicProfessionalDevice  [__si_class_type_info]
  |     |           (Logic Pro, abstract)
  |     |           +-- LogicProfessional8Device   [__si_class_type_info]
  |     |           |     (Logic Pro 8, 21a9:1005, FX3)
  |     |           |
  |     |           +-- LogicProfessional16Device  [__si_class_type_info]
  |     |                 (Logic Pro 16, 21a9:1006, FX3)
  |     |
  |     +-- LogicStudentDevice         [also inherits directly from
  |           LogicAnalyzerDevice, see note below]
  |
  (MsoDeviceInterface -- MSO, out of scope)
```

**Important correction from previous notes**: Earlier analysis hypothesized
`LogicStudentDevice` inherits from `Logic2FpgaDevice`. RTTI data proves
otherwise: `LogicStudentDevice`'s `__si_class_type_info` base pointer
resolves to `LogicAnalyzerDevice`, not `Logic2FpgaDevice`. This means
Logic 4 shares the same parent as Logic16Device and LogicDevice, not the
same parent as LogicGraduateDevice. However, the vtable structure shows
LogicStudentDevice has methods that match Logic2FpgaDevice patterns (like
`FakeReadThread`, `SetCaptureParameters`). It may be that
LogicStudentDevice implements Logic2FpgaDevice's interface through
composition rather than inheritance, or the device was refactored at
some point.

### USB Device Hierarchy

```
UsbDevice                              [ROOT, __class_type_info]
  +-- LinuxUsbDevice                   [__si_class_type_info]
```

This is separate from the device hierarchy. `UsbDevice` is the abstract USB
transport; `LinuxUsbDevice` wraps libusb. These are referenced by composition
from the device classes (e.g., `LogicProfessionalDevice` constructor takes
`std::shared_ptr<UsbDevice>`).

### DeviceFeatures Hierarchy (USB Protocol Features)

```
Fx2DeviceFeatures                      [ROOT, __class_type_info]
  +-- Fx3DeviceFeatures                [__si_class_type_info]

FpgaDeviceFeatures                     [ROOT, __class_type_info]
  +-- ProFpgaDeviceFeatures            [__si_class_type_info]
```

**Key finding**: Fx2DeviceFeatures and FpgaDeviceFeatures are separate root
classes, not related by inheritance. The Fx2/Fx3 branch handles EEPROM and
USB-level operations. The Fpga/ProFpga branch handles FPGA register I/O.

### Device Manager Hierarchy

```
DevicesManager                         [__vmi_class_type_info, uses
  +-- LinuxDevicesManager               virtual/multiple inheritance]
```

DevicesManager is the only device class using VMI (virtual/multiple
inheritance), suggesting it inherits from multiple interfaces.


## Vtable Analysis

### DeviceFeatures Virtual Methods

#### Fx2DeviceFeatures: 13 virtual methods
#### Fx3DeviceFeatures: 13 virtual methods (11 of 13 overridden)

Fx3DeviceFeatures overrides nearly every method from Fx2DeviceFeatures,
keeping only slot [0] (likely destructor) and slot [6] in common. This
is expected since FX2 and FX3 are fundamentally different USB controllers
with different register sets and protocols.

Known virtual methods (from string analysis):
- `ReadEeprom(U16, U8 *, U16)`
- `WriteEeprom(U16, std::vector<U8> &)`
- `ResetAndReEnumerate()` (Fx2 only -- FX3 re-enumerates automatically)

#### FpgaDeviceFeatures: 8 virtual methods
#### ProFpgaDeviceFeatures: 8 virtual methods (2 of 8 overridden)

ProFpgaDeviceFeatures overrides slots [1] and [3]. The 6 shared methods
are inherited unchanged.

Known virtual methods:
- `ReadRegisters(std::vector<U8> &, const S8 *)` -- likely overridden
- `WriteRegisters(std::vector<std::pair<U8, U8>> &, const S8 *)` -- likely overridden
- `WriteConfigurationData(const U8 *, U32)` -- shared
- `WriteRegisterBit(U8, U8, U8, const S8 *)` -- shared

The Pro override likely changes register addressing or bus width to
support the Pro's wider FPGA register interface.

### LogicDevice Hierarchy Virtual Methods

| Class                     | Vtable Entries | Overrides vs Parent | New Methods |
|---------------------------|---------------|---------------------|-------------|
| LogicAnalyzerDevice       | 6             | --                  | --          |
| LogicDevice               | 16            | 2 vs LAD            | 10          |
| Logic16Device             | 16            | 3 vs LAD            | 10          |
| Logic2FpgaDevice          | 6             | 2 vs LAD            | 0           |
| LogicStudentDevice        | 16            | 2 vs LAD            | 10          |
| LogicGraduateDevice       | 17            | 3 vs L2FD           | 11          |
| LogicProfessionalDevice   | 17            | 3 vs L2FD           | 11          |
| LogicProfessional8Device  | 17            | 2 vs LPD            | 0           |
| LogicProfessional16Device | 17            | 2 vs LPD            | 0           |

LAD = LogicAnalyzerDevice, L2FD = Logic2FpgaDevice, LPD = LogicProfessionalDevice

LogicProfessional8Device and LogicProfessional16Device each override
exactly 2 methods from LogicProfessionalDevice: slots [1] and [13].
These correspond to the methods that return device-specific constants
(channel count, device ID, etc.).

Known per-device virtual overrides:
- `StartSampling(DeviceSettings &)` -- each device type has its own
- `GetDeviceSettings()` -- returns device-specific capabilities
- `ComputeTransferSize(U64)` -- USB transfer size varies by device


## Per-Device Differences

### Device Type Enum

From `DeviceTypeToString()` and `GetVidPidForDevice()`:

| Type ID | VID:PID   | Enum Value | String          | Class                     |
|---------|-----------|------------|-----------------|---------------------------|
| 1       | 0925:3881 | ?          | "Logic"         | LogicDevice               |
| 4       | 21a9:1001 | ?          | "Logic16"       | Logic16Device             |
| 5       | 21a9:1003 | ?          | "Logic4"        | LogicStudentDevice        |
| 6       | 21a9:1004 | ?          | "Logic8"        | LogicGraduateDevice       |
| 7       | 21a9:1005 | ?          | "LogicPro8"     | LogicProfessional8Device  |
| 12      | 21a9:1006 | ?          | "LogicPro16"    | LogicProfessional16Device |

The Type IDs 1, 4, 5, 6, 7, 12 were confirmed in fx3-firmware.md.
Note: "Student" = Logic 4, "Graduate" = Logic 8, "Professional" = Logic Pro.

### ADC Type by Device

| Device            | ADC (Xilinx HW Rev) | ADC (Lattice HW Rev)  |
|-------------------|----------------------|-----------------------|
| Logic 4           | HMCAD1100 (8-bit)    | ADS528x (12-bit)      |
| Logic 8           | HMCAD1100 (8-bit)    | ADS528x (12-bit)      |
| Logic Pro 8       | AD9637 (12-bit)      | ADS528x (12-bit)      |
| Logic Pro 16      | AD9637 (12-bit)      | ADS528x (12-bit)      |

ADC type is determined by hardware revision via `RevisionDetail::AdcType`,
not by device model. The `InitHmcad1100()` and `SetAd9637Enabled()` paths
are in `Logic2FpgaDevice` (the common FX3/FPGA base).

### FPGA Bitstream Selection

Each device type x hardware revision gets a different FPGA bitstream.
From the Lattice bitstream design names:

| Design Name            | Devices                    |
|------------------------|----------------------------|
| Logic8_Top_ADS528x     | Logic 8 (all Lattice revs) |
| Logic_Pro_Top_ADS528x  | Logic Pro 8                |
| Logic_Pro16_Top_ADS528x| Logic Pro 16               |

No Logic 4-specific Lattice bitstream was found. Logic 4 may use the
Logic 8 bitstream with channels disabled in software.

### VGA Bandwidth Settings (LMH6518, Pro devices only)

Extracted from binary strings, these are register values for the
variable gain amplifier on Pro devices:

| Setting    | Binary |
|------------|--------|
| MHz_20     | 0b001  |
| MHz_100    | 0b010  |
| MHz_200    | 0b011  |
| MHz_350    | 0b100  |
| MHz_650    | 0b101  |
| MHz_750    | 0b110  |

These control the analog bandwidth filter in the LMH6518 VGA IC, which
is only present on Logic Pro 8 and Logic Pro 16.

### Voltage Configuration

- `LogicProfessionalDevice::SetVoltageLevel(U32)` -- Pro devices have
  programmable voltage levels (for 1.2V, 1.8V, 3.3V digital thresholds)
- `SetDigitalDerivedVoltageThresholds` -- derives thresholds from config
- `/thresholdVoltage` -- JSON field for voltage threshold setting
- Logic 16 has "voltage bitstream" selection (high/low)

### Crypto Interface Per Device

Three crypto implementations exist:
- `CryptoInterfaceI2c` -- FX2 devices (original Logic, Logic 16)
- `CryptoInterfaceFpga` -- FX3/FPGA devices (Logic 4/8)
- `CryptoInterfacePro` -- Logic Pro devices

The `RevisionDetail::UsesNewCryptoInterface()` method determines which
implementation to use based on hardware revision.

### Per-Device USB Constants

FPGA-specific constants affect packet sizes and trigger buffers:

| Constant                          | Context          |
|-----------------------------------|------------------|
| FpgaConstants::SamplesPerPacketMultiple_WithStats | All FPGA devices |
| FpgaConstants::MinSamplesPerPacket_Altera         | Altera FPGA      |
| FpgaConstants::MinSamplesPerPacket_Xilinx         | Xilinx FPGA      |
| FpgaConstants::MaxSamplesPerPacket_Altera         | Altera FPGA      |
| FpgaConstants::MaxSamplesPerPacket_Xilinx         | Xilinx FPGA      |
| MsoConstants::MinTriggerBufferIndex_Xilinx        | Xilinx triggers  |
| MsoConstants::MinTriggerBufferIndex_Altera        | Altera triggers  |
| MsoConstants::MaxTriggerBufferSamples_Xilinx      | Xilinx triggers  |
| MsoConstants::MaxTriggerBufferSamples_Altera      | Altera triggers  |


## Composition Model

From constructor signatures and factory patterns, the composition
structure is:

```
LogicProfessionalDevice
  has-a  std::shared_ptr<UsbDevice>     (via constructor)
  has-a  ProFpgaDeviceFeatures          (for FPGA register I/O)
  has-a  Fx3DeviceFeatures              (for EEPROM, USB ops)
  has-a  CryptoInterfacePro             (for device auth)
  has-a  RevisionDetail                 (ADC type, HW revision)
  has-a  DeviceSettings                 (sample rates, channels)
```

The factory is `DevicesManager::AddNewDevice(shared_ptr<UsbDevice>,
DeviceType, callback)`. The DeviceType enum selects which concrete
class to instantiate. `LinuxDevicesManager::GetAllDevices()` returns
a `map<LibUsbDeviceRef, DeviceType>` from USB enumeration.


## Device Instantiation (from shared_ptr_emplace RTTI)

The following device classes are heap-allocated via `make_shared`:
- `LogicStudentDevice` (Logic 4)
- `LogicGraduateDevice` (Logic 8)
- `LogicProfessional8Device` (Logic Pro 8)
- `LogicProfessional16Device` (Logic Pro 16)
- `Logic16Device` (Logic 16 legacy)

Note: `LogicProfessionalDevice` is NOT directly instantiated. Only its
two concrete subclasses (8 and 16) are created. This confirms
LogicProfessionalDevice is abstract.

Similarly, `Logic2FpgaDevice` and `LogicAnalyzerDevice` are never
directly instantiated -- they are abstract intermediate classes.


## Source File Mapping

From embedded `__FILE__` paths:

| Source File                                         | Classes              |
|-----------------------------------------------------|----------------------|
| `devices/base_device/src/GenericDevice.cpp`          | GenericDevice        |
| `devices/logic_interface/src/LogicDevice.cpp`        | LogicDevice          |
| `devices/logic_interface/src/Logic16Device.cpp`      | Logic16Device        |
| `devices/logic_interface/src/Logic2FpgaDevice.cpp`   | Logic2FpgaDevice     |
| `devices/logic_interface/src/LogicStudentDevice.cpp`  | LogicStudentDevice   |
| `devices/logic_interface/src/LogicGraduateDevice.cpp` | LogicGraduateDevice  |
| `devices/logic_interface/src/LogicProfessionalDevice.cpp` | LogicProfessionalDevice |
| `devices/logic_interface/src/LogicAnalyzerDevice.cpp` | LogicAnalyzerDevice  |
| `devices/usb_device/src/UsbDevice.cpp`               | UsbDevice            |
| `devices/usb_device/src/LinuxUsbDevice.cpp`          | LinuxUsbDevice       |
| `devices/devices_manager/src/DevicesManager.cpp`     | DevicesManager       |
| `devices/devices_manager/src/LinuxDevicesManager.cpp`| LinuxDevicesManager  |
| `core/saleae_base/src/device_type.cpp`               | DeviceType enum      |


## RTTI Technical Details

All typeinfo objects reside in `.data.rel.ro` (section at VA 0x2e2ea60).
RTTI name strings reside in `.rodata` (section at VA 0x34a580).

| Class                     | Typeinfo VA   | RTTI Name Offset |
|---------------------------|---------------|------------------|
| GenericDevice             | 0x2ee5200     | 0x0079d5a0       |
| UsbDevice                 | 0x2ee56a0     | 0x0079db90       |
| LinuxUsbDevice            | 0x2ee53b8     | 0x0079d8f0       |
| LogicDevice               | 0x2e48460     | 0x00473820       |
| LogicAnalyzerDevice       | 0x2e49048     | 0x00474930       |
| Logic16Device             | 0x2e47920     | 0x00472510       |
| Logic2FpgaDevice          | 0x2e47d58     | 0x00472cb0       |
| LogicStudentDevice        | 0x2e492a0     | 0x00474cd0       |
| LogicGraduateDevice       | 0x2e48720     | 0x00473d0c       |
| LogicProfessionalDevice   | 0x2e48b78     | 0x004742ec       |
| LogicProfessional8Device  | 0x2e48ba8     | 0x00474322       |
| LogicProfessional16Device | 0x2e48b90     | 0x00474306       |
| Fx2DeviceFeatures         | 0x2e49060     | 0x00474946       |
| Fx3DeviceFeatures         | 0x2e49070     | 0x0047495a       |
| FpgaDeviceFeatures        | 0x2e49088     | 0x0047496e       |
| ProFpgaDeviceFeatures     | 0x2e49098     | 0x00474983       |
| DevicesManager            | 0x2e47298     | 0x00471a90       |
| LinuxDevicesManager       | 0x2e47798     | 0x00472288       |


## Open Questions

1. **LogicStudentDevice inheritance anomaly**: RTTI says its base is
   LogicAnalyzerDevice, but it uses Logic2FpgaDevice methods (FakeReadThread,
   SetCaptureParameters). Is there a mixin, template, or composition pattern
   that explains this? Or was the inheritance refactored?

2. **RESOLVED (TASK-0045)**: FpgaDeviceFeatures slot [1] = deleting
   destructor (D0), slot [3] = WriteRegisters. Pro overrides the destructor
   (compiler-generated, semantically identical) and WriteRegisters (adds
   response verification after EP1 write). The register addressing width
   is NOT changed -- both use U8 addresses. See `re/notes/fpga-features-vtable.md`.

3. **DevicesManager VMI**: What interfaces does DevicesManager multiply
   inherit from? Likely a callback interface plus a task queue.

4. **Exact per-device channel counts**: Not extractable from strings alone.
   Need Ghidra decompilation of `GetDeviceSettings()` for each device.

5. **Performance options per device**: The three tiers (DigitalOnly,
   AnalogBasic, AnalogAdvanced) return different `SampleRateOption` vectors.
   The exact values need Ghidra decompilation.

6. **Logic 4 bitstream**: No Logic 4-specific Lattice bitstream exists.
   Does Logic 4 use the Logic 8 bitstream with channels masked?
