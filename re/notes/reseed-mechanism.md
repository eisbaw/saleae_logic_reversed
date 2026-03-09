# ReseedDevice Mechanism Analysis

Investigation date: 2026-03-09
Binary: libgraph_server_shared.so v2.4.40
Task: TASK-0028
Source file: LinuxUsbDevice.cpp (from assertion string)


## Summary

ReseedDevice is NOT PID patching and NOT serial number assignment. It is a
**cryptographic seed exchange**: the host generates 4 random bytes, sends
them to the FX3 via EP1 OUT bulk transfer, and both sides use this shared
seed for XOR-based obfuscation of subsequent EP1 command traffic.

The name "reseed" means "re-establish the encryption seed." The seed is
regenerated before each capture session as part of the
`TestFirmwareResponsivnessAndReseed` sequence.


## ReseedDevice Function

Location: VA 0x2d91950 (in LinuxUsbDevice class)
Virtual function: called through vtable (no direct callers found)
Source: `LinuxUsbDevice.cpp` line ~175 (from assertion metadata)

### Algorithm

```
function ReseedDevice(endpoint):
    lock(this->mutex_0xa0)

    assert endpoint.Out()  // must be an OUT endpoint

    seed_buf = new U32(0)

    // Generate 4 cryptographically random bytes
    for attempt in 0..7:
        if RAND_bytes(seed_buf, 4) == 1:
            break
        if attempt == 6:
            throw "Random not seeded"

    // Prepare bulk transfer buffer (wraps seed in a vector)
    transfer_buf = prepare_buffer(seed_buf, this->endpoint_addr_0x60)

    // Send seed to device via EP1 OUT bulk transfer
    result = libusb_bulk_transfer(
        handle = this->usb_handle_0x90,
        endpoint = endpoint_addr,
        data = transfer_buf.data,
        length = transfer_buf.size,
        transferred = &actual,
        timeout = 2000  // 0x7D0 ms
    )

    if result != 0:
        throw "libusb_bulk_transfer failure (reseed)"
    if actual != expected_length:
        throw "usb_bulk_write sent the wrong number of bytes. " + ...

    // Store seed for later use by SimpleEncryption
    this->seed_0x64 = *seed_buf     // 4-byte seed value
    this->seed_valid_0x68 = false   // flag cleared (meaning: seed is set)

    unlock(this->mutex_0xa0)
```

### Key observations

1. Uses `RAND_bytes()` (OpenSSL) for cryptographic randomness, retries up
   to 7 times.
2. Sends the 4-byte seed via `libusb_bulk_transfer` directly (not through
   the vtable command dispatch used by other EP1 commands).
3. Stores the seed at `this->offset_0x64` for use by SimpleEncryption.
4. The transfer uses the same EP1 OUT endpoint as other commands.
5. Timeout is 2000ms (generous for a small write).


## TestFirmwareResponsivnessAndReseed

Location: VA 0x1c4e860
Source: `LogicAnalyzerDevice.cpp` line ~244 (from assertion metadata)

This function is the caller. It runs before each capture start (see
`doc/protocol/ep1-commands.md`, capture start sequence step 3).

### Algorithm

```
function TestFirmwareResponsivnessAndReseed():
    endpoints = get_device_endpoints()  // returns vector of UsbEndpoint
    assert endpoints.count == 3 or 4

    device_type = get_device_type()
    if device_type in {4, 5, 6, 7}:  // Logic 4, LogicStudent, Logic8, Pro8
        expected_endpoint_count = 3
        needs_reseed = true
    else:
        expected_endpoint_count = 4
        needs_reseed = false

    assert endpoints.size/8 == expected_endpoint_count

    // Parse endpoint list into EP1 OUT, EP1 IN, EP2 IN
    for ep in endpoints:
        type = classify_endpoint(ep)
        switch type:
            case 0x01: ep1_out = ep      // command write endpoint
            case 0x81: ep1_in = ep       // command read endpoint
            case 0x82: ep2_in = ep       // data stream endpoint
            case 0x06: throw "Detected a stream write EP on a 3 EP device"

    // Reseed if needed (FX3 devices with 3 endpoints)
    if needs_reseed:
        this->ReseedDevice(ep1_out)  // vtable[0x18] call

    // Ping test: send [0x7D, 0x55] on EP1 OUT, expect 0xAA on EP1 IN
    ping_data = [0x7D, 0x55]
    response = 0x00

    vtable[0x20](ep1_out, ping_data, 2, 0, 0)   // write 2 bytes
    vtable[0x28](ep1_in, &response, 1, 0, 0)    // read 1 byte

    if response == 0xAA:
        return true   // firmware is responsive

    // Retry up to 5 times total
    for retry in 0..4:
        vtable[0x20](ep1_out, ping_data, 2, 0, 0)
        vtable[0x28](ep1_in, &response, 1, 0, 0)
        if response == 0xAA:
            return true

    // All 5 attempts failed
    error = "ping command produced incorrect result"
    log("TestFirmwareResponsivnessAndReseed", "ping reply: {:#x}", response)
    return false
```

