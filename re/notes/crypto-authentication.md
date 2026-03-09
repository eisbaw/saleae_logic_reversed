# Crypto/Authentication Protocol Analysis -- Device Anti-Clone

Analysis date: 2026-03-09
Binary: libgraph_server_shared.so (49,762,176 bytes, ELF 64-bit x86-64)
Source: Saleae Logic 2 v2.4.40
Method: String extraction, RTTI analysis, demangled symbol analysis, IPC
        message catalog cross-reference, JS bundle inspection


## Summary

Saleae Logic devices contain a hardware crypto authentication IC (Microchip
ATECC508A or ATECC608A family) that implements an ECDSA-based anti-clone
mechanism. The host software verifies the device is genuine during
initialization, AFTER FPGA programming and BEFORE the device transitions
to the Ready (operational) state.

The authentication is **host-software enforced, not firmware enforced**. The
FX3 firmware acts as a transparent I2C/SPI tunnel to the crypto IC -- it
does not independently gate device operation on authentication results. This
means an open-source driver can operate genuine Saleae hardware without
implementing the crypto authentication protocol. The crypto IC verification
is a DRM/anti-clone measure in the Saleae application, not a hardware lock.


## Crypto IC Identification

### Hardware: ATECC508A/608A (Microchip)

The crypto IC is identified as an ATECC508A or ATECC608A based on:

1. **32-byte block operations**: All read/write operations use 32-byte blocks
   - `data.size() == CryptoInterface::BlockSize` (assertion)
   - `offset % CryptoInterface::BlockSize == 0` (alignment assertion)
   - `ReadConfig32Bytes`, `ReadData32Bytes`, `WriteData32Bytes`
   - ATECC508A/608A use 32-byte data blocks

2. **Zone-based storage model**: Config zone, Data zone, OTP zone
   - `ReadConfig32Bytes(offset)` -- config zone (88 bytes usable)
   - `ReadData32Bytes(slot, block)` -- data zone (16 slots)
   - `WriteOTP32Bytes(offset, data)` -- one-time programmable zone
   - This matches the ATECC508A/608A memory architecture exactly

3. **Slot-based key storage**: `slot < 16` assertion
   - ATECC508A/608A has exactly 16 slots (0-15)
   - `GetPublicKey(slot)`, `NewPrivateKey(slot)`, `SignDigest(slot, digest)`

4. **I2C wake token**: `"Failed to send i2c wake token"`
   - ATECC508A/608A requires a low-level I2C wake (zero-byte write to
     address 0x00) before communication

5. **ECDSA operations**: `ECDSASignature::VerifyDigest`, `ECKey::SignDigest`
   - ATECC508A supports ECDSA P-256 (NIST P-256 / secp256r1)
   - Binary contains P-256 and prime256v1 curve references

6. **Challenge-response with nonce**:
   - `"nonce not returned"`, `"nonce mismatch"`, `"CHALLENGE_IS_DIFFERENT"`
   - Matches the ATECC Nonce command (opcode 0x16)


## Crypto Interface Class Hierarchy

### Three Transport Implementations

```
Saleae::CryptoInterface (abstract base)
  +-- Saleae::CryptoInterfaceI2c     -- FX2 devices (direct I2C bus)
  +-- Saleae::CryptoInterfaceFpga    -- FX3/FPGA devices (tunneled through FPGA registers)
  +-- Saleae::CryptoInterfacePro     -- Logic Pro devices (tunneled through FPGA)
```

Evidence from RTTI:
- `N6Saleae15CryptoInterfaceE` (base class)
- `N6Saleae18CryptoInterfaceI2cE`
- `N6Saleae19CryptoInterfaceFpgaE`
- `N6Saleae18CryptoInterfaceProE`

All three are heap-allocated via `make_shared` (confirmed by
`__shared_ptr_emplace` RTTI entries).

### Device-to-CryptoInterface Mapping

