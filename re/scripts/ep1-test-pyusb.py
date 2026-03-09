#!/usr/bin/env python3
"""Test EP1 bulk transfers on Saleae Logic Pro 8 using pyusb/libusb.

Tests whether sending a ResetAndReEnumerate command (0x82) as the FIRST
command on a stale device will reset it, and whether the device then
works properly after re-enumeration.
"""

import sys
import time
import usb.core
import usb.util

SALEAE_VID = 0x21a9
SALEAE_PID = 0x1005

EP1_OUT = 0x01
EP1_IN = 0x81
TIMEOUT_MS = 3000

def find_and_open():
    dev = usb.core.find(idVendor=SALEAE_VID, idProduct=SALEAE_PID)
    if dev is None:
        return None
    if dev.is_kernel_driver_active(0):
        dev.detach_kernel_driver(0)
    dev.set_configuration(1)
    usb.util.claim_interface(dev, 0)
    return dev


def test_ep1(dev, label):
    """Send reseed + ping, return True if ping response received."""
    seed_bytes = bytes([0x01, 0x02, 0x03, 0x04])

    print(f"  [{label}] Sending reseed (4 bytes)...")
    try:
        n = dev.write(EP1_OUT, seed_bytes, timeout=TIMEOUT_MS)
        print(f"    OUT: {n} bytes. OK")
    except usb.core.USBTimeoutError:
        print("    OUT: TIMEOUT (DMA buffer not ready)")
        return False
    except usb.core.USBError as e:
        print(f"    OUT: {e}")
        return False

    # Try second OUT (will hang if firmware not consuming).
    ping_bytes = bytes([0x7D, 0x55])
    print(f"  [{label}] Sending raw ping (2 bytes)...")
    try:
        n = dev.write(EP1_OUT, ping_bytes, timeout=TIMEOUT_MS)
        print(f"    OUT: {n} bytes. OK (firmware is processing commands!)")
    except usb.core.USBTimeoutError:
        print("    OUT: TIMEOUT (firmware not consuming)")
        return False
    except usb.core.USBError as e:
        print(f"    OUT: {e}")
        return False

    # Read response.
    print(f"  [{label}] Reading response...")
    try:
        data = dev.read(EP1_IN, 1024, timeout=TIMEOUT_MS)
        hex_str = data.tobytes().hex()
        print(f"    IN: {len(data)} bytes: {hex_str}")
        return True
    except usb.core.USBTimeoutError:
        print("    IN: TIMEOUT")
        return False
    except usb.core.USBError as e:
        print(f"    IN: {e}")
        return False


def main():
    dev = usb.core.find(idVendor=SALEAE_VID, idProduct=SALEAE_PID)
    if dev is None:
        print("No Saleae device found.")
        return 1

    bcd = dev.bcdDevice
    print(f"Found device: {dev.manufacturer} {dev.product}")
    print(f"  bcdDevice: {bcd:#06x}")
    print(f"  Bus {dev.bus} Addr {dev.address}")
    print()

    # Phase 1: Try EP1 on current state.
    print("=== Phase 1: Test EP1 on current device state ===")
    if dev.is_kernel_driver_active(0):
        dev.detach_kernel_driver(0)
    dev.set_configuration(1)
    usb.util.claim_interface(dev, 0)

    if test_ep1(dev, "current"):
        print()
        print("EP1 is working on current device state!")
        usb.util.release_interface(dev, 0)
        return 0

    print()

    # Phase 2: Send ResetAndReEnumerate (0x82) as the first command.
    # Since the first OUT always succeeds (goes into DMA buffer), we can
    # send a single-byte command. If the firmware processes it, the device
    # will reset and re-enumerate.
    print("=== Phase 2: Send ResetAndReEnumerate (0x82) ===")
    # Need to re-init since the previous test left DMA in bad state.
    usb.util.release_interface(dev, 0)
    dev.reset()
    time.sleep(1)

    # Re-find device.
    dev = usb.core.find(idVendor=SALEAE_VID, idProduct=SALEAE_PID)
    if dev is None:
        print("Device not found after reset!")
        return 1

    if dev.is_kernel_driver_active(0):
        dev.detach_kernel_driver(0)
    dev.set_configuration(1)
    usb.util.claim_interface(dev, 0)

    print("  Sending [0x82] on EP1 OUT (ResetAndReEnumerate)...")
    try:
        n = dev.write(EP1_OUT, bytes([0x82]), timeout=TIMEOUT_MS)
        print(f"    OUT: {n} byte. OK")
    except usb.core.USBTimeoutError:
        print("    OUT: TIMEOUT")
    except usb.core.USBError as e:
        print(f"    OUT: {e}")

    # Wait for potential re-enumeration.
    print("  Waiting 5s for re-enumeration...")
    usb.util.release_interface(dev, 0)
    del dev
    time.sleep(5)

    # Check if device re-appeared.
    dev = usb.core.find(idVendor=SALEAE_VID, idProduct=SALEAE_PID)
    if dev is None:
        # Maybe it went back to bootloader? Check for bootloader VID:PID.
        # Actually FX3 bootloader uses same VID:PID with different bcdDevice.
        print("  Device not found! Checking more broadly...")
        all_devs = usb.core.find(find_all=True, idVendor=SALEAE_VID)
        for d in all_devs:
            print(f"    Found: {d.idVendor:#06x}:{d.idProduct:#06x} bcdDevice={d.bcdDevice:#06x}")
        if not list(usb.core.find(find_all=True, idVendor=SALEAE_VID)):
            print("  No Saleae devices found at all.")
        return 1

    new_bcd = dev.bcdDevice
    print(f"  Device found: bcdDevice={new_bcd:#06x}")

    if new_bcd != bcd:
        print(f"  bcdDevice changed from {bcd:#06x} to {new_bcd:#06x}!")
    else:
        print("  bcdDevice unchanged.")

    # Try EP1 again.
    print()
    print("=== Phase 3: Test EP1 after ResetAndReEnumerate ===")
    if dev.is_kernel_driver_active(0):
        dev.detach_kernel_driver(0)
    dev.set_configuration(1)
    usb.util.claim_interface(dev, 0)

    if test_ep1(dev, "after-reset"):
        print()
        print("EP1 works after ResetAndReEnumerate!")
        usb.util.release_interface(dev, 0)
        return 0

    usb.util.release_interface(dev, 0)
    print()
    print("EP1 still not working after ResetAndReEnumerate.")
    print()
    print("CONCLUSION: The firmware's command processing loop is not")
    print("consuming data from the EP1 DMA buffer. A physical power")
    print("cycle is needed to reset the FX3 to bootloader mode.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
