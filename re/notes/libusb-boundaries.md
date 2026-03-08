# libusb Protocol Boundaries in libgraph_server_shared.so

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40

## Key Finding: libusb Symbols Are Exported

Despite being statically linked, all 97 libusb functions are exported in the
dynamic symbol table. This means Ghidra can locate them by name, and our
TraceLibusbCalls.py script can directly find callers via XREF analysis.

The libusb code lives in the address range starting at approximately
`0x2db9f20` (libusb_ref_device) through `0x2dc1fb0` (libusb_interrupt_transfer),
a span of roughly 33KB.


## libusb Functions Used at Protocol Boundaries

### Transfer Functions (the actual USB I/O)

These are the protocol boundaries. Every call to one of these represents a
USB transaction with the device.

| Function | Evidence of Use | Purpose |
|----------|----------------|---------|
| `libusb_control_transfer` | Error strings, `SendControlTransfer` wrapper | Firmware upload, FPGA programming, EEPROM read/write, register I/O, device config |
| `libusb_bulk_transfer` | Multiple error strings for read/write/reseed | Synchronous bulk I/O for firmware/config data, reseed operation |
| `libusb_submit_transfer` | Error string, `TransferQueue` | Async bulk transfers for high-speed data streaming during capture |
| `libusb_cancel_transfer` | Error string | Cancelling in-flight async transfers on capture stop |
| `libusb_interrupt_transfer` | Symbol exported, MSO interrupt endpoint | MSO device firmware interrupt messages |

### Lifecycle Functions (device management)

| Function | Purpose |
|----------|---------|
| `libusb_init` / `libusb_init_context` | Library initialization |
| `libusb_exit` | Library cleanup |
| `libusb_get_device_list` / `libusb_free_device_list` | Device enumeration |
| `libusb_open` / `libusb_close` | Device handle management |
| `libusb_claim_interface` / `libusb_release_interface` | Interface claiming |
| `libusb_set_configuration` | USB configuration selection |
| `libusb_reset_device` | Device reset |
| `libusb_clear_halt` | Endpoint error recovery |
| `libusb_get_device_descriptor` | Device identification |
| `libusb_get_config_descriptor` | Config/endpoint enumeration |
| `libusb_get_device_speed` | USB 2.0 vs 3.0 detection |
| `libusb_get_string_descriptor_ascii` | Serial number, descriptors |
| `libusb_hotplug_register_callback` | Hot-plug detection |
| `libusb_set_auto_detach_kernel_driver` | Kernel driver management |
| `libusb_handle_events*` | Async event loop for submit_transfer |
| `libusb_dev_mem_alloc` / `libusb_dev_mem_free` | Zero-copy DMA buffers |
| `libusb_alloc_streams` / `libusb_free_streams` | USB 3.0 bulk streams |


## USB Transfer Types Used

### 1. Control Transfers (via `libusb_control_transfer`)

Wrapped by `LinuxUsbDevice::SendControlTransfer(const UsbControlSetupPacket &, ...)`.

This is the primary command channel. Control transfers are used for:

- **Firmware download** to Cypress FX3 RAM
- **FPGA bitstream programming** via FX3
- **EEPROM read/write** (device calibration, serial number)
- **Register read/write** on the FPGA
- **Device configuration** (sample rate, channel enables)
- **Reseed** operation (device responsiveness check)
- **I2C tunnel** operations (communicating with components behind FX3)
- **Crypto/authentication** operations

The `UsbControlSetupPacket` struct packs the standard USB control transfer
fields: bmRequestType, bRequest, wValue, wIndex, wLength, plus a data
pointer and timeout.

### 2. Synchronous Bulk Transfers (via `libusb_bulk_transfer`)

Used for medium-speed data movement where async is not needed:

- **Firmware image data** (the FX3 firmware binary)
- **FPGA bitstream data** (the FPGA configuration binary)
- **EEPROM bulk data** (larger reads/writes)
- **Configuration data** (capture parameters)
- **Reseed** (device responsiveness test)

Error strings reveal separate read and write paths:
- `libusb_bulk_transfer (read) failed: {}`
- `libusb_bulk_transfer (write) failed: {}`
- `libusb_bulk_transfer failure (reseed)`

### 3. Asynchronous Bulk Transfers (via `libusb_submit_transfer`)

The high-performance capture data path. This is the most critical path
for sample data streaming.

