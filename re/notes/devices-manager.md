# DevicesManager and LinuxDevicesManager

Analysis date: 2026-03-10
Binary: libgraph_server_shared.so v2.4.40
Source: TASK-0084
Method: objdump disassembly, RIP-relative xref scanning, Ghidra headless decompilation, string/symbol analysis

## Summary

The DevicesManager subsystem handles USB device enumeration, hotplug detection,
device factory dispatch, and device lifecycle management. There are two key classes:

- **DevicesManager** -- abstract base class owning device state, VID:PID dispatch,
  simulation device support, and the AddNewDevice factory
- **LinuxDevicesManager** -- Linux-specific subclass implementing USB enumeration
  via libusb (GetAllDevices, CheckForConnectionThread, AddNewDevice)

Source paths leaked via assertions:
- `/workspace/monorepo/graph-io/devices/devices_manager/src/DevicesManager.cpp`
- `/workspace/monorepo/graph-io/devices/devices_manager/src/LinuxDevicesManager.cpp`


## Class Structure

### DevicesManager

Type: `enable_shared_from_this<DevicesManager>` (ref-counted via shared_ptr)

Key methods (from assertion strings and mangled symbols):
- `GetVidPidForDevice(DeviceType, U16&, U16&)` -- reverse dispatch (type -> VID:PID)
- `AddNewDevice(shared_ptr<UsbDevice>, DeviceType, function<void(DeviceInstanceId, GenericDevice&)>)` -- device factory
- `AddFakeDevice(DeviceId)` -> `DeviceInstanceId` -- simulation device creation
- `RemoveFakeDataDevice(DeviceId)` -- simulation device removal

### LinuxDevicesManager

Inherits from DevicesManager (from shared_ptr_emplace pattern).

Key methods:
- `GetAllDevices()` -> `map<LibUsbDeviceRef, DeviceType>` -- USB enumeration
- `CheckForConnectionThread()` -- hotplug polling thread
- `AddNewDevice(LibUsbDeviceRef, DeviceType)` -- Linux-specific device factory


## DevicesManager Layout (inferred from field offsets)

From the CheckForConnectionThread disassembly (ELF VA 0x1c18440):

| Offset    | Type                        | Purpose                        |
|-----------|-----------------------------|--------------------------------|
| +0x90     | std::mutex                  | Lock for device list access    |
| +0x130    | libusb_context*             | libusb session handle          |
| +0x138    | std::map<?,?>               | Connected devices (red-black tree) |
| +0x140    | (map sentinel/end node)     | Map end iterator               |
| +0x150    | vector<LibUsbDeviceRef>     | Device reference list (start)  |
| +0x158    | (vector end pointer)        | Device reference list (end)    |


## USB Device Enumeration: GetAllDevices()

ELF VA: `0x1c18240`
Source: `LinuxDevicesManager.cpp`

This function performs a full USB bus scan via libusb:

```
GetAllDevices(this):
    map<LibUsbDeviceRef, DeviceType> result

    ctx = this->libusb_context  // at offset +0x130
    if ctx == NULL:
        ASSERT("GetAllDevices", LinuxDevicesManager.cpp, line 0x15)

    count = libusb_get_device_list(ctx, &device_list)
    if count <= 0:
        libusb_free_device_list(device_list, 1)
        return result

    for i in 0..count:
        dev = device_list[i]
        rc = libusb_get_device_descriptor(dev, &desc)
        if rc < 0:
            continue

        vid = desc.idVendor   // at desc+0x08 (USB spec offset)
        pid = desc.idProduct  // at desc+0x0A

        // VID:PID -> DeviceType dispatch
        device_type = INVALID

        if vid == 0x0925 && pid == 0x3881:
            device_type = 0   // Logic (original)
        elif vid == 0x21a9 && pid == 0x1001:
            device_type = 1   // Logic16
        elif vid == 0x21a9 && pid == 0x1003:
            device_type = 4   // Logic4
        elif vid == 0x21a9 && pid == 0x1004:
            device_type = 5   // Logic8
        elif vid == 0x21a9 && pid == 0x1005:
            device_type = 6   // LogicPro8
        elif vid == 0x21a9 && pid == 0x1006:
            device_type = 7   // LogicPro16
        elif vid == 0x21a9 && pid == 0x1007:
            device_type = 12  // Mso
        else:
            continue  // unknown device, skip

        ref = LibUsbDeviceRef(dev)            // wraps libusb_device*
        result.insert(ref, device_type)       // add to map
        // stores device_type at map_node+0x28

    libusb_free_device_list(device_list, 1)   // unref all devices
    return result
```

