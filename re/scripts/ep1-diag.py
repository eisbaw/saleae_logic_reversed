#!/usr/bin/env python3
"""
Comprehensive EP1 diagnostic for Saleae Logic Pro 8.
Tests various USB interactions to determine why the EP1 command thread
isn't consuming data after firmware upload.
"""
import usb.core
import usb.util
import time
import sys
import struct

VID, PID = 0x21a9, 0x1005

def find_device():
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if dev is None:
        print("ERROR: Saleae device not found")
        sys.exit(1)
    print(f"Found: {dev.manufacturer} {dev.product}")
    print(f"  Bus {dev.bus} Addr {dev.address}")
    print(f"  Serial: {dev.serial_number}")
    print(f"  bcdDevice: {dev.bcdDevice:#06x}")
    print(f"  Speed: {dev.speed}")
    return dev

def test_control_transfers(dev):
    """Test if firmware responds to various control transfers."""
    print("\n=== Control Transfer Tests ===")

    # GET_STATUS (standard)
    try:
        status = dev.ctrl_transfer(0x80, 0, 0, 0, 2, timeout=1000)
        print(f"  GET_STATUS(device): {status.tobytes().hex()} -> OK")
    except Exception as e:
        print(f"  GET_STATUS(device): FAIL ({e})")

    # GET_DESCRIPTOR (Device)
    try:
        desc = dev.ctrl_transfer(0x80, 6, 0x0100, 0, 18, timeout=1000)
        print(f"  GET_DESCRIPTOR(device): {len(desc)} bytes -> OK")
    except Exception as e:
        print(f"  GET_DESCRIPTOR(device): FAIL ({e})")

    # Vendor IN request (bRequest=0xA0, the FW upload request, but as IN)
    # This might trigger a response if the firmware has a vendor handler
    for req in [0xA0, 0xB0, 0x01, 0x7D]:
        try:
            resp = dev.ctrl_transfer(0xC0, req, 0, 0, 64, timeout=500)
            print(f"  VENDOR_IN(0x{req:02x}): {resp.tobytes().hex()} -> OK")
        except usb.core.USBTimeoutError:
            print(f"  VENDOR_IN(0x{req:02x}): TIMEOUT")
        except Exception as e:
            print(f"  VENDOR_IN(0x{req:02x}): {e}")

    # Vendor OUT request - try a few
    for req in [0x01, 0xA0]:
        try:
            n = dev.ctrl_transfer(0x40, req, 0, 0, b'\x00', timeout=500)
            print(f"  VENDOR_OUT(0x{req:02x}): sent {n} bytes -> OK")
        except usb.core.USBTimeoutError:
            print(f"  VENDOR_OUT(0x{req:02x}): TIMEOUT")
        except Exception as e:
            print(f"  VENDOR_OUT(0x{req:02x}): {e}")

def test_ep1_raw(dev):
    """Test EP1 bulk transfers with various approaches."""
    print("\n=== EP1 Bulk Transfer Tests ===")

    # Ensure we have the interface
    cfg = dev.get_active_configuration()
    intf = cfg[(0, 0)]
    print(f"  Interface: {intf.bInterfaceNumber}, AltSetting: {intf.bAlternateSetting}")
    print(f"  Endpoints: {[f'0x{ep.bEndpointAddress:02x}' for ep in intf]}")

    # Test 1: Try reading from EP1 IN first (no prior write)
    print("\n  [Test 1] Read EP1 IN without prior write...")
    try:
        data = dev.read(0x81, 64, timeout=500)
        print(f"    Got {len(data)} bytes: {data.tobytes().hex()}")
    except usb.core.USBTimeoutError:
        print("    TIMEOUT (expected - no data queued)")
    except Exception as e:
        print(f"    ERROR: {e}")

    # Test 2: Send 1 byte raw on EP1 OUT
    print("\n  [Test 2] Write 1 byte (0x7D) to EP1 OUT...")
    try:
        n = dev.write(0x01, bytes([0x7D]), timeout=2000)
        print(f"    Sent {n} byte(s) -> OK")
    except usb.core.USBTimeoutError:
        print("    TIMEOUT (DMA full or not started)")
        return
    except Exception as e:
        print(f"    ERROR: {e}")
        return

    # Test 3: Try reading response
    print("\n  [Test 3] Read EP1 IN after 1-byte write...")
    try:
        data = dev.read(0x81, 64, timeout=2000)
        print(f"    Got {len(data)} bytes: {data.tobytes().hex()}")
    except usb.core.USBTimeoutError:
        print("    TIMEOUT (firmware didn't respond)")
    except Exception as e:
        print(f"    ERROR: {e}")

    # Test 4: Send more bytes to check if DMA drains
    print("\n  [Test 4] Write 2 bytes [0x7D, 0x55] to EP1 OUT...")
    try:
        n = dev.write(0x01, bytes([0x7D, 0x55]), timeout=2000)
        print(f"    Sent {n} byte(s) -> OK")
    except usb.core.USBTimeoutError:
        print("    TIMEOUT (DMA full - firmware NOT consuming)")
        return
    except Exception as e:
        print(f"    ERROR: {e}")
        return

    # Test 5: Read again
    print("\n  [Test 5] Read EP1 IN...")
    try:
        data = dev.read(0x81, 64, timeout=2000)
        print(f"    Got {len(data)} bytes: {data.tobytes().hex()}")
    except usb.core.USBTimeoutError:
        print("    TIMEOUT")
    except Exception as e:
        print(f"    ERROR: {e}")

    # Test 6: Send the actual reseed (4 bytes) + encrypted ping
    print("\n  [Test 6] Reseed [01,02,03,04] + Ping [0x7D, 0x55] (raw, unencrypted)...")
    try:
        n = dev.write(0x01, bytes([0x01, 0x02, 0x03, 0x04]), timeout=2000)
        print(f"    Reseed: sent {n} bytes -> OK")
    except usb.core.USBTimeoutError:
        print("    Reseed: TIMEOUT")
        return
    except Exception as e:
        print(f"    Reseed: ERROR: {e}")
        return

    time.sleep(0.1)

    try:
        n = dev.write(0x01, bytes([0x7D, 0x55]), timeout=2000)
        print(f"    Ping: sent {n} bytes -> OK")
    except usb.core.USBTimeoutError:
        print("    Ping: TIMEOUT (DMA didn't drain after reseed)")
        return
    except Exception as e:
        print(f"    Ping: ERROR: {e}")
        return

    time.sleep(0.1)

    try:
        data = dev.read(0x81, 64, timeout=2000)
        print(f"    Response: {len(data)} bytes: {data.tobytes().hex()}")
    except usb.core.USBTimeoutError:
        print("    Response: TIMEOUT")
    except Exception as e:
        print(f"    Response: ERROR: {e}")