Managed by `TransferQueue::TransferQueue(libusb_device_handle &, UsbEndpoint,
uint32_t, uint32_t, uint64_t, uint64_t)` -- parameters are likely:
handle, endpoint, transfer_size, queue_depth, min_timeout, max_timeout.

Key components:
- `LinuxUsbDevice::ReadStream_QueueBuffersThread` -- dedicated thread
  that queues async transfers and processes completions
- `on_libusb_transfer_complete` -- callback for completed async transfers
- `UsbDevice::BulkReadStream` -- stream abstraction for bulk reads
- `TransferQueue` -- manages a pool of in-flight async transfers

The thread logs reveal the queue management:
- "BulkReadThread: Waiting for memory to be available and for the queue to
   have availability. queue_size={}, queue_capacity={}"
- "Starting bulk read thread, transfer_size={}, timeout={}"

### 4. Interrupt Transfers (via `libusb_interrupt_transfer`)

Used for MSO device firmware interrupt messages. Not used for Logic Pro 8/16
standard operation (only MSO).


## USB Endpoint Architecture

From string analysis, the device has exactly **4 endpoints** after firmware load:

```
"Endpoint count is not 4. The device is likely programmed with firmware
 from a different version and must be reconnected."
```

The endpoints use an `UsbEndpoint` abstraction with `.In()` and `.Out()`
accessors. Based on lsusb output of a Logic Pro 8 (21a9:1005):

| Endpoint | Direction | Type | Purpose (inferred) |
|----------|-----------|------|--------------------|
| EP1 OUT  | Host -> Device | Bulk | Commands, config, firmware data |
| EP1 IN   | Device -> Host | Bulk | Responses, register reads |
| EP2 IN   | Device -> Host | Bulk | Capture data stream (high-speed) |
| EP0      | Bidirectional  | Control | USB control transfers |

The firmware expects these exact endpoints:
- `"Expected endpoint not found: 0x{:0x}"`
- `"Device failed 'IsDeviceResponsive' test; incorrect endpoints"`

MSO devices additionally have:
- `"Failed to find MSO bulk endpoint"`
- `"Failed to find MSO interrupt endpoint"`


## Saleae C++ USB Abstraction Layer

### Class Hierarchy (from RTTI strings)

```
UsbDevice (base class)
  +-- LinuxUsbDevice (Linux USB backend, wraps libusb)

DeviceFeatures (per-device-type USB protocol)
  +-- Fx2DeviceFeatures (FX2-based: original Logic, Logic 16)
  +-- Fx3DeviceFeatures (FX3-based: Logic 4/8/Pro 8/Pro 16)
  +-- FpgaDeviceFeatures (FPGA register I/O)
  +-- ProFpgaDeviceFeatures (Pro device extensions)
```

### LinuxUsbDevice Methods (the protocol boundary layer)

These are the C++ methods that sit directly on top of libusb:

```
virtual uint32_t LinuxUsbDevice::SendControlTransfer(
    const UsbControlSetupPacket &, uint8_t *, uint32_t, uint64_t)
virtual uint32_t LinuxUsbDevice::BulkRead(
    UsbEndpoint &, uint8_t *, uint32_t, uint64_t)
virtual void LinuxUsbDevice::BulkWrite(
    UsbEndpoint &, uint8_t *, uint32_t, uint64_t)
virtual void LinuxUsbDevice::Read(
    UsbEndpoint &, uint8_t *, uint32_t, const S8 *)
virtual void LinuxUsbDevice::Write(
    UsbEndpoint &, uint8_t *, uint32_t, bool, const S8 *)
virtual void LinuxUsbDevice::ReseedDevice(UsbEndpoint &)
static void LinuxUsbDevice::ReadStream_QueueBuffersThread(
    UsbDevice::BulkReadStream &,
    TransferQueue &,
    std::atomic<ReadStreamError> &)
```

### DeviceFeatures Methods (higher-level protocol)

These methods use the LinuxUsbDevice methods and add protocol semantics:

```
-- FX2 devices --
Fx2DeviceFeatures::ReadEeprom(U16 addr, U8 *buf, U16 len)
Fx2DeviceFeatures::WriteEeprom(U16 addr, std::vector<U8> &)
Fx2DeviceFeatures::ResetAndReEnumerate()

-- FX3 devices --
Fx3DeviceFeatures::ReadEeprom(U16 addr, U8 *buf, U16 len)
Fx3DeviceFeatures::WriteEeprom(U16 addr, std::vector<U8> &)

-- FPGA register I/O --
FpgaDeviceFeatures::ReadRegisters(std::vector<U8> &addrs, const S8 *label)
FpgaDeviceFeatures::WriteRegisters(std::vector<std::pair<U8,U8>> &, const S8 *)
FpgaDeviceFeatures::WriteRegisterBit(U8 reg, U8 bit, U8 val, const S8 *)
FpgaDeviceFeatures::WriteConfigurationData(const U8 *, U32)

-- Firmware --
UsbDevice::DownloadFirmware(const Saleae::DeviceFirmwareImage &)
UsbDevice::GetDeviceCrc(U16, U16)
```


## Protocol Paths Identified (from string analysis)

### Path 1: Firmware Upload
- `UsbDevice::DownloadFirmware` -> control transfers + bulk writes
- Expects "CY" signature header
- FX3 vendor control transfers to write firmware to RAM
- Device re-enumerates after firmware load
- Firmware compiled: "Oct 25 2022 16:42:00"

### Path 2: FPGA Bitstream Programming
- `DownloadBitstream` / `ProgramFpgaBitstream` / `LoadBitstream`
- Bulk write of bitstream data to device
- `ResetFpga` before programming
- CRC verification after load (10 known CRC values)
- Supports Xilinx (primary), Lattice, and Altera variants
- `SALEAE_CHUNKED_FPGA_PROGRAMMING` env var for chunked mode
- Readback check: expects 0xAA

### Path 3: FPGA Register I/O
- `ReadRegisters` / `WriteRegisters` via control or bulk transfers
- `FPGA ReadRegister 0x{:04X} = 0x{:08X}` -- 16-bit address, 32-bit value
- `FPGA WriteRegister 0x{:04x} = 0x{:08x} ({})` -- with label
- Used for sample rate config, channel enables, trigger setup, etc.

### Path 4: Capture Data Streaming
- `ReadStream_QueueBuffersThread` -> `TransferQueue` -> `libusb_submit_transfer`
- Async bulk IN transfers on EP2
- Dynamic queue sizing based on available memory
- Transfer size configurable
- Error handling: stall detection, overflow, timeout

### Path 5: EEPROM / I2C
- Control transfers tunneled through FX3 to I2C bus
- `ReadEeprom` / `WriteEeprom` with address and size limits
- FX2: EEPROM is 256 bytes
- FX3: larger EEPROM support
- AT24C64D EEPROM on MSO devices
- I2C tunnel operations through FPGA

### Path 6: Device Configuration (Capture Setup)
- `SetCaptureParameters` -> register writes + config data writes
- ADC configuration: HMCAD1100 (Logic 4/8), AD9637 (Logic Pro)
- AFE configuration: per-channel analog front-end settings
- Digital channel configuration via FPGA registers
- Sample rate configuration
- Trigger configuration (software-only, host-side)

### Path 7: Crypto / Authentication
- `CryptoInterfaceFpga` and `CryptoInterfaceI2c`
- Uses control transfers and I2C tunnel
- Likely device authentication / anti-clone
- Separate from capture data path


## Cypress FX3 Firmware Details

The embedded FX3 firmware reveals its event handler:
```
CY_U3P_USB_EVENT_CONNECT
CY_U3P_USB_EVENT_EP0_STAT_CPLT
CY_U3P_USB_EVENT_RESET / CY_U3P_USB_EVENT_DISCONNECT
CY_U3P_USB_EVENT_RESUME
CY_U3P_USB_EVENT_SETCONF
CY_U3P_USB_EVENT_SETINTF
CY_U3P_USB_EVENT_SET_SEL
CY_U3P_USB_EVENT_SPEED
CY_U3P_USB_EVENT_SUSPEND
```

The firmware is based on the Cypress SDK `BulkSrcSink` example:
```
CyFxBulkSrcSinkApplnStart
CyFxBulkSrcSinkApplnStop
CyFxGpioInit
```

It has GPIO control (likely for FPGA programming) and an I2C path
for EEPROM/crypto chip communication.


## FPGA Bitstream Variants

10 distinct bitstream CRC values suggest multiple firmware variants:

| CRC    | Possible Purpose |
|--------|-----------------|
| 0x25C9 | ? |
| 0x36E3 | ? |
| 0x6391 | ? |
| 0x74EF | ? |
| 0x80F3 | ? |
| 0xAC6D | ? |
| 0xCE24 | ? |
| 0xD8EC | ? |
| 0xF147 | ? |
| 0xF4ED | ? |