Key observations:
- **No hotplug API** -- despite libusb_hotplug symbols being linked, the Linux
  implementation uses polling (libusb_get_device_list) rather than hotplug callbacks.
- **PID 0x1007 = Mso** -- this is the first confirmation that Mso has a USB PID
  (21a9:1007). Previously only known to use SPI flash firmware.
- **Sequential VID check** -- the code checks VID==0x0925 first (original Logic),
  then checks VID==0x21a9 for all modern devices. The VID check is redundant for
  each PID comparison but the compiler didn't optimize it away.
- **No bootloader PIDs** -- the FX3 bootloader PIDs (e.g. 04b4:00f3 Cypress
  default) are not checked. The application only enumerates devices that have
  already been programmed with Saleae firmware. Firmware upload must happen through
  a separate mechanism (possibly cyusb_linux or a boot-time enumeration path).


## Hotplug Polling: CheckForConnectionThread()

ELF VA: `0x1c18440`
Source: `LinuxDevicesManager.cpp`

This function is the main body of the hotplug detection thread. It:

1. Compares the current device list (from `this->device_refs` at +0x150) against
   a freshly obtained map of connected devices (passed as parameter)

2. **Detects removed devices**: iterates the existing device list and checks if
   each device is still present in the new map. Devices not found are collected
   into a "disconnected" vector.

3. **Detects new devices**: iterates the new map and checks if each device is
   already known. For unknown devices:
   - Acquires `this->mutex` (at +0x90)
   - Creates a `LibUsbDeviceRef` for the new device
   - Calls `LinuxDevicesManager::AddNewDevice(ref, device_type)` to instantiate
     the appropriate device class
   - Releases the mutex

4. **Processes disconnected devices**: for each disconnected device, calls a
   removal handler (at ELF VA 0x1c154f0).

The function signature matches:
```cpp
void LinuxDevicesManager::CheckForConnectionThread()
```

But it takes a parameter (the new device map), suggesting it's a method called
from the actual thread loop, not the thread function itself.

### Thread Lifecycle

The thread uses mutex-protected access to shared device state. The pattern is:

```
Thread loop (inferred):
    while not shutdown:
        new_devices = GetAllDevices()
        CheckForConnectionThread(new_devices)
        sleep(polling_interval)  // exact interval unknown
```


## LinuxDevicesManager::AddNewDevice

ELF VA: `0x1c189b0`
Source: `LinuxDevicesManager.cpp`

Called when a new USB device is detected during polling. Takes a LibUsbDeviceRef
and DeviceType. The function:

1. Calls `DevicesManager::AddNewDevice` (at ELF VA 0x1c13eb0) which is the
   device factory
2. The factory function:
   - Converts DeviceType to internal enum via the mapping function at 0x1c11fa0
   - Calls DeviceTypeToString to get the display name
   - Checks `SALEAE_IGNORE_DEVICE` environment variable (from re/notes/device-type-enum.md)
   - Creates the appropriate device class instance
   - Invokes the callback with (DeviceInstanceId, GenericDevice&)
3. Logs "Total AddNewDevice took {}s" with timing information


## Device State Machine

From mangled symbol analysis, the device state is modeled as a variant:

```cpp
namespace Saleae {
    struct DSInitializing { ... };  // device is initializing (FW upload, FPGA program)
    struct DSReady { ... };         // device is ready for capture
    struct DSError { ... };         // device encountered an error

    using DeviceState = std::variant<DSInitializing, DSReady, DSError>;
}
```

State transitions (from GenericDevice::UseState and SetDeviceState):

```
[Disconnected]
      |
      | (USB device detected by GetAllDevices)
      v
[DSInitializing]
      |
      | (FW upload, FPGA program, calibration load)
      |--- success ---> [DSReady]
      |--- failure ---> [DSError]

[DSReady]
      |
      | (USB disconnect) ---> [Disconnected] (device removed)
      | (capture error)  ---> [DSError]

[DSError]
      |
      | (USB disconnect) ---> [Disconnected] (device removed)
```

The DeviceListNode (in the Graph kernel architecture) observes state transitions:

```cpp
void DeviceListNode::OnConnect(DeviceInstanceId, shared_ptr<GenericDevice>)
    // Registers callbacks for DeviceState changes
    // Uses: DSInitializing, DSReady, DSError variant visitor

void DeviceListNode::OnInitialDeviceSearchComplete()
    // Called after first full USB scan completes
```


## Connection Error Handling

```cpp
class DevicesManager::ConnectionError_impl {
    enum ConnectionError { ... };  // specific error codes
};
```

