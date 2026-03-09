# USB Capture Setup

## Overview

USB traffic capture from the Saleae Logic Pro 8 is done via SSH to the remote
host `nvdk-msdlab-09` where the device is physically attached. Captures are
streamed back to the local machine and saved to `re/captures/`.

## Remote Host: nvdk-msdlab-09

### Hardware

- **Device**: Saleae Logic Pro 8 (21a9:1005)
- **USB Bus**: 002, Device 003
- **USB Speed**: SuperSpeed (5Gbps) -- USB 3.0
- **Serial**: SERNUM (Saleae's default label, not a real serial)
- **Endpoints** (bootloader state):
  - EP1 OUT (0x01): Bulk, 1024 bytes, bMaxBurst=0 -- command/control
  - EP1 IN  (0x81): Bulk, 1024 bytes, bMaxBurst=0 -- command/control response
  - EP2 IN  (0x82): Bulk, 1024 bytes, bMaxBurst=15 -- high-throughput data
- **MaxPower**: 496mA (bus powered)
- **BOS**: USB 2.0 Extension (LPM), SuperSpeed capability

### Software

| Tool      | Status        | Notes                                          |
|-----------|---------------|-------------------------------------------------|
| tcpdump   | Installed     | v4.99.0, works with usbmon interfaces           |
| tshark    | NOT installed | `sudo dnf install wireshark-cli` to install      |
| usbmon    | Available     | Must be loaded with `sudo modprobe usbmon`       |
| sudo      | Passwordless  | Required for modprobe and tcpdump on usbmon      |

- **OS**: RHEL 9 (kernel 5.14.0-388.el9.x86_64)
- **debugfs**: Mounted at `/sys/kernel/debug`
- **usbmon devices**: `/dev/usbmon0` (all buses), `/dev/usbmon1`, `/dev/usbmon2`
- **Capture interface for Saleae**: `usbmon2` (bus 2)

### Known Limitations

1. **tshark not available on remote host**: All capture is done with tcpdump.
   tcpdump produces pcap format (not pcapng) when writing to stdout. Both
   formats are readable by tshark/Wireshark on the local machine.

2. **No remote-side filtering by VID/PID**: tcpdump on usbmon captures ALL
   traffic on the bus, not just Saleae traffic. Other USB devices on bus 2
   (hub traffic, etc.) will appear in captures. Filter during analysis with
   tshark display filters.

3. **usbmon must be loaded each boot**: The usbmon module is not loaded by
   default. The capture scripts load it automatically with `sudo modprobe
   usbmon`.

4. **Bus number may change**: If the device is unplugged and replugged, or
   after a reboot, the bus/device numbers may change. The capture script
   auto-detects this from `lsusb`.

## Capture Scripts

### re/scripts/usb-capture.sh

Main capture script. Produces a pcap file and a companion .md file.

```bash
# Basic idle capture (5 seconds)
bash re/scripts/usb-capture.sh idle -d 5

# Firmware upload capture (30 seconds, with notes)
bash re/scripts/usb-capture.sh firmware-upload -d 30 -n "Cold boot firmware upload"

# Long capture
bash re/scripts/usb-capture.sh digital-capture -d 120 -n "1ch 1MHz via Saleae app"
```

### Justfile recipes

```bash
just usb-capture idle                  # 5s labeled capture
just usb-capture firmware-upload 30    # 30s labeled capture
just usb-capture-quick 10             # 10s quick unlabeled capture
just usb-list                          # List USB devices on remote host
just usb-descriptors                   # Full descriptor dump
just usb-monitor                       # Live tcpdump monitor (Ctrl-C to stop)
just usb-check                         # Verify capture prerequisites
```

## File Naming Convention

```
re/captures/{device}_{operation}_{timestamp}.pcap
re/captures/{device}_{operation}_{timestamp}.md
```

Example:
```
re/captures/logic-pro-8_idle_2026-03-09_032409.pcap
re/captures/logic-pro-8_idle_2026-03-09_032409.md
```

The .pcap files are gitignored (they contain binary USB traffic data).
The .md companion files are committed to git (they document the capture).

## Analysis (Local Machine)

tshark is available locally (via nix-shell). Use it to analyze captures:

```bash
# Summary statistics
tshark -r re/captures/CAPTURE.pcap -q -z io,stat,1

# Show first N packets
tshark -r re/captures/CAPTURE.pcap -c 50

# Filter by USB device address (e.g., device 3 on bus 2)
tshark -r re/captures/CAPTURE.pcap -Y "usb.device_address == 3"

# Show only control transfers
tshark -r re/captures/CAPTURE.pcap -Y "usb.transfer_type == 2"

# Show only bulk transfers
tshark -r re/captures/CAPTURE.pcap -Y "usb.transfer_type == 3"

# Show vendor-specific control transfers (bRequest is non-standard)
tshark -r re/captures/CAPTURE.pcap -Y "usb.transfer_type == 2 && usb.bmRequestType == 0x40"

# Hex dump of specific packet
tshark -r re/captures/CAPTURE.pcap -x -Y "frame.number == 42"

# Export to CSV for scripting
tshark -r re/captures/CAPTURE.pcap -T fields \
  -e frame.number -e frame.time_relative -e usb.transfer_type \
  -e usb.endpoint_address -e usb.data_len -e usb.capdata \
  -E separator=, -E quote=d > capture.csv
```

## Capture Checklist

For TASK-0018, these captures are needed (per device):

1. [ ] Device idle (bootloader state, no app running)
2. [ ] Device plug-in / enumeration (before firmware)
3. [ ] Firmware upload sequence
4. [ ] FPGA bitstream programming
5. [ ] Simple digital capture (1 channel, 1MHz, 1 second)
6. [ ] Multi-channel digital capture
7. [ ] Analog capture (1 channel)
8. [ ] Mixed analog+digital capture
9. [ ] Capture stop / device idle
10. [ ] Device disconnect
11. [ ] Calibration data read (if observable)

Captures 2-11 require running the Saleae Logic 2 application on the remote
host, which is a separate coordination step.

## Installing tshark on Remote Host

If tshark is needed on the remote host for real-time filtered captures:

```bash
ssh nvdk-msdlab-09 'sudo dnf install wireshark-cli'
```

This would enable BPF filtering on the remote side, reducing bandwidth and
allowing VID/PID-specific capture filters. Currently not critical since
tcpdump captures work and analysis is done locally.
