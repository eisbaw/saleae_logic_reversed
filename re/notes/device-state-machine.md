# Device State Machine Analysis

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (48MB, stripped, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Hardware: Logic Pro 8 (21a9:1005) on nvdk-msdlab-09


## Summary

The Saleae Logic 2 application manages device lifecycle through a typed
state machine implemented as a C++ `std::variant`. There are two layers:

1. **GenericDevice state** (all device types): `DSInitializing | DSReady | DSError`
2. **Capture-level state** (LogicDeviceNode): Ready -> Capturing -> Ready

The state machine is enforced with assertions and explicit transition guards.
State changes are broadcast to the Electron UI via IPC as
`Saleae::Graph::LogicDevice::LogicDeviceState` and
`Saleae::Graph::DeviceListState` messages.


## Device State Type System (from C++ RTTI)

### Core State Variant

```cpp
// GenericDevice.cpp
namespace Saleae {
    struct DSInitializing {};  // Device is initializing (FW upload, FPGA program)
    struct DSReady {};         // Device is ready for commands
    struct DSError {};         // Device is in error state

    using DeviceState = std::variant<DSInitializing, DSReady, DSError>;
}
```

Evidence:
- `void GenericDevice::SetDeviceState(std::variant<DSInitializing, DSReady, DSError>)`
- `void GenericDevice::UseState(std::function<std::optional<DeviceState>(DeviceState)>)`
- Source: `/workspace/monorepo/graph-io/devices/base_device/src/GenericDevice.cpp`

### MSO Device State (parallel, MSO-specific)

```cpp
namespace Saleae {
    namespace MsoInitializingState { struct MsoStateInitializing {}; };
    namespace MsoReadyState { struct MsoStateReady {}; };
    namespace MsoErrorState { struct MsoStateError {}; };

    using MsoState = std::variant<
        MsoInitializingState::MsoStateInitializing,
        MsoReadyState::MsoStateReady,
        MsoErrorState::MsoStateError
    >;
}
```

The MSO device mirrors the generic state machine with its own types.
Assertion confirms parallel tracking:
```
Saleae::holds_alternative<DSInitializing>(device_state) ==
    Saleae::holds_alternative<MsoInitializingState::State>(mso_state)
```

### Node Execution State (graph-level)

```cpp
enum NodeExecutionState {
    Stopped,
    Executing,
    Invalid
};
```

Transitions enforced:
- `Stopped -> Executing` (valid)
- `Stopped -> Invalid` (valid)
- `Stopped -> Stopped` (invalid, asserts)
- No other transitions are valid


## State Transition Rules (from assertion strings)

| From | To | Valid? | Evidence |
|------|----|----|---------|
| DSInitializing | DSReady | Yes | Normal completion of init |
| DSInitializing | DSError | Yes | Init failure |
| DSReady | DSInitializing | **No** | "Cannot move from Ready state to Initializing state" |
| DSReady | DSError | Yes | Runtime error |
| DSReady | DSReady | Yes | Implied (reconfiguration) |
| DSError | DSInitializing | Yes | Implied (reinit after error) |
| DSError | DSReady | **No** | "Cannot move from Error state to non-Error state state" |
| DSError | DSError | **No** | "Device tried to move from one error state to another - ignoring" |

Key invariant: Once in Error, a device cannot go directly to Ready. It must
be reinitialized (presumably by physical reconnection triggering a new
DSInitializing state via the hotplug path).

The Error -> non-Error prohibition plus the Error -> Error ignore means
that Error is effectively a terminal state for a device session. The device
must be physically disconnected and reconnected.


## Full Device Lifecycle

### Phase 1: USB Detection (disconnected -> detected)

```
[OS kernel]
  USB hotplug event (device plugged in)
    |
    v
[libusb_hotplug_register_callback]  -- registered at startup
    |
    v
[LinuxDevicesManager / DevicesManager]
  CheckForConnectionThread
    |
    v
  AddNewDevice(UsbDevice, DeviceType)
    - Reads VID:PID to determine DeviceType
    - Creates GenericDevice subclass
    - Device enters DSInitializing state
    |
    v
[DeviceListNode::OnConnect(DeviceInstanceId, GenericDevice)]
  - Registers state change callback
  - Broadcasts DeviceListState to UI
```

### Phase 2: Initialization (DSInitializing)

```
[GenericDevice: DSInitializing]
    |
    v
[LinuxUsbDevice::Init()]
  "LinuxUsbDevice::Init() Downloading firmware"
    |
    v
[UsbDevice::DownloadFirmware(DeviceFirmwareImage)]
  - Vendor control transfers (0x40/0xA0)
  - FX3 firmware uploaded to RAM
  - "Firmware download took ..."
    |
    v
[Device re-enumerates with new USB descriptors]
  - FX3 disconnects, reconnects with Saleae VID:PID
  - Host detects 4 endpoints
  - "Endpoint count is not 4" check
    |
    v
[IsDeviceResponsive check]
  - "Device failed 'IsDeviceResponsive' test; incorrect endpoints"
    |
    v
[TestFirmwareResponsivnessAndReseed]
  - Bulk transfer test
    |
    v
[ProgramFpgaBitstream]
  - Selects bitstream based on device type + HW revision
  - "Programming FPGA bitstream, {} bytes"
  - Bulk write of bitstream data
  - "FPGA readback got {:#x} instead of 0xAA"
  - "FPGA Programmed? {}"
    |
    v
[IsFpgaResponsive check]
    |
    v
[CommonDeviceInit]
  - ADC init (HMCAD1100 or AD9637)
  - Lattice: "Lattice PLL and SERDES reset complete"
  - JESD lock (Lattice ECP5): "Locked JESD after {} ms, {} retries"
  - FPGA register configuration
    |
    v
[CryptoInterface authentication]
  - ECDSA device verification
    |
    v
[GenericDevice: DSInitializing -> DSReady]
  "Device already initialized" (guard against double init)
```

### Phase 3: Ready (DSReady)

In DSReady state, the device accepts commands:

```
[GenericDevice: DSReady]
    |
    +-- AcquireLogicDeviceRequest -> AcquireLogicDeviceResult
    |     "acquire request for device instance id {}"
    |     "device is already acquired"
    |
    +-- SetSampleRate -> SettingResponse
    +-- EnableChannels -> SettingResponse
    +-- SetDigitalVoltageThreshold -> SettingResponse
    +-- SetAdcFullScaleGain
    +-- SetLed -> SettingResponse
    +-- GetDefaultLogicDeviceConfiguration -> Result
    |
    +-- StartCapture -> StartCaptureResponse
    |     Guard: "Cannot start capture while device is not in Ready state"
    |     On success: enters capturing sub-state
    |     Calls: SetCaptureParameters, StartSampling
    |
    +-- ReleaseDeviceRequest
    |     "can't release device while recording"
    |
    +-- SetOverrideUsb2PowerLimit (on DeviceListNode)
```

### Phase 4: Capturing (sub-state of DSReady)

```
[Capturing]
  - ReadStream_QueueBuffersThread running
  - TransferQueue managing async bulk transfers on EP2 IN
  - "Starting bulk read thread, transfer_size={}, timeout={}"
  - "BulkReadThread: Waiting for memory to be available..."
  - Data flows: EP2 IN -> TransferQueue -> BulkReadStream -> OnDataBuffer
    |
    +-- StopCapture -> StopCaptureResponse
    |     Cancels in-flight transfers
    |     "Done with usb stream, {} successful transfers"
    |     Returns to non-capturing ready state
    |
    +-- Error during capture:
          OnWrite error callback (LogicError)
          -> DSError transition
```

### Phase 5: Error (DSError)

```
[GenericDevice: DSError]
  "Device is not responsive, please reconnect the device. error: "
  "Device unresponsive: "
  "Device unresponsive. Error: "
  "USB Device Init Error: {}"
  "Cannot use interface, MsoDevice is in error state: {}"
  "Measurer is in an error state"
    |
    +-- Cannot transition to DSReady directly
    +-- Cannot transition to another DSError
    +-- Only recovery: physical disconnect/reconnect
```

### Phase 6: Disconnect

```
[USB hotplug: device removed]
    |
    v
[DeviceListNode]
  OnDisconnect callback
  Removes device from connectedDevices
  Broadcasts updated DeviceListState to UI
```


## USB Descriptor Layout (Live Hardware Data)

### Logic Pro 8 (21a9:1005) -- After Firmware Load

Captured via `lsusb -v -d 21a9:` from nvdk-msdlab-09 on 2026-03-09.
The device was in post-firmware state (Saleae software had been run previously;
firmware persists in RAM until power cycle).

```
Bus 002 Device 003: ID 21a9:1005 Saleae, Inc. Logic Pro 8

Device Descriptor:
  bLength                18
  bDescriptorType         1
  bcdUSB               3.00          # USB 3.0 SuperSpeed
  bDeviceClass            0           # Per-interface
  bDeviceSubClass         0
  bDeviceProtocol         0
  bMaxPacketSize0         9           # 2^9 = 512 bytes (USB 3.0)
  idVendor           0x21a9          # Saleae, Inc.
  idProduct          0x1005          # Logic Pro 8
  bcdDevice            0.00          # No device revision in descriptor
  iManufacturer           1 Saleae
  iProduct                2 Logic Pro
  iSerial                 4 SERNUM    # Fixed label, not actual serial
  bNumConfigurations      1

Configuration Descriptor:
  bLength                 9
  bDescriptorType         2
  wTotalLength       0x0039           # 57 bytes total
  bNumInterfaces          1
  bConfigurationValue     1
  iConfiguration          0
  bmAttributes         0x80           # Bus Powered
  MaxPower              496mA         # ~500mA bus power budget

  Interface Descriptor:
    bLength                 9
    bDescriptorType         4
    bInterfaceNumber        0
    bAlternateSetting       0
    bNumEndpoints           3
    bInterfaceClass       255         # Vendor Specific
    bInterfaceSubClass      0
    bInterfaceProtocol      0
    iInterface              0

    Endpoint Descriptor: EP 1 OUT (Bulk)
      bEndpointAddress     0x01
      bmAttributes            2       # Bulk
      wMaxPacketSize     0x0400       # 1024 bytes
      bInterval               0
      bMaxBurst               0       # No burst

    Endpoint Descriptor: EP 1 IN (Bulk)
      bEndpointAddress     0x81
      bmAttributes            2       # Bulk
      wMaxPacketSize     0x0400       # 1024 bytes
      bInterval               0
      bMaxBurst               0       # No burst

    Endpoint Descriptor: EP 2 IN (Bulk)
      bEndpointAddress     0x82
      bmAttributes            2       # Bulk
      wMaxPacketSize     0x0400       # 1024 bytes
      bInterval               0
      bMaxBurst              15       # USB 3.0 burst of 16 packets

Binary Object Store Descriptor:
  USB 2.0 Extension:
    bmAttributes   0x00000002         # LPM Supported
  SuperSpeed USB Device Capability:
    bmAttributes         0x00
    wSpeedsSupported   0x000e         # Full + High + SuperSpeed
    bFunctionalitySupport   3         # Lowest speed = SuperSpeed
    bU1DevExitLat          10 us
    bU2DevExitLat        2047 us

Device Status:     0x000c
  Bus Powered
  U1 Enabled
  U2 Enabled
```

### Endpoint Purposes (confirmed)

| Endpoint | Address | Direction | Burst | Purpose |
|----------|---------|-----------|-------|---------|
| EP0      | 0x00    | Bidi      | -     | USB control transfers (FW upload, register I/O) |
| EP1 OUT  | 0x01    | Host->Dev | 0     | Commands, config data, FPGA bitstream |
| EP1 IN   | 0x81    | Dev->Host | 0     | Responses, register reads, reseed |
| EP2 IN   | 0x82    | Dev->Host | 15    | **High-speed capture data stream** |

EP2 IN has bMaxBurst=15 (16 packets burst), confirming it is the high-throughput
capture data endpoint. At USB 3.0 SuperSpeed with 1024-byte packets and 16x
burst, this gives a theoretical maximum of 16 * 1024 = 16,384 bytes per
service interval.

EP1 OUT and EP1 IN have bMaxBurst=0 (single packet), appropriate for
low-frequency command/response traffic.

### Descriptor State at Different Lifecycle Phases

| Phase | VID:PID | Endpoints | bcdUSB | Notes |
|-------|---------|-----------|--------|-------|
| FX3 Bootloader | 04b4:00f3 | EP0 only | 2.00/3.00 | Cypress default bootloader descriptors |
| After FW Upload | 21a9:1005 | EP0 + 3 bulk | 3.00 | Saleae descriptors from firmware |
| After FPGA Program | 21a9:1005 | Same | 3.00 | No descriptor change; FPGA readiness is via register |

The FX3 bootloader VID:PID (04b4:00f3) is the Cypress default. After firmware
upload, the FX3 re-enumerates with the Saleae VID:PID. The FPGA programming
does NOT cause a USB re-enumeration -- the endpoints are already present
from the firmware, and FPGA state is checked via register reads.

### USB Speed Detection

The host detects USB speed via `libusb_get_device_speed()`:
- "USB Full Speed" (12 Mbps) -- unlikely but handled
- "USB High Speed" (480 Mbps) -- USB 2.0
- "USB Super Speed" (5 Gbps) -- USB 3.0

The detected speed affects:
- Maximum sample rates (bandwidth limited)
- Power budget (USB 2.0 = 500mA, USB 3.0 = 900mA)
- `SetOverrideUsb2PowerLimit` -- allows overriding USB 2.0 power limit
- WriteConfigurationData chunk sizes (62 bytes USB 2.0, 509 bytes USB 3.0)

### Firmware Descriptor Differences (Pro 8 vs Pro 16)

From firmware binary analysis (re/notes/fx3-firmware-protocol.md):

| Field | Pro 8 (FW1) | Pro 16 (FW2) |
|-------|-------------|--------------|
| idProduct | 0x1005 | 0x1006 |
| bMaxPower | 62 (124mA) | 100 (200mA) |
| Everything else | Identical | Identical |

Both firmwares contain two USB device descriptors:
- USB 2.0 descriptor (bcdUSB=0x0210)
- USB 3.0 descriptor (bcdUSB=0x0300)


## IPC State Messages

### DeviceListState (device discovery level)

Serialized type: `Saleae::Graph::DeviceListState`

JSON fields:
- `/connectedDevices` -- array of connected device descriptions
- `/deviceId` -- unique device identifier
- `/deviceType` -- enum value (1=Logic, 4=Logic16, 5=Logic4, 6=Logic8, 7=Pro8, 12=Pro16)
- `/lastConnectionError` -- error from last failed connection attempt
- `/hardwareRevision` -- hardware revision string

### LogicDeviceState (per-device level)

Serialized type: `Saleae::Graph::LogicDevice::LogicDeviceState`

This is broadcast when device state changes (acquisition, configuration
changes, capture start/stop). The state is sent as an IPC response or
pipe broadcast.

JSON fields (inferred from serialization code):
- `/state` -- current device state
- `/channelStates` -- per-channel configuration
- `/analogChannels` -- enabled analog channels
- `/digitalChannels` -- enabled digital channels
- `/sampleRate` -- current sample rate
- `/capturing` -- whether capture is active
- `/errorState` -- error description if in error

### Device Commands (request handlers)

| Command | Type | Guard | Transition |
|---------|------|-------|------------|
| AcquireLogicDeviceRequest | Request | None (but device must be connected) | None (acquisition is app-level) |
| ReleaseDeviceRequest | Request | "can't release device while recording" | None |
| StartCapture | Request | "Cannot start capture while device is not in Ready state" | Ready -> Capturing |
| StopCapture | Request | Capture must be active | Capturing -> Ready |
| SetSampleRate | Request | Device must be acquired | None (config change) |
| EnableChannels | Request | Device must be acquired | None (config change) |
| SetDigitalVoltageThreshold | Request | Device must be acquired | None (config change) |
| SetAdcFullScaleGain | Request | Device must be acquired | None (config change) |
| SetLed | Request | Device must be acquired | None (LED control) |


## FX3 Firmware USB Event Handler

The FX3 firmware (embedded in the .so) handles these USB events:

| Event | When | Firmware Action |
|-------|------|-----------------|
| CY_U3P_USB_EVENT_CONNECT | USB cable connected | Start application |
| CY_U3P_USB_EVENT_DISCONNECT | Cable removed | Stop application |
| CY_U3P_USB_EVENT_RESET | Bus reset | Reset application state |
| CY_U3P_USB_EVENT_SUSPEND | Host suspends device | Enter low power |
| CY_U3P_USB_EVENT_RESUME | Host resumes device | Resume operation |
| CY_U3P_USB_EVENT_SETCONF | Host sends SET_CONFIGURATION | Configure endpoints |
| CY_U3P_USB_EVENT_SETINTF | Host sends SET_INTERFACE | Interface alt setting |
| CY_U3P_USB_EVENT_SET_SEL | Host sends SET_SEL | USB 3.0 latency config |
| CY_U3P_USB_EVENT_SPEED | Speed negotiation complete | Log speed |
| CY_U3P_USB_EVENT_EP0_STAT_CPLT | EP0 status stage complete | Vendor request done |

The firmware source is `LogicPro.c` with entry point `LogicProApplicationInit`
and main loop `LogicProEventLoop` / `LogicProThread_Entry`.


## Smart Cable and Power Detection

The firmware reports hardware events through the FX3 firmware interrupt
message system:

```
[fw_thread {}] Smart cable plugged in on port {}
[fw_thread {}] Smart cable unplugged on port {}
[fw_thread {}] Wall power: plugged, before PD contract. Event count {}
[fw_thread {}] Wall power: plugged, but PD power is insufficient. Event count {}
[fw_thread {}] Wall power: unplugged. Event count {}
[fw_thread {}] PC power unplugged. word={:#X}
```

These are received by the host as `FirmwareInterruptMessage` items in the
MsoDevice mailbox (variant type), suggesting the MSO and Logic Pro devices
share some firmware interrupt infrastructure.


## State Machine Diagram (ASCII)

```
                    USB Hotplug
                        |
                        v
    +-------------------------------------------+
    |           DISCONNECTED                     |
    |    (No USB device present)                 |
    +-------------------------------------------+
                        |
                        | device plugged in
                        | libusb_hotplug_callback
                        v
    +-------------------------------------------+
    |           DSInitializing                   |
    |                                            |
    |  1. FX3 bootloader detected               |
    |  2. Firmware uploaded (vendor 0xA0)        |
    |  3. Device re-enumerates (21a9:100x)       |
    |  4. 4 endpoints verified                   |
    |  5. IsDeviceResponsive check               |
    |  6. FPGA bitstream programmed              |
    |  7. IsFpgaResponsive check                 |
    |  8. CommonDeviceInit (ADC, PLL, etc.)      |
    |  9. CryptoInterface authentication         |
    +-------------------------------------------+
          |                          |
          | success                  | failure
          v                          v
    +------------------+    +------------------+
    |     DSReady      |    |     DSError      |
    |                  |    |                  |
    | - Accept cmds    |    | - Terminal state |
    | - Configure      |    | - No recovery   |
    | - Start/Stop     |    |   without        |
    |   capture        |    |   reconnect      |
    +------------------+    +------------------+
      |      ^      |              ^
      |      |      |              |
      |      |      | runtime      |
      |      |      | error        |
      |      |      +----->--------+
      |      |
      | StartCapture
      |      | StopCapture
      v      |
    +------------------+
    |    Capturing      |
    | (sub-state of     |
    |  DSReady)         |
    |                  |
    | - EP2 IN bulk    |
    |   streaming      |
    | - TransferQueue  |
    |   active         |
    +------------------+
```


## Key Implementation Details

### GenericDevice::SetDeviceState

This is the central state transition function. It takes a `DeviceState`
variant and applies transition rules:

1. Error -> non-Error: blocked ("Cannot move from Error state to non-Error state state")
2. Ready -> Initializing: blocked ("Cannot move from Ready state to Initializing state")
3. Error -> Error: ignored ("Device tried to move from one error state to another")
4. Success: broadcasts new state via callback

### DeviceListNode::OnConnect

Entry point when a new device is detected. This function:
1. Receives `DeviceInstanceId` and `shared_ptr<GenericDevice>`
2. Registers a state change callback on the device
3. The callback (lambda $\_5) receives `variant<DSInitializing, DSReady, DSError>`
4. Updates `DeviceListState` and broadcasts to UI

### LogicDeviceNode::HandleStartCapture

This is the capture initiation handler. It:
1. Checks device is in Ready state
2. Calls `SetCaptureParameters(DeviceSettings&)` on the device
3. Calls `StartSampling(DeviceSettings&)`
4. Registers data buffer callback (lambda $\_7) for `OnDataBuffer`
5. Registers error callback (lambda $\_8) for `LogicError`
6. Returns `StartCaptureResponse`

### TransferQueue (capture data path)

During capture, the `TransferQueue` manages a pool of asynchronous USB
bulk transfers:
- Constructor: `TransferQueue(libusb_device_handle&, UsbEndpoint, uint32_t, uint32_t, uint64_t, uint64_t)`
- Runs on `ReadStream_QueueBuffersThread`
- Dynamic queue sizing based on memory availability
- Callback: `on_libusb_transfer_complete`
- Completion: "Done with usb stream, {} successful transfers"


## Confidence Assessment

| Finding | Confidence | Sources |
|---------|-----------|---------|
| Three-state variant (DSInitializing/DSReady/DSError) | **Confirmed** | RTTI strings, assertion messages, mangled symbols |
| Transition rules (Error terminal, Ready !-> Initializing) | **Confirmed** | Assertion message strings in binary |
| USB descriptor layout (3 endpoints + EP0) | **Confirmed** | Live lsusb from hardware |
| EP2 IN is capture data endpoint (burst=15) | **Confirmed** | Live lsusb + binary string analysis |
| Firmware re-enumeration changes VID:PID | **High** | Binary analysis + Cypress FX3 protocol docs |
| FPGA programming does NOT re-enumerate | **High** | No re-enumeration strings near FPGA code; readback check instead |
| Capturing is a sub-state of DSReady | **High** | StartCapture guard string + no separate DS variant |
| FX3 bootloader VID:PID = 04b4:00f3 | **Hypothesis** | Standard Cypress default; needs live capture to confirm |


## Open Questions

1. What VID:PID does the FX3 bootloader expose before firmware upload?
   Standard Cypress is 04b4:00f3, but Saleae may have customized it.
   Need to power-cycle the device and capture descriptors BEFORE running
   Logic 2 software.

2. Does the device expose different USB descriptors on USB 2.0 vs USB 3.0?
   The firmware contains two device descriptors (bcdUSB 0x0210 and 0x0300).
   Need to test with a USB 2.0 port to see the difference.

3. What is the exact format of `LogicDeviceState` JSON broadcast?
   We have the field paths but not a complete example. Need to intercept
   the IPC message or run Ghidra on the serialization function.

4. How does `iSerial = "SERNUM"` work? Is the actual serial number
   read from EEPROM and reported differently, or is "SERNUM" literally
   the only serial available at the USB level?

5. Are there additional sub-states within DSInitializing? The MSO code
   has `Initialize_PreWallPowerDetected` and `Initialize_PostWallPowerDetected`,
   suggesting at least 2 sub-phases of initialization for MSO devices.
   Logic devices may have similar internal phases.


## New Leads (backlog candidates)

- The `SetOverrideUsb2PowerLimit` command suggests USB 2.0 power negotiation
  is sometimes insufficient and needs a manual override.
- The `FirmwareInterruptMessage` system is shared between MSO and Logic Pro
  devices -- worth investigating for smart cable detection.
- `NodeExecutionState` (Stopped/Executing/Invalid) is a separate state
  machine at the graph processing level, separate from device state.
- The `ConnectionError` variant in `DevicesManager` handles USB connection
  failures, separate from `DSError`.
