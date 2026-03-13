# Decompiled Functions: FUN_01d22af0 (ResetMainDcm) and FUN_01d21c90 (CommonDeviceInit)

Investigation date: 2026-03-13
Task: TASK-0169
Method: Ghidra decompilation analysis


## FUN_01d22af0: ResetMainDcm

**Ghidra VA**: 0x01d22af0 | **ELF VA**: 0x1c22af0 | **Size**: ~730 bytes
**Source**: `Logic2FpgaDevice.cpp`, method `ResetMainDcm`

### Purpose

Resets the FPGA's main DCM (Digital Clock Manager) / PLL and verifies
that PLLs lock successfully. Handles both Xilinx Spartan-6 and Lattice
ECP5 FPGA variants.

### Control Flow

The function branches on `*(int*)(param_1 + 0x21c)`, which is the FPGA
type / device type field:

#### Case 0: Xilinx Spartan-6

Performs the staged power-down sequence via FPGA register 0x00:
```
r0x00 = 0x1e -> 0x1c -> 0x18 -> 0x08 -> 0x00
```

This clears bits in sequence to bring up the DCM in a controlled order.
No PLL lock check is performed for Spartan-6 (r0x40 returns 0x00 on this
FPGA because the JESD/PLL status register does not exist -- the Spartan-6
uses a different clocking architecture than the ECP5).

#### Case 1: Lattice ECP5

1. Writes r0x00 = 0x04 (assert PLL reset)
2. Sleeps 1ms
3. Writes r0x00 = 0x00 (release PLL reset)
4. **Polls r0x40 for PLL lock**:
   - Reads r0x40 in a loop with 1ms delays
   - Checks `(value & 0xA0) == 0xA0` (both system PLL and PCLK PLL locked)
   - Times out after ~20 seconds (0x1312d00 nanoseconds)
   - On timeout: throws exception with message:
     `"PLL(s) failed to lock after PLL reset! system PLL: {}, PCLK PLL: {}. status register: {:#x}"`
5. After PLL lock, performs SERDES reset loop:
   - Writes r0x08 = 1 (SERDES reset assert)
   - Sleeps 1ms
   - Writes r0x00 = 0x00 (clear)
   - Sleeps 1ms
   - Reads r0x19 (SERDES status)
   - Sleeps 1ms
   - Reads r0x4a (bitslip status):
     - Checks bit 1 (ADC0 bitslip error)
     - Checks bit 2 AND `device_type == 7` (ADC1 bitslip error, only for
       16-channel devices)
   - If bitslip errors detected, retries up to 8 times
   - On failure: throws "Bitslip lock failed." exception
6. Logs: `"Lattice PLL and SERDES reset complete."`

### Key Observations

- The r0x00 power sequence we see in captures (`0x1e -> 0x1c -> 0x18 ->
  0x08 -> 0x00`) is the Xilinx Spartan-6 path (case 0)
- This matches our Logic Pro 8 hardware (Rev 1.2, Spartan-6)
- The Lattice ECP5 path is more complex with PLL lock polling and SERDES
  bitslip recovery
- The check `device_type == 7` for ADC1 bitslip suggests device type 7 is
  a 16-channel variant (Logic Pro 16 with Lattice ECP5)
- Our Rust driver already implements the Spartan-6 path correctly via
  `reset_main_dcm()` in `init.rs`


## FUN_01d21c90: CommonDeviceInit (Pre-Capture)

**Ghidra VA**: 0x01d21c90 | **ELF VA**: 0x1c21c90 | **Size**: 638 bytes
**Source**: `Logic2FpgaDevice.cpp`, labeled `PreCapture_CommonDeviceInit`

### Purpose

Orchestrates device initialization before capture. Called from the capture
start path when the device has not yet been initialized for the current
session.

### Control Flow

1. **Checks if device is already initialized** via a shared_ptr callback
   mechanism (FUN_02ed1390 with FUN_01d2a770 as callback). If a valid
   shared_ptr exists at `param_1 + 0x41` (offset 0x208 in the device
   struct), checks initialization state.

2. **If already initialized**: calls FUN_01d296c0 (labeled
   "CommonDeviceInit helper") and checks a flag at offset 0xd0 in the
   result. If the flag is set, skips to cleanup (already initialized).

3. **Branches on `param_1[0x44]`** (device type / crypto interface
   selector, same field as iVar4):

   #### Case iVar4 == 0: New Crypto Interface Path
   - Calls vtable[0x68] on the device (`*param_1 + 0x68`), which is
     `GetPublicKey()` (returns a vector of bytes)
   - Copies the public key data
   - Passes to FUN_01d2cbf0 for verification
   - This is the **CryptoInterfacePro** / new-style authentication path

   #### Case iVar4 == 1: Old Crypto Interface Path
   - Calls vtable[0x80] on the device, which returns a
     `shared_ptr<CryptoInterface>`
   - Passes to FUN_02ea2b80 for authentication verification
   - On failure: throws "device" error at source line 0x239

   #### Case iVar4 >= 3: Assertion failure
   - Calls FUN_02eca300 with assertion:
     `"false"` / `"bool RevisionDetail::UsesNewCryptoInterface()"`
   - Source: `Logic2FpgaDevice.cpp` line 0x9e
   - This means `iVar4` must be 0 or 1; other values are not valid

