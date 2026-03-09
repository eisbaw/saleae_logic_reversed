#!/usr/bin/env bash
# usb-capture.sh -- Capture USB traffic from a Saleae device on a remote host
#
# Uses tcpdump over SSH to capture from usbmon. Produces a pcap file in
# re/captures/ with a companion .md file documenting the capture.
#
# Requirements on remote host:
#   - tcpdump (installed)
#   - usbmon kernel module (loaded automatically by this script via sudo)
#   - passwordless sudo for modprobe and tcpdump
#
# Note: tshark is NOT installed on the remote host as of 2026-03-09.
# tcpdump captures are pcap format (not pcapng) when streamed via stdout.
# Wireshark and tshark on the local machine can read both formats.

set -euo pipefail

# ── Configuration ──────────────────────────────────────────────────────────

REMOTE_HOST="${SALEAE_USB_HOST:-nvdk-msdlab-09}"
SALEAE_VID="21a9"
CAPTURES_DIR="$(cd "$(dirname "$0")/../captures" && pwd)"

# ── Usage ──────────────────────────────────────────────────────────────────

usage() {
    cat <<'USAGE'
Usage: usb-capture.sh <operation> [options]

Arguments:
  operation     Name of the operation being captured (e.g., "idle", "firmware-upload",
                "digital-capture"). Used in filename and companion .md.

Options:
  -d DURATION   Capture duration in seconds (default: 10)
  -b BUS        USB bus number to capture from (default: auto-detect from device)
  -c COUNT      Max packet count (default: unlimited, duration-limited)
  -D DEVICE     Device product string filter, e.g., "1005" (default: auto-detect)
  -n NOTES      One-line description for the companion .md file
  -h            Show this help

Output:
  re/captures/logic-pro-8_{operation}_{timestamp}.pcap
  re/captures/logic-pro-8_{operation}_{timestamp}.md

Examples:
  usb-capture.sh idle -d 5
  usb-capture.sh firmware-upload -d 30 -n "Cold boot with no prior firmware"
  usb-capture.sh digital-capture -d 60 -n "1ch 1MHz 1s capture via Saleae app"
USAGE
    exit "${1:-0}"
}

# ── Argument parsing ──────────────────────────────────────────────────────

OPERATION=""
DURATION=10
BUS=""
MAX_COUNT=""
DEVICE_FILTER=""
NOTES=""

if [ $# -lt 1 ]; then
    usage 1
fi

OPERATION="$1"
shift

while getopts "d:b:c:D:n:h" opt; do
    case "$opt" in
        d) DURATION="$OPTARG" ;;
        b) BUS="$OPTARG" ;;
        c) MAX_COUNT="$OPTARG" ;;
        D) DEVICE_FILTER="$OPTARG" ;;
        n) NOTES="$OPTARG" ;;
        h) usage 0 ;;
        *) usage 1 ;;
    esac
done

# ── Detect device ─────────────────────────────────────────────────────────

echo "=== Detecting Saleae device on $REMOTE_HOST ==="

DEVICE_INFO=$(ssh "$REMOTE_HOST" "lsusb -d ${SALEAE_VID}: 2>/dev/null" || true)

if [ -z "$DEVICE_INFO" ]; then
    echo "ERROR: No Saleae device found on $REMOTE_HOST" >&2
    exit 1
fi

echo "  Found: $DEVICE_INFO"

# Extract bus number if not specified
if [ -z "$BUS" ]; then
    BUS=$(echo "$DEVICE_INFO" | head -1 | sed 's/Bus \([0-9]*\).*/\1/' | sed 's/^0*//')
    echo "  Auto-detected bus: $BUS"
fi

# Extract device address for filtering
DEV_ADDR=$(echo "$DEVICE_INFO" | head -1 | sed 's/.*Device \([0-9]*\).*/\1/' | sed 's/^0*//')
echo "  Device address: $DEV_ADDR"

# Determine device model from product ID
PRODUCT_ID=$(echo "$DEVICE_INFO" | head -1 | grep -oP 'ID \K[0-9a-f]{4}:[0-9a-f]{4}')
case "$PRODUCT_ID" in
    21a9:1003) DEVICE_MODEL="logic-4" ;;
    21a9:1004) DEVICE_MODEL="logic-8" ;;
    21a9:1005) DEVICE_MODEL="logic-pro-8" ;;
    21a9:1006) DEVICE_MODEL="logic-pro-16" ;;
    21a9:1001) DEVICE_MODEL="logic-16-legacy" ;;
    0925:3881) DEVICE_MODEL="logic-original" ;;
    *)         DEVICE_MODEL="unknown-${PRODUCT_ID}" ;;
esac
echo "  Device model: $DEVICE_MODEL"

# Get serial number
SERIAL=$(ssh "$REMOTE_HOST" "lsusb -v -d ${PRODUCT_ID} 2>/dev/null | grep iSerial | awk '{print \$3}'" || echo "unknown")
echo "  Serial: $SERIAL"

# ── Prepare output files ─────────────────────────────────────────────────

TIMESTAMP=$(date +%Y-%m-%d_%H%M%S)
DATE_HUMAN=$(date +"%Y-%m-%d %H:%M:%S %Z")
BASENAME="${DEVICE_MODEL}_${OPERATION}_${TIMESTAMP}"
PCAP_FILE="${CAPTURES_DIR}/${BASENAME}.pcap"
MD_FILE="${CAPTURES_DIR}/${BASENAME}.md"

mkdir -p "$CAPTURES_DIR"