| Device | VID:PID | USB Controller | CryptoInterface | Transport Path |
|--------|---------|----------------|-----------------|----------------|
| Logic (original) | 0925:3881 | FX2 | CryptoInterfaceI2c | Direct I2C via vendor control transfer |
| Logic 16 | 21a9:1001 | FX2LP | CryptoInterfaceI2c | Direct I2C via vendor control transfer |
| Logic 4 | 21a9:1003 | FX3 | CryptoInterfaceFpga | I2C tunneled through FPGA registers |
| Logic 8 | 21a9:1004 | FX3 | CryptoInterfaceFpga | I2C tunneled through FPGA registers |
| Logic Pro 8 | 21a9:1005 | FX3 | CryptoInterfacePro | I2C tunneled through FPGA registers |
| Logic Pro 16 | 21a9:1006 | FX3 | CryptoInterfacePro | I2C tunneled through FPGA registers |
| MSO | -- | -- | CryptoInterfaceI2c | I2C via MSO device interface |

### Hardware Revision Determines Crypto Path

```cpp
bool RevisionDetail::UsesNewCryptoInterface()
```

This method on `RevisionDetail` selects between old (I2C direct) and new
(FPGA tunneled) crypto interface variants based on hardware revision. Newer
hardware revisions of all device types likely use the FPGA-tunneled path
exclusively.

### MSO Crypto Construction

For MSO devices, the crypto interface is constructed via:
```cpp
void Saleae::MsoDeviceInterface::BuildCryptoInterface()
```

And used via:
```cpp
void Saleae::MsoLogicAnalyzerInstrument::WithCryptoInterface(
    std::function<void(CryptoInterface&)>)
```

The MSO builds a `CryptoInterfaceI2c` using an I2C callback that tunnels
through the MSO device interface (lambda closure captured in the RTTI).

### FX2 Crypto Construction

For FX2 devices:
```cpp
Saleae::CryptoInterfaceI2cFromFx2DeviceFeatures(Fx2DeviceFeatures&)
```

This factory function creates a CryptoInterfaceI2c that uses the
Fx2DeviceFeatures I2C methods (opcode 0x87 WriteI2C, opcode 0x88 ReadI2c)
to communicate with the crypto IC.


## CryptoInterface API

### Base Class Methods (from assertion/function signature strings)

```cpp
class Saleae::CryptoInterface {
public:
    static constexpr size_t BlockSize = 32;

    // Config zone operations
    void ReadConfig32Bytes(size_t offset, std::vector<uint8_t>& data);
    void ReadConfigFourBytes(size_t offset, std::vector<uint8_t>& data);
    void WriteConfigFourBytes(size_t offset, std::span<uint8_t> data);

    // Data zone operations (slot-based)
    void ReadData32Bytes(size_t slot, size_t block, std::vector<uint8_t>& data);
    void WriteData32Bytes(size_t slot, size_t block, std::span<uint8_t> data);

    // OTP zone
    void WriteOTP32Bytes(size_t offset, std::span<uint8_t> data);

    // Key operations
    ECKey GetPublicKey(size_t slot);
    ECKey NewPrivateKey(size_t slot);
    ECDSASignature SignDigest(size_t slot, std::span<uint8_t> digest);
};
```

### Virtual Method (per-transport implementation)

```cpp
// The core I/O method -- each subclass implements this differently
virtual CryptoInterface::ErrorCode ReadTransmission(
    bool is_write,    // direction
    size_t length,    // number of bytes
    std::vector<uint8_t>& data
);
```

All three subclasses override `ReadTransmission`:
- `CryptoInterfaceFpga::ReadTransmission` -- reads/writes via FPGA register tunnel
- `CryptoInterfaceI2c::ReadTransmission` -- reads/writes via I2C bus
- `CryptoInterfacePro::ReadTransmission` -- reads/writes via Pro FPGA registers

### EC Key and Signature Types

```cpp
class Saleae::ECKey {
    // Construction
    static ECKey FromPublicKeyCoordinates(const CryptoBigNum& x, const CryptoBigNum& y);
    static ECKey FromCompressedPublicKey(const std::span<uint8_t>);
    static ECKey FromPEMPublicKey(const std::string&);
    static ECKey NewPair();  // Generate new key pair

    // Export
    std::string PEMPublicKey() const;
    std::vector<uint8_t> CompressedPublicKey() const;

    // Signing
    ECDSASignature SignDigest(std::span<uint8_t> digest) const;
};

class Saleae::ECDSASignature {
    static ECDSASignature FromPEM(const std::string&);
    bool VerifyDigest(const ECKey& key, std::span<uint8_t> digest);
};

class Saleae::CryptoBigNum {
    std::vector<uint8_t> ToBigEndianBytes(size_t length) const;
};

// Utility
std::vector<uint8_t> Saleae::CryptoRandomBytes(size_t count);
```