Error types observed in the binary:
- `Saleae::UsbPermissionsError` -- USB permissions issue (udev rules missing)
- Connection errors are propagated via callback to `DeviceListNode`
- The `/lastConnectionError` path suggests errors are exposed to the Electron UI

Error recovery pattern:
- USB disconnect during capture triggers DSError state
- The device is removed from the connected device list
- On next poll cycle, if the device reappears, it gets a new DeviceInstanceId
  and goes through full re-initialization (DSInitializing -> DSReady)


## DeviceInstanceId

```cpp
namespace Saleae {
    struct DeviceInstanceId {
        // Wraps an integer identifier
        // Format string: "DeviceInstanceId({})"
    };
}
```

- Unique per device connection (not per physical device)
- A disconnect/reconnect produces a new DeviceInstanceId
- The assertion `!std::any_of(mState.mConnectedDevices.cbegin(), ...)` ensures
  no duplicate instance IDs in the connected device list
- `InvalidDeviceInstanceId` is a sentinel value for "no device"


## Simulation/Fake Devices

The DevicesManager supports simulation devices for development/testing:

- `AddSimulationDevice` / `RemoveSimulationDevice` -- kernel request handlers
  registered on DeviceListNode
- `AddFakeDevice(DeviceId)` -> `DeviceInstanceId` -- creates a fake device
- `RemoveFakeDataDevice(DeviceId)` -- removes a fake device
- "Unknown or unsupported fake device type" -- error for invalid simulation types
- `LogicDevice::ModifyFakeData(...)` -- generates fake capture data
- `LogicStudentDevice::ModifyFakeData(...)` -- Logic 4 variant
- "A protocol analyzer has failed at producing enough fake data." -- simulation error

DeviceType values 2 and 3 (gaps in the external enum) may have been reserved for
simulation device types, though GetVidPidForDevice treats them as assertion failures.


## USB Power Management

From the DeviceListNode request handlers:
- `SetOverrideUsb2PowerLimit` -- allows overriding the USB 2.0 power budget
  (USB 2.0 limits bus power to 500mA, USB 3.0 allows 900mA)
- This affects maximum sample rates and channel counts on USB 2.0 connections


## Graph Architecture Integration

DevicesManager integrates with the Graph kernel system:

```
KernelFactory::AddKernel<DeviceListNode>()
    |
    v
DeviceListNode::Initialize(DependencyInjector, Kernel::Configuration)
    |
    +-- Creates DevicesManagerProvider (shared_ptr)
    +-- Registers callbacks for:
    |     - OnConnect(DeviceInstanceId, shared_ptr<GenericDevice>)
    |     - OnDisconnect(DeviceInstanceId)
    |     - ConnectionError(DevicesManager::ConnectionError_impl::ConnectionError)
    |     - DeviceState changes (variant<DSInitializing, DSReady, DSError>)
    +-- Registers request handlers for:
          - AddSimulationDevice
          - RemoveSimulationDevice
          - SetOverrideUsb2PowerLimit
```


## Key ELF Virtual Addresses

| Address      | Function/Data                                      |
|-------------|-----------------------------------------------------|
| 0x1c11fa0   | DeviceType external-to-internal mapping              |
| 0x1c13df0   | DevicesManager::GetVidPidForDevice                   |
| 0x1c13eb0   | DevicesManager::AddNewDevice (factory)               |
| 0x1c154f0   | Device disconnect handler                            |
| 0x1c18240   | LinuxDevicesManager::GetAllDevices                   |
| 0x1c18440   | LinuxDevicesManager::CheckForConnectionThread        |
| 0x1c189b0   | LinuxDevicesManager::AddNewDevice                    |
| 0x471910    | DeviceType lookup table (ext -> int, 13 entries)     |
| 0x4719d4    | GetVidPidForDevice jump table (8 entries)             |


## Implications for Rust Implementation

1. **No hotplug needed initially** -- the Saleae binary uses polling, not
   libusb hotplug callbacks. Our Rust implementation can use nusb's device
   enumeration on a polling timer and be compatible.

2. **Device factory is straightforward** -- match on VID:PID to determine device
   type, then construct the appropriate device driver.

3. **DeviceInstanceId should be monotonic** -- each new device connection gets a
   new ID, even for the same physical device.

4. **PID 0x1007 = Mso** -- confirmed USB PID for Mso device. This was previously
   unknown; MsoDevice was thought to only use SPI flash.

5. **USB 2.0 power override** -- our driver should expose this setting for users
   who know their USB 2.0 port can supply more power.

6. **Error recovery is reconnection** -- on USB errors, the device is fully
   removed and re-detected on the next poll. No in-place recovery.