Bitstream version: "Final Version 10.27"
Lattice design name: "Logic_Pro16_Top_ADS528x_lse_impl1.ncd"

The different CRCs likely correspond to:
- Different device types (Logic 4 vs 8 vs Pro 8 vs Pro 16)
- Different hardware revisions
- Different FPGA vendors (Xilinx vs Lattice vs Altera)
- Different voltage configurations (Logic 16 has voltage-selectable bitstream)


## Device Class Mapping

| Class | Device | USB |
|-------|--------|-----|
| LogicDevice | Original Logic | FX2, 0925:3881 |
| Logic16Device | Logic 16 | FX2LP, 21a9:1001 |
| LogicStudentDevice | Logic 4 | FX3, 21a9:1003 |
| LogicGraduateDevice | Logic 8 | FX3, 21a9:1004 |
| LogicProfessionalDevice | Logic Pro 8/16 | FX3, 21a9:1005/1006 |
| LogicProfessional8Device | Logic Pro 8 | FX3, 21a9:1005 |
| LogicProfessional16Device | Logic Pro 16 | FX3, 21a9:1006 |


## ADC Hardware

Two ADC types are referenced:
- **HMCAD1100**: Used in Logic 4, Logic 8 (TI, 8-bit, quad-channel)
- **AD9637**: Used in Logic Pro 8, Logic Pro 16 (ADI, 12-bit, quad-channel)

ADC register reads/writes are tunneled through FPGA register I/O:
```
ADC Read 0x{:x} = 0x{:x}
ADC Write 0x{:x} = 0x{:x}
```


## Source Code Organization (from embedded paths)

The original Saleae source is organized as a monorepo at `/workspace/monorepo/graph-io/`:

```
devices/
  usb_device/src/
    LinuxUsbDevice.cpp       -- libusb wrapper, protocol boundary
    UsbDevice.cpp            -- base device, firmware download
    SimpleEncryption.cpp     -- device encryption
    usb_device/memory_pool.cpp
  logic_interface/src/
    Logic2FpgaDevice.cpp     -- FPGA device management
    LogicProfessionalDevice.cpp
    LogicGraduateDevice.cpp
    LogicStudentDevice.cpp
    LogicDevice.cpp
    Logic16Device.cpp
    LogicAnalyzerDevice.cpp
    DeviceSettings.cpp
    DeviceHelpers.cpp
    BasicCryptoCommands.cpp
    crypto_interface_devices.cpp
    ComputationUtilities.cpp
    AnalogSimulationGenerator.cpp
  firmware/src/firmware/
    firmware_helpers.cpp
  devices_manager/src/
    DevicesManager.cpp
    LinuxDevicesManager.cpp
  crypto_device/src/crypto_device/
    cryptography.cpp
    crypto_interface.cpp
    crypto_interface_i2c.cpp
  base_device/src/
    GenericDevice.cpp
```


## What the Ghidra Script Will Reveal

The `TraceLibusbCalls.py` script, when run against the loaded binary, will:

1. **Find all direct callers** of `libusb_control_transfer`, `libusb_bulk_transfer`,
   `libusb_submit_transfer`, `libusb_cancel_transfer`, and `libusb_interrupt_transfer`.

2. **Extract control transfer parameters** (bmRequestType, bRequest, wValue, wIndex)
   from the assembly leading up to each call site. This will reveal the vendor
   request codes used for firmware upload, FPGA programming, register I/O, etc.

3. **Extract bulk endpoint numbers** to confirm which endpoints carry what data.

4. **Map the call graph** from high-level device operations down through the
   Saleae C++ wrapper layer to the raw libusb calls.

5. **Classify each call site** into protocol categories (firmware, FPGA, capture,
   EEPROM, register I/O, etc.).

6. **Identify the TransferQueue** async transfer management code and its callers.


## Next Steps

- [ ] Run TraceLibusbCalls.py in Ghidra to extract actual call sites and parameters
- [ ] Capture real USB traffic with usbmon to validate inferred protocol structure
- [ ] Extract vendor request codes from Ghidra analysis
- [ ] Map FPGA register addresses to functions (sample rate, channels, triggers)
- [ ] Identify the firmware image boundaries within the .so binary
- [ ] Trace the device state machine transitions