The EC operations use OpenSSL internally:
- `EC_KEY_set_public_key_affine_coordinates(key.mKey, x.mNum, y.mNum)`
- `EC_KEY_set_public_key(key.mKey, pubkey_point)`
- ECDSA verification via `ECDSA_do_verify` and related OpenSSL functions
- P-256 curve (NIST P-256 / prime256v1 / secp256r1)


## Authentication Flow

### When Authentication Happens

Authentication occurs during device initialization, as the LAST step before
the device transitions from `DSInitializing` to `DSReady`:

```
1. USB hotplug detection
2. FX3 firmware upload (vendor control transfers)
3. Device re-enumeration with Saleae VID:PID
4. Endpoint verification (must have 4 endpoints)
5. IsDeviceResponsive check
6. TestFirmwareResponsivnessAndReseed (opcode 0x22)
7. ProgramFpgaBitstream (opcodes 0x7E, 0x7F)
8. IsFpgaResponsive check
9. CommonDeviceInit (ADC init, PLL, JESD lock)
10. ** CryptoInterface authentication ** <--- HERE
11. DSInitializing -> DSReady (device is operational)
```

This timing is critical: authentication happens AFTER the FPGA is programmed,
meaning the FPGA bitstream loads unconditionally. The crypto IC is accessed
through the FPGA register interface (for FX3 devices), so the FPGA must be
operational first.

### Reconstructed Authentication Protocol

Based on string analysis, the authentication protocol is:

```
1. Wake crypto IC
   - Send I2C wake token (zero-byte write to address 0x00)
   - "Failed to send i2c wake token" on error

2. Read crypto IC serial number
   - Read from config zone
   - "Error Reading Cyrpto serial number" on failure (note: typo in binary)

3. Challenge-response
   a. Host generates random nonce via CryptoRandomBytes()
   b. Host sends nonce to crypto IC (Nonce command)
   c. Crypto IC processes nonce internally
   d. Host requests signature of digest from a specific slot (SignDigest)
   e. Crypto IC signs using its private key in the specified slot
   f. Host verifies signature using known public key (VerifyDigest)

4. Result evaluation
   - "nonce not returned" -- crypto IC did not respond to nonce command
   - "nonce mismatch" -- returned nonce does not match sent nonce
   - "CHALLENGE_IS_DIFFERENT" / "challenge is different" -- challenge failed
   - "invalid challenge length" -- wrong-sized challenge data
   - "bad ecdsa signature" / "BAD_ECDSA_SIGNATURE" -- signature verification failed
   - "bad authentication type" / "BAD_AUTHENTICATION_TYPE" -- unexpected auth type
```

### Authentication Type Enum

The strings `auth-ecdsa` / `AuthECDSA` suggest an authentication type enum:

```cpp
enum AuthenticationType {
    AuthECDSA,      // ECDSA challenge-response (primary)
    // possibly others for older hardware
};
```

### VerifyDevice

The string `VerifyDevice` appears as a standalone function name or command.
This is likely the top-level function that orchestrates the full
authentication flow. It is called from `CommonDeviceInit` or immediately
after it.

### LogicProfessionalDevice::GetPublicKey

```cpp
virtual std::vector<U8> LogicProfessionalDevice::GetPublicKey()
```

Logic Pro devices have a device-level `GetPublicKey` method that retrieves
the public key from the crypto IC. This key is likely used by the host to
verify the device's identity (the host has a known Saleae root public key
and verifies the device certificate/signature chain).

### BasicCryptoCommands.cpp

The source file `BasicCryptoCommands.cpp` contains the crypto command
implementations. This file is in the `logic_interface` source tree:
```
/workspace/monorepo/graph-io/devices/logic_interface/src/BasicCryptoCommands.cpp
```

This suggests the crypto commands are device-specific (part of the logic
device interface), not in the generic crypto device library.


## SimpleEncryption