echo ""
echo "=== Capture Setup ==="
echo "  Operation:   $OPERATION"
echo "  Duration:    ${DURATION}s"
echo "  Bus:         usbmon${BUS}"
echo "  Output:      $PCAP_FILE"
echo "  Companion:   $MD_FILE"

# ── Load usbmon on remote host ───────────────────────────────────────────

echo ""
echo "=== Loading usbmon on $REMOTE_HOST ==="
ssh "$REMOTE_HOST" "sudo modprobe usbmon 2>/dev/null && echo '  usbmon loaded'" || {
    echo "ERROR: Failed to load usbmon module on $REMOTE_HOST" >&2
    exit 1
}

# ── Build tcpdump command ─────────────────────────────────────────────────

TCPDUMP_CMD="sudo timeout $((DURATION + 2)) tcpdump -i usbmon${BUS} -w -"

if [ -n "$MAX_COUNT" ]; then
    TCPDUMP_CMD="$TCPDUMP_CMD -c $MAX_COUNT"
fi

# Use snaplen large enough for bulk transfers (default 245824 is fine)

echo ""
echo "=== Capturing (${DURATION}s, Ctrl-C to stop early) ==="
echo "  Remote cmd: $TCPDUMP_CMD"

# Run the capture, streaming pcap data back via SSH
# tcpdump writes pcap to stdout, stderr has statistics
CAPTURE_START=$(date +%s)

ssh "$REMOTE_HOST" "$TCPDUMP_CMD" > "$PCAP_FILE" 2>/tmp/usb-capture-stderr.txt || true

CAPTURE_END=$(date +%s)
ACTUAL_DURATION=$((CAPTURE_END - CAPTURE_START))

# Show capture stats from stderr
STATS=$(cat /tmp/usb-capture-stderr.txt 2>/dev/null || echo "(no stats)")
rm -f /tmp/usb-capture-stderr.txt

echo ""
echo "=== Capture Complete ==="
FILESIZE=$(stat -c%s "$PCAP_FILE" 2>/dev/null || echo "0")
echo "  File size:    $FILESIZE bytes"
echo "  Duration:     ~${ACTUAL_DURATION}s"
echo "  Stats:        $STATS"

if [ "$FILESIZE" -lt 100 ]; then
    echo "WARNING: Capture file is suspiciously small ($FILESIZE bytes)" >&2
    echo "  The device may not have generated any traffic." >&2
fi

# ── Count packets (if tshark is available locally) ────────────────────────

PACKET_COUNT="unknown"
if command -v tshark &>/dev/null; then
    PACKET_COUNT=$(tshark -r "$PCAP_FILE" 2>/dev/null | wc -l || echo "unknown")
    echo "  Packets:      $PACKET_COUNT"
fi

# ── Get USB descriptor snapshot ──────────────────────────────────────────

DESCRIPTORS=$(ssh "$REMOTE_HOST" "lsusb -v -d ${PRODUCT_ID} 2>&1" || echo "(descriptor dump failed)")

# ── Write companion .md file ─────────────────────────────────────────────

cat > "$MD_FILE" << MDEOF
# USB Capture: ${OPERATION}

## Capture Info

| Field             | Value                                      |
|-------------------|--------------------------------------------|
| Device            | ${DEVICE_MODEL} (${PRODUCT_ID})            |
| Serial            | ${SERIAL}                                  |
| HW Revision       | TBD                                        |
| Saleae Logic 2    | 2.4.40 (not running during capture)        |
| Date              | ${DATE_HUMAN}                              |
| Duration          | ~${ACTUAL_DURATION}s (requested ${DURATION}s) |
| Packets           | ${PACKET_COUNT}                            |
| File              | ${BASENAME}.pcap                           |
| File Size         | ${FILESIZE} bytes                          |
| Capture Host      | ${REMOTE_HOST}                             |
| USB Bus           | ${BUS}                                     |
| USB Device Addr   | ${DEV_ADDR}                                |
| Capture Tool      | tcpdump 4.99.0 via SSH                     |

## Operation Description

${NOTES:-TODO: Describe what operation was performed during this capture.}

## USB Descriptors at Capture Time

\`\`\`
${DESCRIPTORS}
\`\`\`

## Key Observations

TODO: Document interesting findings from analyzing this capture.

- Transfer types observed:
- Endpoint usage:
- Packet sizes:
- Notable patterns:

## Analysis Commands

\`\`\`bash
# View in Wireshark (if installed locally)
wireshark ${BASENAME}.pcap

# Summarize with tshark
tshark -r re/captures/${BASENAME}.pcap -q -z io,stat,1

# Show first 50 packets
tshark -r re/captures/${BASENAME}.pcap -c 50

# Filter by endpoint
tshark -r re/captures/${BASENAME}.pcap -Y "usb.endpoint_address == 0x01"

# Show control transfers
tshark -r re/captures/${BASENAME}.pcap -Y "usb.transfer_type == 2" -T fields \\
  -e frame.number -e frame.time_relative -e usb.setup.bRequest \\
  -e usb.setup.wValue -e usb.setup.wIndex -e usb.setup.wLength -e usb.capdata

# Hex dump of packet data
tshark -r re/captures/${BASENAME}.pcap -x -c 10
\`\`\`
MDEOF

echo ""
echo "=== Output Files ==="
echo "  Capture: $PCAP_FILE"
echo "  Notes:   $MD_FILE"
echo ""
echo "Next steps:"
echo "  1. Edit $MD_FILE to document observations"
echo "  2. Analyze with: tshark -r $PCAP_FILE"
echo "  3. For Wireshark: wireshark $PCAP_FILE"