### Ping mechanism

The ping command `[0x7D, 0x55]` expects the firmware to return `0xAA`.

`0x55 XOR 0xFF = 0xAA`, which suggests the firmware complements the
second byte and returns it. This is a minimal liveness check: the
firmware can parse a 2-byte command, process it (bitwise NOT), and
return a result. If the firmware has crashed or the USB path is broken,
this will fail.

Opcode `0x7D` is not in the known command table -- it appears to be
a dedicated ping/echo command not documented elsewhere.


## SimpleEncryption (XOR Obfuscation)

Source: `SimpleEncryption.cpp`
Key function: `SetControlByte(U8 *, U8 *, U32, bool, bool)`

The `SetControlByte` function contains the assertion:
"Can't reset the device seed and download the firmware in one command"

This indicates that:
1. The "device seed" is the shared 4-byte value from ReseedDevice.
2. It is used in `SetControlByte` for XOR-based obfuscation of control
   data sent over EP1.
3. The seed cannot be reset while firmware download is in progress
   (mutual exclusion with the firmware upload state machine).

The XOR-based obfuscation is likely trivial: each byte of the EP1
command payload is XORed with a byte derived from the 4-byte seed
(possibly cycling through the seed bytes, or using the seed as a
PRNG state). This is NOT real encryption -- it is obfuscation to
prevent casual USB sniffing of commands.

### Object layout (relevant fields)

| Offset | Size | Name | Description |
|--------|------|------|-------------|
| 0x60 | 4 | endpoint_addr | EP1 OUT endpoint address byte |
| 0x64 | 4 | seed | Current 4-byte XOR seed |
| 0x68 | 1 | seed_flag | Cleared after reseed |
| 0x90 | 8 | usb_handle | libusb device handle pointer |
| 0xA0 | ? | mutex | Mutex protecting reseed/command ops |


## Corrections to Prior Documentation

1. **ep1-commands.md**: ReseedDevice (0x22) entry is WRONG. The 0x22
   byte found earlier was actually an ASCII double-quote character in
   the "Random not seeded" error string construction, not a command
   opcode. ReseedDevice does not use a single-byte opcode -- it sends
   4 random seed bytes directly via bulk transfer.

2. **logic4-8-firmware.md**: States "the reseed mechanism patches the
   USB PID to match the device's actual identity." This is INCORRECT.
   ReseedDevice has nothing to do with PID patching. The PID patching
   occurs during firmware upload (the firmware binary itself contains
   the PID; the FX3 re-enumerates with whatever PID the firmware
   specifies). The "reseed" establishes a shared encryption seed.

3. **Opcode 0x7D**: A new opcode `0x7D` (ping/echo) was discovered.
   Payload is 1 byte, response is the bitwise complement. This should
   be added to the EP1 command table.


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| ReseedDevice sends 4 random bytes | High | RAND_bytes(4) + libusb_bulk_transfer in disassembly |
| Seed stored at this+0x64 | High | MOV instruction stores RAND_bytes result |
| Used by SimpleEncryption | High | Same source file, SetControlByte function |
| XOR-based obfuscation | Medium | "SimpleEncryption.cpp" name + SetControlByte signature |
| Ping command 0x7D returns complement | High | Sends 0x55, expects 0xAA (complement), clear code path |
| Not PID patching | Confirmed | No PID-related code in ReseedDevice function |
| Not serial number assignment | Confirmed | Seed is random, not device-specific |
| Called before each capture | High | In TestFirmwareResponsivnessAndReseed, which is step 3 of capture start |


## Open Questions

1. **Exact XOR scheme**: How does SetControlByte use the 4-byte seed?
   Rolling XOR? PRNG seeded with the 4 bytes? The disassembly of
   SetControlByte in SimpleEncryption.cpp has not been analyzed yet.

2. **What does the firmware do with the seed?** The FX3 firmware
   presumably stores the seed and uses it to decode incoming commands
   and encode outgoing responses. The exact firmware-side implementation
   is unknown.

3. **Which commands are obfuscated?** Are all EP1 commands XOR'd after
   the reseed, or only certain ones? The vtable dispatch at [0x20] and
   [0x28] may apply the XOR transparently.

4. **Why obfuscate at all?** Possibly anti-cloning (prevent trivial USB
   replay), or to make casual protocol sniffing harder for competitors.
   The "encryption" is trivially breakable since both sides use the
   same seed with XOR.

5. **Opcode 0x7D response format**: Does the firmware return exactly 1
   byte (the complement), or is there a header/status byte?


## References

- `doc/protocol/ep1-commands.md` -- EP1 command protocol (needs 0x22 correction)
- `re/notes/ep1-command-protocol.md` -- detailed analysis notes
- `re/notes/logic4-8-firmware.md` -- firmware mapping (PID patching correction)
- `re/notes/crypto-authentication.md` -- separate crypto IC investigation