A separate `SimpleEncryption` module exists:
```
/workspace/monorepo/graph-io/devices/usb_device/src/SimpleEncryption.cpp
```

This is part of the USB device layer, not the crypto IC interface. It likely
implements a simple symmetric encryption scheme (possibly XOR-based or AES)
for obscuring data in transit, NOT for authentication. The name "Simple"
suggests it is not a strong cryptographic mechanism and may be used for
basic obfuscation of EEPROM data, calibration data, or device IDs.


## What Happens on Authentication Failure

Based on the device state machine analysis:

1. If authentication fails during `DSInitializing`, the device transitions
   to `DSError`
2. `DSError` is a **terminal state** -- recovery requires physical
   disconnect/reconnect
3. The error messages broadcast to the UI:
   - "Device is not responsive, please reconnect the device. error: "
   - "USB Device Init Error: {}"
4. No partial functionality -- the device is completely non-operational
   in `DSError`

However, this is enforced by the **host software** (libgraph_server_shared.so),
not by the firmware. The FX3 firmware does not independently check
authentication.


## Assessment for Open Driver

### Can we operate without crypto authentication?

**YES.** The crypto authentication is purely a host-software check. The
analysis strongly supports this conclusion for these reasons:

1. **Timing**: Authentication happens AFTER the FPGA is fully programmed
   and operational. If firmware enforcement existed, it would block FPGA
   programming.

2. **No firmware-side auth strings**: The FX3 firmware (embedded in the
   .so binary) contains I2C initialization code (`InitI2C failed, %d`) but
   NO authentication or crypto-related strings. The firmware is a transparent
   I2C/SPI tunnel.

3. **The FX3 is just a tunnel**: Opcodes 0x87 (WriteI2C) and 0x88 (ReadI2c)
   are generic I2C tunnels through the FX3. The firmware forwards I2C
   traffic without understanding or enforcing authentication semantics.

4. **The FPGA bitstream is not encrypted**: All Lattice bitstreams have
   `Security: Off` in their headers. No bitstream encryption or
   authentication is applied at the FPGA level.

5. **No gating in capture path**: The EP1 command protocol has START_READ
   (0x01) and STOP_STREAM (0x02) commands that directly control data flow.
   There is no "authentication required" gate in the command protocol.

6. **JS bundle has no auth handling**: The Electron frontend has no code
   for authentication results, suggesting auth is purely internal to the
   native library and never communicated to the UI as a separate step.

### What our Rust driver needs to do

1. **Skip crypto authentication entirely** -- simply do not communicate
   with the ATECC IC at all
2. **Proceed directly after FPGA programming** -- after CommonDeviceInit
   completes (ADC init, PLL lock, etc.), the device is ready for capture
3. **No I2C communication with crypto IC needed** -- the crypto IC sits
   passively on the I2C bus; not communicating with it has no side effects

### What our Rust driver does NOT need

- No ATECC508A/608A driver implementation
- No ECDSA key handling
- No challenge-response protocol
- No OpenSSL dependency for device communication
- No `CryptoInterface` equivalent

### Potential risks

1. **Future firmware versions**: Saleae could release firmware that gates
   operations on authentication. Our driver uses the firmware embedded in
   v2.4.40 (uploaded to FX3 RAM, not persistent), so this would only matter
   if we updated to newer embedded firmware.

2. **Newer hardware revisions**: Future hardware could implement firmware-level
   authentication. Currently, evidence shows no such enforcement.

3. **EEPROM access conflicts**: The crypto IC and EEPROM share the I2C bus.
   Our driver reads the EEPROM for device ID and calibration keys. We need
   to ensure we do not accidentally wake the crypto IC by sending traffic to
   its I2C address (typically 0xC0 for ATECC508A). Using the EEPROM address
   (typically 0xA0-0xA7 for AT24CXX) should be safe.


## ATECC508A/608A Command Set (Reference)

The following ATECC commands are used by Saleae, mapped from string/API evidence:

