#!/usr/bin/env bash
# find-usb-constants.sh -- Extract USB-related constants from libgraph_server_shared.so
#
# Searches for libusb API references, USB endpoint numbers, vendor request
# codes, Cypress FX3 constants, FPGA-related strings, and device class
# information. Since libusb is statically linked into the binary, we rely
# on error messages, debug strings, and API function name strings to identify
# usage patterns.
#
# Usage: ./find-usb-constants.sh [path-to-so]

set -euo pipefail

BINARY="${1:-/nix/store/brcaw41j62pjki70ahf7ffd2f3bhaa3d-saleae-logic-2-extracted/resources/linux-x64/libgraph_server_shared.so}"

if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found: $BINARY" >&2
    exit 1
fi

echo "============================================================"
echo "USB Constants Analysis: $(basename "$BINARY")"
echo "Binary: $BINARY"
echo "Date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "============================================================"
echo

# ---------------------------------------------------------------
# Section 1: libusb API function references
# ---------------------------------------------------------------
echo "=== 1. LIBUSB API FUNCTIONS (exported symbols) ==="
echo
nm -D "$BINARY" 2>/dev/null | grep ' T libusb_' | awk '{print $3}' | sort
echo
echo "Count: $(nm -D "$BINARY" 2>/dev/null | grep -c ' T libusb_')"
echo

# ---------------------------------------------------------------
# Section 2: libusb error and status strings
# ---------------------------------------------------------------
echo "=== 2. LIBUSB ERROR AND STATUS STRINGS ==="
echo
strings "$BINARY" | grep -E '^LIBUSB_(ERROR|TRANSFER|SUCCESS)' | sort -u
echo

# ---------------------------------------------------------------
# Section 3: libusb call-site error messages (protocol boundaries)
# ---------------------------------------------------------------
echo "=== 3. LIBUSB CALL-SITE ERROR MESSAGES ==="
echo "(These reveal which libusb functions are actually called)"
echo
strings "$BINARY" | grep -E 'libusb_[a-z_]+.*(fail|error|:)' | sort -u
echo

# ---------------------------------------------------------------
# Section 4: USB control transfer wrapper
# ---------------------------------------------------------------
echo "=== 4. USB CONTROL TRANSFER REFERENCES ==="
echo
strings "$BINARY" | grep -iE '(SendControlTransfer|UsbControlSetupPacket|control_transfer|submit_control)' | sort -u
echo

# ---------------------------------------------------------------
# Section 5: USB bulk transfer references
# ---------------------------------------------------------------
echo "=== 5. USB BULK TRANSFER REFERENCES ==="
echo
strings "$BINARY" | grep -iE '(BulkRead|BulkWrite|bulk_transfer|BulkReadStream|TransferQueue)' | grep -v '^_Z' | grep -v '^NSt' | sort -u
echo

# ---------------------------------------------------------------
# Section 6: USB endpoint references
# ---------------------------------------------------------------
echo "=== 6. USB ENDPOINT REFERENCES ==="
echo
strings "$BINARY" | grep -iE '(endpoint|UsbEndpoint|FindEndpoint)' | grep -v '^_Z' | grep -v '^NSt' | grep -v '^\s' | sort -u
echo

# ---------------------------------------------------------------
# Section 7: Cypress FX3 firmware constants
# ---------------------------------------------------------------
echo "=== 7. CYPRESS FX3 CONSTANTS ==="
echo
strings "$BINARY" | grep -iE '(CyFx|CY_U3P|FX3|@CYWB|cypress|Firmware.*error|Firmware.*download|Firmware.*upload|DownloadFirmware|firmware.*signature)' | grep -v '^_Z' | grep -v '^NSt' | sort -u
echo

# ---------------------------------------------------------------
# Section 8: FPGA bitstream references
# ---------------------------------------------------------------
echo "=== 8. FPGA BITSTREAM REFERENCES ==="
echo
strings "$BINARY" | grep -iE '(bitstream|ProgramFpga|DownloadBitstream|LoadBitstream|ResetFpga|FPGA.*program|FPGA.*register|Altera|Xilinx|Lattice|Spartan)' | grep -v '^_Z' | grep -v '^NSt' | sort -u
echo

# ---------------------------------------------------------------
# Section 9: Bitstream CRC values (identify firmware variants)
# ---------------------------------------------------------------
echo "=== 9. BITSTREAM CRC VALUES ==="
echo
strings "$BINARY" | grep -i 'Bitstream CRC' | sort -u
echo

# ---------------------------------------------------------------
# Section 10: Device class hierarchy
# ---------------------------------------------------------------
echo "=== 10. DEVICE CLASS HIERARCHY ==="
echo
strings "$BINARY" | grep -E '^[0-9]+(UsbDevice|LinuxUsbDevice|Logic|Fpga|Fx[23]|Pro|Device)' | sort -u
echo

# ---------------------------------------------------------------
# Section 11: Device-specific method signatures
# ---------------------------------------------------------------
echo "=== 11. KEY METHOD SIGNATURES (protocol boundary functions) ==="
echo
strings "$BINARY" | grep -E '^virtual.*(LinuxUsbDevice|UsbDevice|Fx[23]DeviceFeatures|FpgaDeviceFeatures)' | sort -u
echo

# ---------------------------------------------------------------
# Section 12: EEPROM and I2C (used over USB for device config)
# ---------------------------------------------------------------
echo "=== 12. EEPROM AND I2C REFERENCES ==="
echo
strings "$BINARY" | grep -iE '(ReadEeprom|WriteEeprom|eeprom|TransferI2c|ReadI2c|WriteI2C|i2c.*tunnel)' | grep -v '^_Z' | grep -v '^NSt' | grep -v 'i2c_ASN1' | sort -u
echo

# ---------------------------------------------------------------
# Section 13: ADC and AFE chip references
# ---------------------------------------------------------------
echo "=== 13. ADC AND AFE HARDWARE REFERENCES ==="
echo
strings "$BINARY" | grep -iE '(Hmcad1100|AD9637|ADC.*Read|ADC.*Write|ADC.*Vendor|ADC.*Lock|ADC.*calibrat|AfeConfig|afeChannel|afe.*celsius)' | grep -v '^_Z' | grep -v '^NSt' | sort -u
echo

# ---------------------------------------------------------------
# Section 14: Source file paths (reveals code organization)
# ---------------------------------------------------------------
echo "=== 14. SOURCE FILE PATHS (original build tree) ==="
echo
strings "$BINARY" | grep '/workspace/monorepo/graph-io/' | sort -u
echo

# ---------------------------------------------------------------
# Section 15: VID/PID and device identification
# ---------------------------------------------------------------
echo "=== 15. VID/PID AND DEVICE IDENTIFICATION ==="
echo
strings "$BINARY" | grep -iE '(GetVidPid|SetVidPid|vid.*pid|DeviceType|DeviceInstanceId|SERNUM|21a9)' | grep -v '^_Z' | grep -v '^NSt' | sort -u
echo

echo "============================================================"
echo "Analysis complete."
echo "============================================================"
