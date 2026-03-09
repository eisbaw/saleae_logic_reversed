# Device Capabilities

Status: Confirmed (binary constructor immediates + JS bundle cross-reference)
Source: TASK-0044
Binary: libgraph_server_shared.so v2.4.40
Corroboration: Constructor MOV immediates match JS bundle defaults


## Per-Device Maximum Sample Rates

These values are hardcoded as MOV immediate operands in the device class
constructors. They represent the absolute maximum rates for each device
when connected at full USB 3.0 speed.

| Device       | Class                     | VID:PID   | Max Digital  | Max Analog  | Channels |
|-------------|---------------------------|-----------|-------------|-------------|----------|
| Logic 4     | LogicStudentDevice        | 21a9:1003 | 12 MHz      | 6 MHz       | 4        |
| Logic 8     | LogicGraduateDevice       | 21a9:1004 | 200 MHz     | 20 MHz      | 8        |
| Logic Pro 8 | LogicProfessional8Device  | 21a9:1005 | 500 MHz     | 50 MHz      | 8        |
| Logic Pro 16| LogicProfessional16Device | 21a9:1006 | 500 MHz     | 50 MHz      | 16       |

Extraction addresses:

| Device       | Constructor Call | Digital Rate Reg | Analog Rate Reg | Channel Reg |
|-------------|-----------------|------------------|-----------------|-------------|
| Logic 4     | 0x1c563e7       | esi=0x00B71B00   | ecx=0x005B8D80  | esi=4 @0x1c5633f |
| Logic 8     | 0x1c3d1f6       | ecx=0x0BEBC200   | r8d=0x01312D00  | r9d=8       |
| Logic Pro 8 | 0x1c44961       | r8d=0x1DCD6500   | r9d=0x02FAF080  | stack=8     |
| Logic Pro 16| 0x1c446e1       | r8d=0x1DCD6500   | r9d=0x02FAF080  | stack=16    |


## ADC Hardware and Ranges

| Device       | ADC (Xilinx Rev) | ADC (Lattice Rev) | Bit Depth | ADC Range       |
|-------------|------------------|-------------------|-----------|-----------------|
| Logic 4     | HMCAD1100        | ADS528x           | 8-bit     | 0 to 255        |
| Logic 8     | HMCAD1100        | ADS528x           | 8-bit*    | -2048 to 2047** |
| Logic Pro 8 | AD9637           | ADS528x           | 12-bit    | -2048 to 2047   |
| Logic Pro 16| AD9637           | ADS528x           | 12-bit    | -2048 to 2047   |

\* Logic 8 uses 8-bit HMCAD1100 on Xilinx revisions but data is stored in
int16_t containers. On Lattice revisions, it uses 12-bit ADS528x.

\** JS bundle uses {min:-2048, max:2047} for Logic 8, suggesting the native
code normalizes 8-bit values to 12-bit range on HMCAD1100 hardware.


## Default ADC-to-Voltage Transforms

These are fallback transforms used when calibration data is not available.
With calibration, per-channel transforms from `fullScaleVoltageRanges` are
applied instead.

| Device       | Input Min | Input Range | Output Min | Output Range | Voltage Range |
|-------------|-----------|-------------|------------|-------------|---------------|
| Logic 4     | 0         | 255         | 0 V        | 5 V         | 0 V to 5 V   |
| Logic 8     | -2048     | 4095        | 0 V        | 5 V         | 0 V to 5 V   |
| Logic Pro 8 | -2048     | 4095        | -10 V      | 20 V        | -10 V to 10 V|
| Logic Pro 16| -2048     | 4095        | -10 V      | 20 V        | -10 V to 10 V|

Transform formula: `voltage = adc_code * (outputRange / inputRange) + (outputMin - inputMin * m)`

Source: `compute1DTransform()` in bundle.js


## Analog Downsampling

Available downsampled analog rates (Hz), applicable to devices that
support analog downsampling:

```
31250, 15625, 6250, 1250, 250, 50
```

Devices supporting analog downsampling: Logic 4, Logic 8, Logic Pro 8, Logic Pro 16.


## Calibration System

| Device       | Calibration ID | Requires Calibration |
|-------------|---------------|---------------------|
| Logic       | 0             | No                  |
| Logic 16    | 1             | No                  |
| Logic 4     | 4             | Yes                 |
| Logic 8     | 5             | Yes                 |
| Logic Pro 8 | 6             | Yes                 |
| Logic Pro 16| 7             | Yes                 |


## Channel Capability Types

Each channel reported by `GetDeviceSettings()` has one of three
capability states:

| Capability       | Meaning                        |
|-----------------|--------------------------------|
| AlwaysEnabled   | Channel cannot be disabled     |
| HardwareDisabled| Channel unavailable on this HW |
| Toggleable      | User can enable/disable        |

The `channelCapabilities` array describes every channel (Analog and
Digital) on the device. The exact per-device channel layout (which
channels are Toggleable vs AlwaysEnabled) is returned from the native
`GetDeviceSettings()` function and depends on the device type and
hardware revision.


## USB Bandwidth Limiting

When a device connects at USB 2.0 speed instead of USB 3.0, the
`hasLimitedBandwidth` flag is set, which restricts available sample rate
options. The JS bundle uses lower default rates for limited bandwidth:

| Device       | Limited BW Digital | Limited BW Analog |
|-------------|-------------------|-------------------|
| Logic 4     | 12 MHz            | 6 MHz             |
| Logic 8     | 100 MHz           | 2,500 Hz          |
| Logic Pro 8 | 100 MHz           | 3,125 Hz          |
| Logic Pro 16| 100 MHz           | 3,125 Hz          |

Note: The extremely low analog rates (2.5 kHz, 3.125 kHz) for limited
bandwidth are the downsampled rates, not the ADC physical rate. The ADC
still samples at a higher rate and the data is downsampled on the host.


## Device Settings Protocol

The native code serializes device capabilities to JSON using these keys:

```json
{
  "channelCapabilities": [
    {"type": "Analog"|"Digital", "index": 0, "capability": "Toggleable"}
  ],
  "sampleRateOptions": [
    {"digital": 500000000, "analog": 50000000}
  ],
  "digitalThresholdOptions": [
    {"description": "1.8 Volts"}
  ],
  "isPhysicalDevice": true
}
```

Source file: `/workspace/monorepo/graph-io/devices/logic_interface/src/DeviceSettings.cpp`


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|---------|
| Logic 8: 200 MHz digital, 20 MHz analog | **Confirmed** | MOV immediates at 0x1c3d1f6 + JS defaults |
| Logic Pro 8: 500 MHz digital, 50 MHz analog | **Confirmed** | MOV immediates at 0x1c44961 + JS defaults |
| Logic Pro 16: 500 MHz digital, 50 MHz analog | **Confirmed** | MOV immediates at 0x1c446e1 + JS defaults |
| Logic 4: 12 MHz digital, 6 MHz analog | **Confirmed** | MOV immediates at 0x1c563e7 + JS defaults |
| Logic Pro 16: 16 channels | **Confirmed** | MOV $0x10 at 0x1c446c2 |
| Logic Pro 8: 8 channels | **Confirmed** | MOV $0x8 at 0x1c44942 |
| Logic 8: 8 channels | **Confirmed** | MOV $0x8 at 0x1c3d201 |
| Logic 4: 4 channels | **Confirmed** | MOV $0x4 at 0x1c5633f |
| ADC ranges (Logic 4 vs others) | **Confirmed** | JS bundle compute1DTransform |
| Voltage transforms | **High** | JS bundle only; not independently verified |
| Channel capability types | **Confirmed** | JSON schema in JS + /capability key in binary |
| Calibration IDs | **High** | JS bundle only |