| ATECC Command | Opcode | Saleae Method | Evidence |
|---------------|--------|---------------|----------|
| Read | 0x02 | ReadConfig32Bytes, ReadData32Bytes | zone-based read operations |
| Write | 0x12 | WriteConfigFourBytes, WriteData32Bytes, WriteOTP32Bytes | zone-based write operations |
| Nonce | 0x16 | (internal to challenge flow) | "nonce not returned", "nonce mismatch" |
| GenKey | 0x40 | GetPublicKey, NewPrivateKey | key generation and retrieval |
| Sign | 0x41 | SignDigest | "ECDSA signature" related strings |
| Verify | 0x45 | ECDSASignature::VerifyDigest (host-side via OpenSSL) | Verification done in software, not on IC |
| Info | 0x30 | (serial number read) | "Error Reading Cyrpto serial number" |

Note: The ECDSA **verification** is done host-side using OpenSSL, not on the
ATECC IC itself. The IC performs the **signing** operation using its stored
private key. The host verifies the signature using a known public key.


## Source File Map

| Source File | Purpose |
|-------------|---------|
| `devices/crypto_device/src/crypto_device/crypto_interface.cpp` | CryptoInterface base class (zone read/write, key ops) |
| `devices/crypto_device/src/crypto_device/crypto_interface_i2c.cpp` | CryptoInterfaceI2c (I2C transport) |
| `devices/crypto_device/src/crypto_device/cryptography.cpp` | ECKey, ECDSASignature, CryptoBigNum (OpenSSL wrappers) |
| `devices/logic_interface/src/crypto_interface_devices.cpp` | CryptoInterfaceFpga, CryptoInterfacePro (FPGA transport) |
| `devices/logic_interface/src/BasicCryptoCommands.cpp` | High-level crypto commands (VerifyDevice, etc.) |
| `devices/usb_device/src/SimpleEncryption.cpp` | Simple symmetric encryption (not auth-related) |


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| Crypto IC is ATECC508A/608A family | High | 32-byte blocks, 16 slots, zone model, I2C wake, ECDSA P-256 |
| Three CryptoInterface variants (I2c/Fpga/Pro) | Confirmed | RTTI type names, `__shared_ptr_emplace` entries |
| Authentication uses ECDSA challenge-response | Confirmed | `SignDigest`, `VerifyDigest`, nonce/challenge strings |
| Auth happens during init, after FPGA programming | Confirmed | State machine sequence in device-state-machine.md |
| Auth failure causes DSError (terminal state) | Confirmed | State transition rules from assertions |
| FX3 firmware does NOT enforce authentication | High | No auth strings in firmware, firmware is I2C tunnel only |
| Open driver can skip authentication entirely | High | All evidence points to host-software-only enforcement |
| EC curve is P-256 (secp256r1/prime256v1) | High | P-256 and prime256v1 strings present |
| SimpleEncryption is separate from auth | High | Different source path, in usb_device not crypto_device |
| Host verifies signature using OpenSSL (not IC) | High | OpenSSL ECDSA functions linked, VerifyDigest signature |


## Open Questions

1. **What slot number holds the device's private key?** The ATECC IC has
   16 slots. The device's signing key is in one specific slot. Ghidra
   decompilation of `BasicCryptoCommands.cpp` would reveal this.

2. **Is there a certificate chain?** Does the device store a certificate
   in the data zone that chains to a Saleae root CA? Or is the public key
   hardcoded in the host software?

3. **What is the SimpleEncryption used for?** Is it used to encrypt
   calibration data, device IDs, or communication with the EEPROM?

4. **What is the "Crypto handle" string?** It appears to be an opaque
   handle to the crypto IC session, but its exact use needs Ghidra analysis.

5. **Do all hardware revisions have a crypto IC?** The `UsesNewCryptoInterface`
   method suggests different hardware revisions use different access paths,
   but do any revisions lack the crypto IC entirely?


## References

- `re/notes/i2c-eeprom-protocol.md` -- I2C tunnel and EEPROM operations
- `re/notes/device-class-hierarchy.md` -- device class hierarchy, CryptoInterface per device
- `re/notes/fpga-bitstream-protocol.md` -- FPGA programming sequence, auth step
- `re/notes/device-state-machine.md` -- initialization sequence and state transitions
- `re/notes/ep1-command-protocol.md` -- EP1 command opcodes for I2C tunnel
- `doc/protocol/ep1-commands.md` -- confirmed EP1 command protocol
- ATECC508A datasheet (Microchip DS20005927)
- ATECC608A datasheet (Microchip DS40002236)
