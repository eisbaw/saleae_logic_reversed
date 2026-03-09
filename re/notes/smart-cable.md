# Smart Cable Identification Protocol

## Summary

Smart cables are an **MSO-only feature**. They are NOT used by Logic Pro 8,
Logic Pro 16, Logic 8, or Logic 4. All smart cable code paths live under the
`MsoDevice` / `MsoDeviceInterface` / `MsoLogicAnalyzerInstrument` class
hierarchy, which is the Saleae MSO (Mixed Signal Oscilloscope) -- a separate
product from the Logic analyzer family.

This means smart cables are **out of scope** for the current RE effort, which
targets Logic Pro 16/8/4 devices.

## Key Evidence

### Exclusively MSO classes

Every smart cable function is on an MSO class:

- `Saleae::MsoDeviceInterface::GetSmartCableDetails(uint8_t)`
- `Saleae::MsoDeviceInterface::SmartCableReadPdUvdm(int, SCRegister, gsl::span<uint8_t>)`
- `Saleae::MsoDeviceInterface::SmartCableWritePdUvdm(int, SCRegister, gsl::span<const uint8_t>)`
- `Saleae::MsoDeviceInterface::BuildIoExpander_SmartCableA()`
- `Saleae::MsoDeviceInterface::BuildIoExpander_SmartCableB()`
- `Saleae::MsoFpga::SetEnabledSmartCableDigitalChannels(int, std::array<bool, 4>)`
- `Saleae::FpgaRegisters::CalculateDigitalConfigurationAddress_SmartCable(int, int)`
- `Saleae::MsoLogicAnalyzerInstrument::WriteMetadata(MsoLaSmartCableMetadata)`

The `MsoLogicAnalyzerInstrument` constructor takes `SmartCableMetadata` as a
parameter, confirming smart cables are integral to the MSO's LA instrument.

### Communication Protocol: USB-PD UVDM

Smart cables communicate via **USB Power Delivery Unstructured Vendor Defined
Messages (PD UVDM)**. This is a sideband communication channel over USB Type-C.

Key functions:
- `TransferPdUvdmSendMessage(uint8_t pdPortId, uint16_t, bool, uint16_t, gsl::span<const uint8_t>)`
- `TransferPdUvdmGetResponse(uint8_t pdPortId, uint16_t, uint8_t, PdUvdmTransfer*)`
- `SerializePdUvdmTransfer(&bb, transfer)`
- `ParsePdoData`

The cable exposes a register interface (`SCRegister`) accessed through PD UVDM
read/write operations. A register mask `SC_PD_UVDM_REGISTER_MASK` constrains
valid register addresses.

Error handling: `SmartCableNakError` class, `PdUvdm timed out waiting for
response from port {}`.

### Cable Identity Fields

The SmartCableInfo structure reports:
```
SmartCable(port={} vid=0x{:x} pid=0x{:x} bcd={} hwVersion={} fwVersion={} plug_count={} orientation={})
```

Fields:
- **vid/pid**: Vendor/Product ID (likely Saleae's USB-PD VID)
- **bcd**: BCD-encoded version
- **hwVersion, fwVersion**: Hardware and firmware revision
- **plug_count**: Number of times the cable has been inserted
- **orientation**: Cable plug orientation (USB-C is reversible)

Part numbers tracked:
- `MsoLaSmartCableBoardPartNumber` -- PCB part number
- `MsoLaSmartCableAssemblyPartNumber` -- full assembly part number

### Physical Architecture

Each smart cable port provides **4 digital channels** (from
`SetEnabledSmartCableDigitalChannels(int, std::array<bool, 4>)`).

The MSO has multiple smart cable ports (up to `MsoConstants::MaxSmartCablePortCount`).
The asar source references `new Array(5)` for instruments with a TODO comment
about "num smart cable ports + scope", suggesting 4 smart cable ports + 1
oscilloscope port.

Each smart cable connects through two IO expanders (A and B), programmed via
`InitalizeSmartCableExpanderA` / `InitalizeSmartCableExpanderB` [sic -- typo
in binary]. The cable has a controllable DAC: `Setting Smart Cable {} DAC
voltage to {} V`.

### Hot-plug Support

Smart cables support hot-plug:
```
[fw_thread {}] Smart cable plugged in on port {}
[fw_thread {}] Smart cable unplugged on port {}
Smart Cable connected, fw_port={}, port={}
Unexpceted: smart cable plugged in without orientation information!
```

Cable disconnect is tracked: `SmartCable(port={} disconnected)`.

### FPGA Register Space

Smart cables have dedicated FPGA register space:
- `CalculateDigitalConfigurationAddress_SmartCable(int portNumber, int channelNumber)`
- Bounded by `DigitalConfiguration_SmartCable_END`
- Separate from `DigitalConfiguration_DigitalDerived` (the oscilloscope's own
  digital channels)

### MSO Component Architecture

The MSO has several I2C/SPI components relevant to the cable system:
- `Eeprom_AT24C64D` -- AFE calibration EEPROM (I2C, 8KB)
- `CryptoInterfaceI2c` -- authentication/licensing chip
- IO Expanders for attenuation, DC/VGA/ADC/DAC, and smart cables
- `ADC_12QJ1600` -- 12-bit ADC
- `VGA_LMH6518` -- variable gain amplifier
- `PMIC_TPS650864` -- power management IC

Source file paths confirm MSO-specific code:
```
/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/component/eeprom_at24c64d.cpp
/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/mso_device_interface.cpp
/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/logic_analyzer_instrument.cpp
```

## Relevance to Logic Pro RE

**None.** The Logic Pro devices use a completely different architecture:

- Logic Pro uses `ProFpgaDeviceFeatures` (not `MsoDeviceInterface`)
- Logic Pro uses `Fx3DeviceFeatures::ReadEeprom` / `WriteEeprom` for I2C
  EEPROM access (calibration data), not PD UVDM
- Logic Pro does not have smart cable ports
- Logic Pro's I2C is for EEPROM calibration storage only

The `DigitalConfiguration_SmartCable` FPGA register space and PD UVDM protocol
are MSO-only constructs.

## Conclusion

Smart cables are a modular input system for the Saleae MSO product. They use
USB-PD UVDM for sideband communication, contain active electronics (IO
expanders, DAC), and each provide 4 digital channels. The MSO supports multiple
smart cable ports with hot-plug detection and orientation awareness.

This feature is entirely orthogonal to the Logic Pro/Logic 8/Logic 4 product
line and requires no further investigation for the current project scope.
