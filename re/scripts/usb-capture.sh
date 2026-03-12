#!/run/current-system/sw/bin/bash
OUTDIR="/home/mpedersen/nvidia/saleae_reverse/re/captures"
STAMP=$(date +%Y%m%d_%H%M%S)
/run/current-system/sw/bin/tcpdump -i usbmon0 -s 65535 -w "${OUTDIR}/usbmon0-${STAMP}.pcap" &
PID0=$!
/run/current-system/sw/bin/tcpdump -i usbmon4 -s 65535 -w "${OUTDIR}/usbmon4-${STAMP}.pcap" &
PID4=$!
echo "$PID0 $PID4" > /tmp/usb-capture-pids.txt
echo "Capturing usbmon0 (PID $PID0) and usbmon4 (PID $PID4) -> ${STAMP}"
trap "kill $PID0 $PID4 2>/dev/null" EXIT
wait
