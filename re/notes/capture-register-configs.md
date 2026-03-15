# Capture Register Configurations (Hardware-Validated)

All configs captured via usbmon of Saleae Logic 2 GUI v2.4.36 on Logic Pro 8 (hw rev 1.2).
Pcap files in `re/captures/gui_pcaps/`.

## Register Map (12 registers written via WriteRegisters 0x80)

| Reg   | Name       | Description |
|-------|------------|-------------|
| 0x00  | CTRL       | Master control (0x00=idle, 0x01=capture, JESD cascade values) |
| 0x08  | ANA_MASK   | Analog channel enable bitmask (bit per channel) |
| 0x06  | DIG_MASK   | Digital channel enable bitmask (bit per channel) |
| 0x0a  | ANA_DEC    | Analog decimation: 50M / analogRate |
| 0x0b  | DIG_DIV_LO | Digital divider low: (500M / digitalRate) / 5 |
| 0x0c  | DIG_DIV_HI | Digital divider high byte |
| 0x1a  | ADC_CFG    | ADC config: raw 500M/digRate==2 -> 1, ==4 -> 2, else 0 |
| 0x0d  | MIX_ANA    | Analog mix value: samples_per_group * numAnalog * ana_quotient |
| 0x0e  | MIX_DIG    | Digital mix value: numDigital * dig_quotient |
| 0x12  | MODE       | Capture mode (0x01=analog-only, 0x02=mixed-USB2, 0x04=digital/mixed-USB3) |
| 0x13  | RSVD       | Reserved (always 0x00 for board rev < 2) |
| 0x14  | DEC_FACTOR | Bandwidth decimation: min(0xFF, ceil(3.2G / T)) |

## Validated Configurations

### Config 1: Mixed 6.25MHz dig + 1.5625MHz ana, 8dig+8ana (USB 2.0) - ORIGINAL REFERENCE
- Source: `re/captures/usbmon4-ep1-decoded.txt` (pre-existing pcap)
- `r0x00=0x00, r0x08=0xff, r0x06=0xff, r0x0a=0x20, r0x0b=0x10, r0x0c=0x00`
- `r0x1a=0x00, r0x0d=0x18, r0x0e=0x08, r0x12=0x02, r0x13=0x00, r0x14=0x10`

### Config 2: Digital-only 1MHz 8ch (USB 3.0)
- Source: `usbmon4_digital_1mhz_8ch_20260315_210312.pcap`
- `r0x00=0x00, r0x08=0x00, r0x06=0xff, r0x0a=0x00, r0x0b=0x64, r0x0c=0x00`
- `r0x1a=0x00, r0x0d=0x00, r0x0e=0x08, r0x12=0x04, r0x13=0x00, r0x14=0xff`

### Config 3: Analog-only 1.5625MHz 2ch (USB 3.0)
- Source: `usbmon4_analog_1_5mhz_2ch_20260315_210644.pcap`
- `r0x00=0x00, r0x08=0x03, r0x06=0x00, r0x0a=0x20, r0x0b=0x00, r0x0c=0x00`
- `r0x1a=0x00, r0x0d=0x06, r0x0e=0x00, r0x12=0x01, r0x13=0x00, r0x14=0x56`

### Config 4: Digital-only 50MHz 2ch (USB 3.0)
- Source: `usbmon4_digital_50mhz_2ch_20260315_211304.pcap`
- `r0x00=0x00, r0x08=0x00, r0x06=0x03, r0x0a=0x00, r0x0b=0x02, r0x0c=0x00`
- `r0x1a=0x00, r0x0d=0x00, r0x0e=0x02, r0x12=0x04, r0x13=0x00, r0x14=0x20`

### Config 5: Mixed 50MHz dig + 12.5MHz ana, 4dig+2ana (USB 3.0)
- Source: `usbmon4_mixed_50m_12_5m_4d2a_20260315_211221.pcap`
- `r0x00=0x00, r0x08=0x03, r0x06=0x0f, r0x0a=0x04, r0x0b=0x02, r0x0c=0x00`
- `r0x1a=0x00, r0x0d=0x06, r0x0e=0x04, r0x12=0x04, r0x13=0x00, r0x14=0x07`

### Config 6: Mixed 250MHz dig + 6.25MHz ana, 4dig+2ana (USB 3.0)
- Source: `usbmon4_mixed_125m_6_25m_4d2a_20260315_211344.pcap`
- `r0x00=0x00, r0x08=0x03, r0x06=0x0f, r0x0a=0x08, r0x0b=0x00, r0x0c=0x00`
- `r0x1a=0x01, r0x0d=0x06, r0x0e=0x28, r0x12=0x04, r0x13=0x00, r0x14=0x03`

## Analysis

### r0x12 (MODE)
- 0x01 = analog-only
- 0x02 = mixed mode on USB 2.0
- 0x04 = digital-only OR mixed mode on USB 3.0

### r0x0b:r0x0c (DIG_DIV) - Digital clock divider
Formula: `raw = 500M / digitalRate; if raw==1: raw=0; result = raw / 5`
- 1MHz: (500M/1M)/5 = 100 = 0x64
- 50MHz: (500M/50M)/5 = 2 = 0x02
- 6.25MHz (USB2): (500M/6.25M)/5 = 16 = 0x10
- 250MHz: 500M/250M=2, /5=0 = 0x00

### r0x0a (ANA_DEC) - Analog decimation
- 1.5625MHz analog: 0x20 (32) = 50MHz/32
- 12.5MHz analog: 0x04 = 50MHz/4 (USB3 mixed) or 0x20 = 50MHz/32 (USB2 and analog-only 1.5MHz)
- 6.25MHz analog: 0x08 = 50MHz/8
- Digital-only: 0x00

### r0x0d:r0x0e (MIX ratio)
- Digital-only: r0x0d=0x00 (no analog bandwidth needed)
- Analog-only: r0x0d=0x06, r0x0e=0x00
- Mixed: r0x0d=0x06, r0x0e varies with digital rate
  - r0x0e encodes how much USB bandwidth goes to digital vs analog

### r0x14 (DEC_FACTOR) - Bandwidth decimation factor
Formula: `T = numDig * digRate + numAna * anaRate * 12; result = min(0xFF, ceil(3.2G / T))`
- 0xff = 8 digital @ 1MHz: T=8M, ceil(3.2G/8M)=400 -> clamped to 255
- 0x56 = 2 analog @ 1.5625MHz: T=37.5M, ceil(3.2G/37.5M)=86 = 0x56
- 0x10 = 8+8 mixed @ 6.25M/1.5625M: T=200M, ceil(3.2G/200M)=16 = 0x10
- 0x20 = 2 digital @ 50MHz: T=100M, ceil(3.2G/100M)=32 = 0x20
- 0x07 = 4d+2a mixed 50M/12.5M: T=500M, ceil(3.2G/500M)=7
- 0x03 = 4d+2a mixed 250M/6.25M: T=1150M, ceil(3.2G/1150M)=3

### r0x1a (ADC_CFG)
Based on raw digital divider (500M / digitalRate) BEFORE /5:
- 0x00 = default (raw_divider != 2 and != 4)
- 0x01 = 250 MHz digital rate (raw_divider=2)
- 0x02 = 125 MHz digital rate (raw_divider=4, not yet observed in pcap)