def test_with_set_interface(dev):
    """Test EP1 with explicit SET_INTERFACE."""
    print("\n=== SET_INTERFACE Tests ===")

    # Try alt setting 0
    try:
        dev.set_interface_altsetting(0, 0)
        print("  set_interface_altsetting(0, 0): OK")
    except Exception as e:
        print(f"  set_interface_altsetting(0, 0): {e}")

    # Wait for firmware to process
    time.sleep(0.5)

    # Try EP1 OUT
    print("  Write 4 bytes (reseed) to EP1 OUT after SET_INTERFACE...")
    try:
        n = dev.write(0x01, bytes([0x05, 0x06, 0x07, 0x08]), timeout=2000)
        print(f"    Sent {n} bytes -> OK")
    except usb.core.USBTimeoutError:
        print("    TIMEOUT")
        return
    except Exception as e:
        print(f"    ERROR: {e}")
        return

    print("  Write 2 bytes (raw ping) to EP1 OUT...")
    try:
        n = dev.write(0x01, bytes([0x7D, 0x55]), timeout=2000)
        print(f"    Sent {n} bytes -> OK")
    except usb.core.USBTimeoutError:
        print("    TIMEOUT (DMA not draining)")
        return
    except Exception as e:
        print(f"    ERROR: {e}")
        return

    print("  Read EP1 IN...")
    try:
        data = dev.read(0x81, 64, timeout=2000)
        print(f"    Got {len(data)} bytes: {data.tobytes().hex()}")
    except usb.core.USBTimeoutError:
        print("    TIMEOUT")
    except Exception as e:
        print(f"    ERROR: {e}")

def test_lpm_disable(dev):
    """Try disabling USB3 Link Power Management."""
    print("\n=== LPM Disable Test ===")

    # Try to disable U1/U2 via SET_FEATURE/CLEAR_FEATURE
    # USB_DEVICE_U1_ENABLE = 48, USB_DEVICE_U2_ENABLE = 49
    for feat, name in [(48, "U1"), (49, "U2")]:
        try:
            # CLEAR_FEATURE (bmReqType=0x00, bReq=1)
            dev.ctrl_transfer(0x00, 1, feat, 0, None, timeout=1000)
            print(f"  CLEAR_FEATURE({name}_ENABLE): OK")
        except Exception as e:
            print(f"  CLEAR_FEATURE({name}_ENABLE): {e}")

    time.sleep(0.5)

    # Now try EP1
    print("  Write 4 bytes to EP1 OUT after LPM disable...")
    try:
        n = dev.write(0x01, bytes([0x0A, 0x0B, 0x0C, 0x0D]), timeout=2000)
        print(f"    Sent {n} bytes -> OK")
    except usb.core.USBTimeoutError:
        print("    TIMEOUT")
        return
    except Exception as e:
        print(f"    ERROR: {e}")
        return

    print("  Write 2 bytes (raw ping)...")
    try:
        n = dev.write(0x01, bytes([0x7D, 0x55]), timeout=2000)
        print(f"    Sent {n} bytes -> OK")
    except usb.core.USBTimeoutError:
        print("    TIMEOUT (DMA still not draining)")
    except Exception as e:
        print(f"    ERROR: {e}")

def main():
    dev = find_device()

    # Detach kernel driver if attached
    for i in range(1):
        if dev.is_kernel_driver_active(i):
            dev.detach_kernel_driver(i)
            print(f"Detached kernel driver from interface {i}")

    # Set configuration
    try:
        dev.set_configuration(1)
        print("set_configuration(1): OK")
    except Exception as e:
        print(f"set_configuration(1): {e}")

    test_control_transfers(dev)
    test_ep1_raw(dev)
    test_with_set_interface(dev)
    test_lpm_disable(dev)

    print("\n=== Done ===")

if __name__ == '__main__':
    main()
