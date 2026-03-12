//! EP1 command opcode constants for the Saleae USB protocol.
//!
//! All device control after firmware upload flows through EP1 bulk endpoints
//! using a command/response protocol. Each command starts with a 1-byte opcode.
//! On Pro devices, the entire frame is XOR-encrypted (see [`super::obfuscation`]).
//!
//! These constants replace magic hex literals scattered across the driver crate.
//! The canonical reference is `doc/protocol/ep1-commands.md`.

/// Start sampling on EP2 (triggers bulk-in data stream).
///
/// No payload. Response: `[0x07, 0x00]`.
pub const START_READ: u8 = 0x01;

/// Stop the EP2 data stream.
///
/// No payload. Response: `[0x07, 0x00]`.
pub const STOP_STREAM: u8 = 0x02;

/// Read version / EEPROM via the I2C tunnel.
///
/// Payload: `[0x33, 0x81, addr_lo, addr_hi, count_lo, count_hi]` for EEPROM reads.
/// Also used for EEPROM writes (two-step: header then data).
/// Response: requested byte count.
pub const READ_VERSION: u8 = 0x07;

/// Write EEPROM data.
///
/// Used via the ReadVersion I2C tunnel (`0x07`) in the current implementation.
/// This opcode is reserved in the firmware but not directly sent by the driver.
pub const WRITE_EEPROM: u8 = 0x55;

/// Ping command for seed synchronization verification.
///
/// Payload: `[0x55]` (test byte). Response: `[0xAA]` (expected).
/// Used after ReseedDevice to confirm both sides share the same LFSR state.
///
/// Note: The ping constants (`PING_OPCODE`, `PING_TEST_BYTE`,
/// `PING_EXPECTED_RESPONSE`) also live in [`super::obfuscation`] where they
/// are used by the encryption layer directly.
pub const PING: u8 = 0x7D;

/// Reset the FPGA to accept a new bitstream.
///
/// No payload. Response: 1-byte ACK (`0x00` = success).
pub const RESET_FPGA: u8 = 0x7E;

/// Write FPGA configuration (bitstream) data.
///
/// Payload format depends on device type:
/// - Pro (USB 3.0): `[len_lo, len_hi, data...]` (max 1020 bytes per chunk)
/// - Pro (USB 2.0): `[len_lo, len_hi, data...]` (max 508 bytes per chunk)
/// - Non-Pro: `[len_8bit, data...]` (max 62 bytes per chunk)
///
/// Pro devices return a 1-byte ACK (`0x00`) per chunk.
pub const WRITE_CONFIG_DATA: u8 = 0x7F;

/// Write FPGA registers.
///
/// Payload: `[count, addr0, val0, addr1, val1, ...]` (max 31 pairs).
/// Pro devices return a 1-byte ACK (`0x00`).
pub const WRITE_REGISTERS: u8 = 0x80;

/// Read FPGA registers.
///
/// Payload: `[count, addr0, addr1, ...]` (max 62 addresses).
/// Response: `[val0, val1, ...]` matching the requested addresses.
pub const READ_REGISTERS: u8 = 0x81;

/// Stop the data path (teardown after capture).
///
/// No payload. Some devices may not send a response.
pub const STOP_DATA_PATH: u8 = 0x86;

/// Write to an I2C device via the FX3 proxy.
///
/// Payload: `[i2c_addr, reg_addr, data...]`.
/// Response: 1-byte ACK.
pub const WRITE_I2C: u8 = 0x87;

/// Read from an I2C device via the FX3 proxy.
///
/// Payload: `[i2c_addr, reg_addr, count]`.
/// Response: requested byte count.
/// Also used (with no payload) to read the EEPROM device ID.
pub const READ_I2C: u8 = 0x88;

/// Device2_1 command (purpose under investigation).
///
/// No payload. Response: single byte (`0x00` observed).
/// See task-0169 for ongoing RE of this command.
pub const DEVICE2_1: u8 = 0x89;

/// Get device info string.
///
/// No payload. Response: null-terminated ASCII string
/// (e.g., `"1, Logic Pro 8"`).
pub const GET_DEVICE_INFO: u8 = 0x8B;
