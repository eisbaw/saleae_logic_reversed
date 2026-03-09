# FX3 USB Event Handler Analysis

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (49,762,176 bytes, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40


## Key Finding: Strings Are in Device Firmware, Not Host Code

The CY_U3P_USB_EVENT_* strings are **inside the embedded FX3 firmware blobs**,
not in the host-side x86-64 code. This was determined by cross-referencing
string file offsets against the firmware blob regions documented in
`firmware-extraction.md`.

### Proof

FX3 firmware regions in the .so:
- Pro 8:  0x0079E7A0 - 0x007BBBB0 (119,824 bytes)
- Pro 16: 0x007BBBB0 - 0x007D8FC0 (119,824 bytes)

All CY_U3P strings appear in **both** firmware images at identical relative
offsets. Example:

| String                        | Pro 8 .so offset | Pro 16 .so offset | FW-relative offset |
|-------------------------------|-----------------|------------------|--------------------|
| CY_U3P_USB_EVENT_SETCONF      | 0x007A6584      | 0x007C3994       | 0x7DE4             |
| CY_U3P_USB_EVENT_SPEED         | 0x007A66B8      | 0x007C3AC8       | 0x7F18             |
| CyU3PGpifSMStart failed        | 0x007A68F8      | 0x007C3D08       | 0x8158             |

The identical relative offsets (0x7DE4, 0x7F18, 0x8158) confirm these are the
same firmware code compiled twice with only PID/power/timestamp differences.

### Contrast with Host-Side Strings

These strings are **outside** the firmware regions (in host x86-64 code):

| String                          | .so offset | Location            |
|---------------------------------|------------|---------------------|
| Endpoint count is not 4         | 0x4208C6   | Host code (.rodata) |
| Invalid USB speed               | 0x36309A   | Host code (.rodata) |
| Firmware download took           | 0x41FE73   | Host code (.rodata) |
| LogicProfessionalDevice          | 0x369CDA   | Host code (.rodata) |

Note: "Error! Invalid USB speed." also appears inside the firmware at FW
offset 0x7D40 (in ProUsb.c). This is a **different** string from the host-side
"Invalid USB speed" at 0x36309A. The firmware version includes "Error!" prefix
and is a device-side debug message. The host version is a different code path
in `LogicProfessionalDevice`.


## Firmware Source File Structure

The firmware is organized into three source files (identified from embedded
`__FILE__` strings):

| File                  | FW offset | Responsibility                               |
|-----------------------|-----------|----------------------------------------------|
| CommandProcessing.c   | 0x7BAC    | Host command parser (EP1 OUT bulk commands)   |
| ProUsb.c              | 0x7CE8    | USB stack: descriptors, events, endpoints     |
| LogicPro.c            | 0x80E8    | Main app: init, GPIO, GPIF, thread management |


## FX3 RTOS Architecture

The firmware runs on Express Logic ThreadX RTOS:

```
Copyright (c) 1996-2008 Express Logic Inc.
ThreadX ARM9/RVDS Version G5.1.5.1
SN: 2923-115-1301
```

### System Threads (from CyU3P SDK)

| Thread              | Purpose                                        |
|---------------------|------------------------------------------------|
| 01_DMA_THREAD       | DMA engine for USB bulk transfers              |
| 02_SYSTEM_THREAD    | System management (clock, GPIO, power)         |
| 03_PIB_THREAD       | P-port Interface Block (GPIF II to FPGA)       |
| 04_UIB_THREAD       | USB Interface Block (USB 2.0/3.0 controller)   |
| 05_LPP_THREAD       | Low Performance Peripherals (I2C, SPI, GPIO)   |
| 06_SIB_THREAD       | Storage Interface Block (SD/MMC, unused)       |
| 07_DEBUG_THREAD     | Debug/trace output                             |
| System Timer Thread | ThreadX RTOS timer tick                        |

### Application Threads (Saleae custom code)

| Thread/Function           | Purpose                                       |
|---------------------------|-----------------------------------------------|
| LogicProApplicationInit   | Entry point: USB, GPIO, GPIF setup            |
| LogicProThread_Entry      | Main worker: command processing loop          |
| LogicProEventLoop         | USB event callback dispatch                   |
| 21:Bulk_src_sink          | DMA channel descriptor (bulk data, priority 21)|


## USB Event Handler (ProUsb.c)

The USB event callback in `ProUsb.c` handles the following events from the
CyU3P SDK. These are standard Cypress FX3 USB events registered via
`CyU3PUsbRegisterEventCallback()`.

### Event Flow

```
USB hardware interrupt
  -> UIB_THREAD (04_UIB_THREAD)
    -> CyU3PUsbRegisterEventCallback dispatch
      -> LogicProEventLoop (Saleae callback)
        -> switch(event_type) {
             CY_U3P_USB_EVENT_CONNECT:      // USB cable connected
             CY_U3P_USB_EVENT_DISCONNECT:    // combined with RESET
             CY_U3P_USB_EVENT_RESET:         // USB bus reset
             CY_U3P_USB_EVENT_SETCONF:       // SET_CONFIGURATION received
             CY_U3P_USB_EVENT_SETINTF:       // SET_INTERFACE received
             CY_U3P_USB_EVENT_SUSPEND:       // USB suspend (power saving)
             CY_U3P_USB_EVENT_RESUME:        // Resume from suspend
             CY_U3P_USB_EVENT_SPEED:         // USB speed negotiated
             CY_U3P_USB_EVENT_SET_SEL:       // USB 3.0 SET_SEL request
             CY_U3P_USB_EVENT_EP0_STAT_CPLT: // EP0 status stage complete
             default:                        // "OTHER USB event received:%d."
           }
```

### Event Details

| Event               | Data param | Meaning                                      | Action                                     |
|---------------------|------------|----------------------------------------------|--------------------------------------------|
| CONNECT             | none       | USB cable physically connected               | Log only                                   |
| RESET               | data       | USB bus reset signal received                | Log with data value                        |
| RESET/DISCONNECT    | n/a        | Combined reset+disconnect                    | Handled as reset variant                   |
| SETCONF             | config_num | Host sent SET_CONFIGURATION                  | Configure endpoints, start DMA             |
| SETINTF             | intf_num   | Host sent SET_INTERFACE                      | Reconfigure alternate setting              |
| SUSPEND             | none       | Host initiated USB suspend                   | Enter low power mode                       |
| RESUME              | none       | Resume from suspend state                    | Restore active mode                        |
| SPEED               | speed      | USB speed negotiation complete               | Validate speed (error if invalid)          |
| SET_SEL             | none       | USB 3.0 System Exit Latency request          | Log only (USB 3.0 power management)        |
| EP0_STAT_CPLT       | none       | EP0 control transfer status stage complete   | Log only                                   |

### SETCONF Action (Key Event)

The SETCONF event is the most important: it is triggered when the host sends
a USB SET_CONFIGURATION request. The firmware responds by:

1. Configuring bulk endpoints via `CyU3PSetEpConfig()`
2. Creating DMA channels via `CyU3PDmaChannelCreate()`
3. Starting DMA transfers via `CyU3PDmaChannelSetXfer()`
4. Configuring GPIF socket via `CyU3PGpifSocketConfigure()`

This matches the device state machine: after firmware upload and
re-enumeration, the host sends SET_CONFIGURATION, which triggers the
firmware to set up the data path (endpoints -> DMA -> GPIF -> FPGA).


## USB SETUP Request Handler

In addition to the event callback, the firmware has a separate USB SETUP
request handler (also in ProUsb.c):

```
USB SETUP event received. bType:%d, bTarget:%d, Request:%d, Value:%d, Index:%d, Length:5d
```

This handles vendor-specific control transfers on EP0 (bType = vendor,
bTarget = device). These are the register read/write commands used by the
host after firmware is running.

Note the format string has a typo: "Length:5d" instead of "Length:%d". This
is in the actual firmware.


## Initialization Sequence (Firmware-Side)

From the string ordering and API call sequence in the firmware:

```
1. LogicProApplicationInit()          [LogicPro.c]
   |
   +-> CyFxGpioInit()                 GPIO pin configuration
   |   +-> CyU3PGpioInit()            FX3 GPIO subsystem init
   |   +-> CyU3PGpioSetSimpleConfig() Per-pin configuration
   |
   +-> InitI2C()                       I2C bus initialization
   |
   +-> P-port initialization           GPIF II / FPGA interface
   |   +-> CyU3PGpifLoad()            Load GPIF state machine
   |   +-> CyU3PGpifSMStart()         Start GPIF state machine
   |
   +-> InitUsb()                       [ProUsb.c]
   |   +-> CyU3PUsbStart()            Start USB stack
   |   +-> CyU3PUsbSetDesc()          Register USB descriptors
   |   +-> CyU3PConnectState()        Enable USB connection (pull-up)
   |
   +-> Create LogicProThread_Entry()   Start main worker thread
       "app started!"

2. LogicProThread_Entry()              [CommandProcessing.c]
   |
   +-> process_raw_command() loop      Read commands from EP1 OUT
       +-> GetCommandRxBuffer()        Get DMA buffer from EP1 OUT
       +-> (parse and execute)
       +-> CommitCommandTxBuffer()     Send response via EP1 IN
       +-> DiscardRxBuffer()           Release input buffer
```


## DMA Channel Architecture

The firmware uses the CyU3P DMA API to set up data flow:

| Function                    | Purpose                                       |
|-----------------------------|-----------------------------------------------|
| CyFxBulkSrcSinkApplnStart  | Start bulk DMA channels (capture data flow)   |
| CyFxBulkSrcSinkApplnStop   | Stop bulk DMA channels                        |
| GetCommandTxBuffer          | Acquire TX buffer for EP1 IN (responses)      |
| GetCommandRxBuffer          | Acquire RX buffer from EP1 OUT (commands)     |
| CommitCommandTxBuffer       | Send response buffer to host via EP1 IN       |
| DiscardRxBuffer             | Release processed command buffer              |

The "BulkSrcSink" naming follows Cypress SDK conventions. The DMA channel
"21:Bulk_src_sink" suggests priority 21 (ThreadX priority) and the standard
Cypress bulk source/sink DMA template.

### Data Flow

```
Host                          FX3                           FPGA
 |                             |                             |
 |--[EP1 OUT: command]-------->|                             |
 |                             |--process_raw_command()      |
 |<--[EP1 IN: response]-------|                             |
 |                             |                             |
 |                             |<--[GPIF II: sample data]----|
 |<--[EP2 IN: capture data]---|                             |
 |                             |  (DMA: Bulk_src_sink)       |
```


## Standby/Power Management

The firmware supports a standby mode:
- `Enter standby returned %d` - response to a standby command
- `USB Power state changed to %d` - USB power state tracking
- `CY_U3P_USB_EVENT_SUSPEND` / `CY_U3P_USB_EVENT_RESUME` - standard USB
  suspend/resume handling

The standby appears to be command-triggered from the host, separate from
USB suspend.


## FX3 Memory Layout for These Strings

All application strings reside in Section 2 (SYSMEM at 0x40003000):

| Content area           | FW file offset  | FX3 memory address    |
|------------------------|----------------|-----------------------|
| Saleae app strings     | 0x7B68-0x81D4  | 0x400087D4-0x40008E40 |
| CyU3P SDK thread names | 0xAA0F-0x16273 | 0x4000B67B-0x40016ED7 |
| ThreadX copyright      | 0x1B920        | 0x40030310 (Section 4)|


## Relationship to Device State Machine

The firmware USB events map to the host-side device state machine
(see `doc/state-machine/device-states.md`):

```
Firmware Event                    Host-Side Consequence
-----------------------------------------------------------------
USB cable connected               -> libusb hotplug callback
(bootloader, no events yet)       -> host uploads firmware (0xA0)
FX3 re-enumerates                 -> host detects new VID:PID
CY_U3P_USB_EVENT_CONNECT          (logged by firmware)
CY_U3P_USB_EVENT_SPEED            (firmware validates USB speed)
CY_U3P_USB_EVENT_SETCONF          -> firmware configures endpoints+DMA
                                   -> host verifies 4 endpoints
                                   -> host programs FPGA bitstream
                                   -> host runs CommonDeviceInit
                                   -> DSInitializing -> DSReady
CY_U3P_USB_EVENT_SUSPEND          -> device power saving
CY_U3P_USB_EVENT_RESUME           -> device wake
CY_U3P_USB_EVENT_RESET            -> device reset / reconnect
CY_U3P_USB_EVENT_DISCONNECT       -> DSError (terminal)
```


## Open Questions

1. **Vendor requests on EP0**: The USB SETUP handler logs bType, bTarget,
   Request, Value, Index, Length. What are the specific vendor request
   codes used for register access? The format string confirms they exist
   but the values need USB capture analysis.

2. **GPIF state machine**: `CyU3PGpifLoad` loads a GPIF II state machine
   descriptor. This defines the parallel interface timing to the FPGA. The
   descriptor itself is embedded as binary data in the firmware (not as
   strings). Extracting and disassembling the GPIF descriptor would reveal
   the exact bus timing protocol.

3. **Command protocol**: `process_raw_command` and "unrecognized command"
   suggest a command/response protocol on EP1 OUT/IN. The command format
   and opcodes need USB capture analysis or Ghidra disassembly of the
   firmware ARM9 code.

4. **Standby mechanism**: What host command triggers "Enter standby"? This
   is separate from USB suspend and may be a vendor-specific power
   management feature.

5. **I2C usage**: `InitI2C` is called during init. I2C is likely used for
   EEPROM access (calibration data, serial number) or communication with
   the analog front-end / ADC. Needs investigation.


## References

- Cypress FX3 SDK: `cyfxapidesc.h` - USB event callback types
  (`CyU3PUsbEventType_t` enum)
- Cypress AN75705: Getting Started with EZ-USB FX3
  (ThreadX RTOS thread model, GPIF II architecture)
- Express Logic ThreadX: ARM9 port, priority-based preemptive RTOS
- `re/notes/firmware-extraction.md` - firmware blob locations
- `re/notes/fx3-firmware-protocol.md` - firmware upload protocol
- `doc/state-machine/device-states.md` - host-side state machine
