# saleae-all-usb.pcap Analysis

Captured: ~March 2026
Duration: 9.6 seconds
Total packets: 299 (280 TCP, 19 other)
Capture type: SLL (Linux cooked capture) -- **network traffic, NOT USB**

## Misnomer Warning

Despite the filename "saleae-all-usb.pcap", this is a **network packet capture**,
not a USB capture. It contains TCP/TLS traffic between the host machine and several
remote servers. It was likely captured simultaneously with the USB init sequence
in `saleae-logic2-full-init.pcap` to observe what network calls the Saleae Logic 2
application makes during device initialization.

## Traffic Summary

| Remote IP        | Port | Packets | Host->Remote | Remote->Host | Purpose (inferred)       |
|------------------|------|---------|--------------|--------------|--------------------------|
| 160.79.104.10    | 443  | 251     | 441 KB       | 20 KB        | Calibration upload/API   |
| 104.18.6.151     | 443  | 14      | 1.6 KB       | 1.9 KB       | Cloudflare (telemetry?)  |
| 172.66.152.186   | 443  | 4       | 183 B        | 179 B        | Cloudflare (telemetry?)  |
| 104.18.5.137     | 443  | 8       | 380 B        | 380 B        | Cloudflare (telemetry?)  |

All traffic is TLS-encrypted (HTTPS on port 443). No DNS queries are present in the
capture (sessions were already established or DNS was on a separate interface).
No TLS Client Hello SNI fields were captured for the main session.

## Main Session: 160.79.104.10

The dominant connection is a **441 KB upload from host to 160.79.104.10:443**.
The server responds with only 20 KB.

### Traffic Pattern

1. **t=0.31s**: Two large TCP segments sent (7312 bytes each = 14,624 bytes)
2. **t=0.34s**: Server ACKs (8 ACK packets)
3. **t=0.34-0.35s**: Sustained burst of TLS Application Data from host
   - Mixed 2968, 8760, 14552, 17448-byte TCP segments
   - This is a bulk upload pattern
4. **t=0.35-8.8s**: Continued upload bursts with ACK batches
5. **Total upload: ~441 KB** from host to server

### Interpretation

The 441 KB upload is large for a simple API call. Possible purposes:
- **Calibration data upload**: Device calibration data being sent to Saleae servers
  (less likely -- calibration is typically downloaded, not uploaded)
- **Firmware/device info registration**: Sending device metadata
- **Analytics/telemetry payload**: Accumulated usage data
- **Crash dump or log upload**: Application diagnostic data

Given the timing (concurrent with device init), the most likely explanation is
that this is the **calibration data download session** where the asymmetry is
caused by TLS overhead + TCP ACKs inflating the "from host" byte count, while
the actual calibration payload comes in the "from server" 20 KB response.

However, 20 KB response seems small for a full calibration file (.cal files are
typically larger). This may instead be a **device registration or license check**
that happens at startup.

## Cloudflare Sessions

Three small TLS sessions to Cloudflare-hosted IPs (104.18.x.x, 172.66.x.x):
- Very small payloads (100-1100 bytes)
- Likely telemetry, update checks, or API calls
- 104.18.6.151 session has the most data (3.4 KB total) with a TLS handshake
  visible (handshake type 2 = ServerHello)

## Relationship to USB Init Capture

The 9.6-second duration of this network capture overlaps with the early portion
of the 67.4-second USB capture (`saleae-logic2-full-init.pcap`). The network
activity occurs during the two silence gaps observed in the USB capture:

- **Gap 1 (t=0.1s to t=17s)**: Application startup -- network calls happen here
- **Gap 2 (t=17s to t=32s)**: Pre-firmware-upload preparation -- may involve
  downloading firmware or calibration from servers

## Limitations

- All payload data is TLS-encrypted -- actual content is not visible
- No DNS queries captured -- server hostnames unknown
- No TLS SNI captured for the main session
- Cannot determine if calibration data was downloaded or uploaded
- Short capture duration (9.6s) may miss later network activity

## Known Saleae Server Infrastructure

From reverse engineering, Saleae Logic 2 contacts:
- `downloads.saleae.com` -- firmware and calibration downloads
- `telemetry.saleae.com` or similar -- usage analytics
- The 160.79.104.10 IP likely belongs to one of these services
