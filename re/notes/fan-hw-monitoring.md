# Fan Control and Hardware Monitoring

## Summary

Fan control and hardware monitoring are **MSO-only features**. They do not
apply to Logic Pro 8, Logic Pro 16, Logic 8, or Logic 4 devices. The
`LogicProfessionalDevice` class has zero fan/thermal/monitoring references.

This is relevant context for the MSO device (which is out of scope per PRD),
but documented here for completeness.

## Evidence: MSO-Only

All fan/thermal strings trace to:
- Source path: `/workspace/monorepo/graph-io/devices/mso_device/src/mso_device/mso_device_interface.cpp`
- Class: `Saleae::MsoDeviceInterface`
- Functions: `EnableFanControl`, `EnableHardwareMonitoring`
- No equivalent exists in `LogicProfessionalDevice` or `Logic2FpgaDevice`

## Firmware Events (USB interrupt stream)

The MSO device sends firmware interrupt events via a USB read stream.
Event types (from binary strings):

| Event ID | Name             | Description                          |
|----------|------------------|--------------------------------------|
| 0x0001   | WallPlugEvent    | Wall power connected/disconnected    |
| 0x0003   | ScPlugEvent      | Smart cable plugged in               |
| 0x0004   | ScUnplugEvent    | Smart cable unplugged                |
| 0x0006   | OverflowEvent    | Data overflow (FIFO overrun)         |
| 0x0007   | ThermalEvent     | Temperature threshold crossed        |
| 0x000F   | FaultEvent       | Hardware fault (general)             |

## Thermal Monitoring

### Temperature Sensors (5 sensors)

The MSO reports temperatures from 5 locations, serialized as
`MsoTemperature(afe={}C, adc={}C, fpga={}C, fan0={}C, fan1={}C)`:

| Flatbuffers Path         | Sensor Location                  |
|--------------------------|----------------------------------|
| `/afeCelsius`            | Analog front-end board           |
| `/adcCelsius`            | ADC (12QJ1600 - TI ADC12QJ1600) |
| `/fpgaCelsius`           | FPGA die temperature             |
| `/fanController0Celsius` | Fan controller 0 (ambient/inlet) |
| `/fanController1Celsius` | Fan controller 1 (ambient/inlet) |

### Thermal Thresholds

Two threshold levels exist:
- `/hotThresholdCelsius` -- triggers `[fw_thread {}] Thermal hot: sensor ID {} is {}C`
- `/criticalThresholdCelsius` -- triggers `[fw_thread {}] Thermal critical: sensor ID {} is {}C`

### Thermal Faults

- `MsoThermalFault` -- "MSO temperature too high"
- `MsoFanFault` -- "Fan failure detected - did your [device] ..."
- Both surface as user-facing error messages in the Electron UI

## Fan Control

- `EnableFanControl` -- firmware command sent during MSO initialization
- `SerializeFanControlState( &bb, &state )` -- fan state is serialized
  (likely as flatbuffers) for reporting to the host
- Two fan controllers (fan0, fan1) with temperature readback

## Hardware Monitoring

- `EnableHardwareMonitoring` -- firmware command sent during MSO init
- `SerializeSystemMonitorState( &bb, &state )` -- general system health
- `SerializeTemperatureState( &bb, &state )` -- temperature data
- Can be disabled via env var: `SALEAE_MSO_DISABLE_HARDWARE_MONITORING`

## MSO Hardware Components (from source paths)

The MSO device has these identifiable ICs:

| Component               | Part Number      | Role                          |
|--------------------------|------------------|-------------------------------|
| PMIC                    | TPS650864 (TI)   | Power management IC, 6 bucks + Vtt |
| ADC                     | ADC12QJ1600 (TI) | 12-bit quad-channel 1.6 GSPS ADC   |
| EEPROM                  | AT24C64D         | 64Kbit I2C EEPROM (cal data?) |
| VGA (variable gain amp) | LMH6518 (TI)     | Wideband VGA for analog input |
| SPI Flash               | (unknown part)   | Firmware storage              |
| FPGA                    | (Xilinx)         | Main data acquisition         |

### PMIC Initialization Sequence

The TPS650864 has multiple buck regulators enabled in sequence:
1. Initializing PMIC
2. Enabling PMIC Buck 1
3. Enabling PMIC Buck 2
4. Enabling PMIC Buck 3
5. Enabling PMIC Buck 4
6. Enabling PMIC Buck 5 (implied)
7. Enabling PMIC Buck 6
8. Enabling PMIC Buck Vtt

### Voltage Monitoring

- `UnderVoltage` event type exists
- `Insufficient MSO power supply voltage` -- power rail monitoring
- `MSO power supply: output={}V, max_current: {}A` -- PSU telemetry
- `UH OH, MSO power suppply DOES NOT meet required voltage: actual={}V, required={}V`

## MSO Initialization Sequence (relevant portion)

1. `Initialize_PreWallPowerDetected` -- wait for wall power
2. Wall power detection (WallPlugEvent 0x0001)
3. `Initialize_PostWallPowerDetected` -- proceed with hardware init
4. PMIC initialization (buck regulators)
5. `Enabling fan control in firmware.`
6. `Enabling hardware monitoring in firmware.`
7. (FPGA, ADC, etc. initialization continues)

## UI Integration (from bundle.js)

The Electron UI displays MSO health data:

```javascript
// Temperature display in device info panel
o.push(["Temperatures", ""]);
o.push(["  ADC",              `${i.adcCelsius.toFixed(2)} C`]);
o.push(["  AFE",              `${i.afeCelsius.toFixed(2)} C`]);
o.push(["  Fan Controller 0", `${i.fanController0Celsius.toFixed(2)} C`]);
o.push(["  Fan Controller 1", `${i.fanController1Celsius.toFixed(2)} C`]);
o.push(["  FPGA",             `${i.fpgaCelsius.toFixed(2)} C`]);
```

A temperature warning (blinking red background) triggers when:
- AFE temperature exceeds 55C with more than 1C margin (i.e., > 56C)

The data flows through:
`msoDeviceExtraInfo.msoHealth.temperatures` -> UI display

## Relevance to Logic Pro RE

**None.** Fan control and hardware monitoring are exclusively MSO features.
The Logic Pro 8/16 devices are passive-cooled with no temperature sensors,
no fan controllers, and no PMIC. This investigation is closed.

The MSO component details (TPS650864, ADC12QJ1600, AT24C64D, LMH6518) are
interesting for anyone attempting MSO RE but are out of scope for this project.
