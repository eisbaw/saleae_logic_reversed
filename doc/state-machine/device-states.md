# Device State Machine

Confidence: **Confirmed** (RTTI strings, assertion messages, live hardware)

## State Variant

```cpp
namespace Saleae {
    struct DSInitializing {};
    struct DSReady {};
    struct DSError {};
    using DeviceState = std::variant<DSInitializing, DSReady, DSError>;
}
```

All device types (Logic 4/8/Pro 8/Pro 16) use this same three-state
variant, managed by `GenericDevice::SetDeviceState`.

## State Diagram

```
  [DISCONNECTED]
        |
        | USB hotplug (device plugged in)
        v
  [DSInitializing]
   FW upload -> re-enumerate -> FPGA program -> ADC init -> crypto
        |                   |
        | success           | failure
        v                   v
  [DSReady] --------->  [DSError]
    |    ^     error       (terminal)
    |    |
    | StartCapture
    v    | StopCapture
  [Capturing]
  (sub-state)
```

## Transition Rules

| From | To | Allowed |
|------|-----|---------|
| DSInitializing | DSReady | Yes |
| DSInitializing | DSError | Yes |
| DSReady | DSError | Yes |
| DSReady | DSReady | Yes (reconfiguration) |
| DSError | any | **No** (terminal; requires physical reconnect) |
| DSReady | DSInitializing | **No** |

Source: assertion strings in `GenericDevice::SetDeviceState`:
- "Cannot move from Error state to non-Error state state"
- "Cannot move from Ready state to Initializing state"
- "Device tried to move from one error state to another - ignoring"

## USB Descriptor Layout (Post-Firmware)

Confirmed via live `lsusb -v` from Logic Pro 8 (21a9:1005):

| Endpoint | Address | Type | Burst | Purpose |
|----------|---------|------|-------|---------|
| EP0 | 0x00 | Control | - | Vendor requests, FW upload, register I/O |
| EP1 OUT | 0x01 | Bulk | 0 | Commands, config, FPGA bitstream data |
| EP1 IN | 0x81 | Bulk | 0 | Responses, register reads |
| EP2 IN | 0x82 | Bulk | 15 | Capture data stream (high throughput) |

- USB 3.0 SuperSpeed, 1024-byte max packet size
- Bus powered, 496mA max
- Vendor Specific class (255), 1 interface, 1 configuration
- iSerial = "SERNUM" (fixed label)

The firmware error "Endpoint count is not 4" confirms exactly 4 endpoints
(EP0 + 3 bulk) are expected after firmware load.

## Initialization Sequence

1. FX3 bootloader detected via USB hotplug
2. Firmware uploaded via vendor control transfers (bRequest=0xA0)
3. FX3 re-enumerates with Saleae VID:PID
4. Host verifies 4 endpoints present
5. `IsDeviceResponsive` check
6. FPGA bitstream programmed via bulk writes
7. `IsFpgaResponsive` check (readback expects 0xAA)
8. `CommonDeviceInit` (ADC, PLL, SERDES initialization)
9. CryptoInterface device authentication
10. State transitions to DSReady

## IPC State Types

- `Saleae::Graph::DeviceListState` -- broadcast on device connect/disconnect
- `Saleae::Graph::LogicDevice::LogicDeviceState` -- broadcast on state changes
- Both serialized as JSON over the IPC channel

## Capture Guard

Capture can only start when device is in DSReady state:
```
"Cannot start capture while device is not in Ready state"
```

Capture cannot be released while recording:
```
"can't release device while recording"
```