### Key Observations

- `param_1[0x44]` (the field at offset 0x220) determines which crypto
  interface to use. The check `1 < iVar4 - 1U` means values 0 and 1 are
  valid, 2+ trigger assertion failure.
- **iVar4 == 0**: Uses `GetPublicKey()` via vtable -- this is the
  "new" crypto interface (CryptoInterfacePro for Logic Pro devices)
- **iVar4 == 1**: Uses a shared_ptr<CryptoInterface> via vtable[0x80]
  -- this is the "old" crypto interface (CryptoInterfaceFpga or
  CryptoInterfaceI2c)
- This function is the **crypto authentication step** in the init
  sequence (step 10 from crypto-authentication.md)
- Our Rust driver correctly skips this entire function -- we go straight
  from ADC/PLL init to capture configuration

### Callers

From decompiled code:
- `decompiled-graduate.txt` (FUN_01d21c90 called)
- `decompiled-compression.txt` (FUN_01d21c90 called)
- `decompiled-capture-start.txt` (FUN_01d21c90 called at two sites)
- `re/notes/logic-graduate-device.md`: "Calls FUN_01d21c90
  (CommonDeviceInit): initializes FPGA, ADC, crypto"


## FUN_01d20d90: WriteAd9637Register (ADC Register Write Helper)

**Ghidra VA**: 0x01d20d90 | **ELF VA**: 0x1c20d90 | **Size**: ~280 bytes
**Source**: `Logic2FpgaDevice.cpp`, method `WriteAd9637Register`

Already decompiled in `re/ghidra/decompiled-fun-01d20d90.txt`. This is
the ADC register write function that uses the FPGA proxy pattern:
```
r0x03 = address,  r0x16 = 0,  r0x04 = value,  r0x05 = 0
```

Called by the ADC initialization sequence. Validates that data fits in
8 bits (assertion: "invalid ADC data, exceedes 8 bits.").

The function writes to these ADC (AD9637) registers in sequence:
- 0x08 = 0x03 (soft reset), then 0x00 (clear reset)
- Sleep 10ms
- 0x0D = param_3 (output mode)
- 0x15 = param_2 (mixed: low byte is power control, upper nibble is
  from bit 28+ of param_2)
- 0x21 = 0xC2 (test pattern / data format)
- 0x05 = 0x0F (clock divider?)
- 0x04 = 0x0F (clock phase?)
- 0x10 = 0x00 (offset binary)
- 0x18 = 0x04 (VREF internal)


## Impact on Rust Driver

All three functions are already effectively implemented in our driver:

1. **ResetMainDcm** (FUN_01d22af0): Implemented as `reset_main_dcm()` in
   `init.rs` -- the Spartan-6 path (r0x00 staged write) is correct for
   our hardware.

2. **CommonDeviceInit** (FUN_01d21c90): This is the crypto auth step. We
   correctly skip it -- the driver works without authentication.

3. **WriteAd9637Register** (FUN_01d20d90): Implemented as the ADC init
   register write sequence in `init.rs`.

No code changes needed.


## Confidence Assessment

| Finding | Confidence | Evidence |
|---------|-----------|----------|
| FUN_01d22af0 is ResetMainDcm | Confirmed | Source string "ResetMainDcm" in function body |
| Two FPGA paths (Xilinx=0, Lattice=1) | Confirmed | Switch on device type, PLL lock message for Lattice |
| FUN_01d21c90 is crypto auth orchestrator | High | Calls vtable GetPublicKey, UsesNewCryptoInterface assertion |
| iVar4 field selects crypto interface type | High | Branch on 0/1, assertion on >=2, matches RevisionDetail |
| FUN_01d20d90 is WriteAd9637Register | Confirmed | Source string in assertion message |
| No Rust driver changes needed | Confirmed | All paths already implemented or intentionally skipped |


## References

- `re/ghidra/decompiled-fun-01d22af0.txt` -- Full ResetMainDcm decompilation
- `re/ghidra/decompiled-graduate2.txt` -- Full CommonDeviceInit decompilation
- `re/ghidra/decompiled-fun-01d20d90.txt` -- Full WriteAd9637Register decompilation
- `re/notes/crypto-authentication.md` -- Crypto auth analysis
- `re/notes/init-sequence-pcap.md` -- Observed JESD power sequence
- `re/notes/logic-graduate-device.md` -- Caller analysis
