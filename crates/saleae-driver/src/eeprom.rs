//! EEPROM I2C calibration read/write operations.
//!
//! Saleae FX3-based devices have an I2C EEPROM on the board accessed via
//! the EP1 I2C tunnel (opcodes 0x87 WriteI2C, 0x88 ReadI2C). The EEPROM
//! stores device identity and is also used to read calibration parameters
//! from a secondary I2C device at address 0xC0 during initialization.
//!
//! # Protocol (from pcap Phase 6)
//!
//! The I2C device at address 0xC0 uses a command-response protocol:
//!
//! 1. **Device ID read**: `ReadI2C(0xC0, 0x04, 0)` returns 5 bytes
//! 2. **Calibration block read** (with polling):
//!    - `WriteI2C(0xC0, 0x08, [data])` -- issue read command
//!    - `ReadI2C(0xC0, reg, 0)` -- poll until status byte != 0x01
//!    - Response: `[status, data...]` where status 0x02 = data ready
//!    - `WriteI2C(0xC0, 0x01, [])` -- acknowledge/reset
//! 3. **Calibration parameter write**: `WriteI2C(0xC0, 0x1C, [data])`
//!
//! # I2C address note
//!
//! The 0xC0 address is the 8-bit I2C address (7-bit address 0x60 with R/W
//! bit cleared). This is consistent with common I2C EEPROM addressing.
//!
//! # References
//!
//! - `re/notes/i2c-eeprom-protocol.md` -- I2C tunnel protocol analysis
//! - `re/notes/eeprom-layout.md` -- EEPROM memory layout
//! - `re/notes/init-sequence-pcap.md` -- Phase 6 shows exact I2C operations

use tracing::{debug, info, warn};

use crate::ep1::Ep1CommandChannel;
use crate::error::{DriverError, DriverResult};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// I2C address for the calibration EEPROM (8-bit format, 7-bit = 0x60).
///
/// From pcap Phase 6: all EEPROM operations use address 0xC0.
pub const EEPROM_I2C_ADDR: u8 = 0xC0;

/// EEPROM register: device identification.
///
/// `ReadI2C(0xC0, 0x04, 0)` returns 5 bytes of device ID.
pub const EEPROM_REG_DEVICE_ID: u8 = 0x04;

/// EEPROM register: issue a read command to the EEPROM controller.
///
/// `WriteI2C(0xC0, 0x08, [data])` sets up a calibration block read.
pub const EEPROM_REG_READ_CMD: u8 = 0x08;

/// EEPROM register: acknowledge/status reset.
///
/// `WriteI2C(0xC0, 0x01, [])` acknowledges completion of a read.
pub const EEPROM_REG_ACK: u8 = 0x01;

/// EEPROM register: write calibration parameters.
///
/// `WriteI2C(0xC0, 0x1C, [data])` writes calibration parameters.
pub const EEPROM_REG_WRITE_CAL: u8 = 0x1C;

/// EEPROM register: read 35-byte calibration block response.
///
/// `ReadI2C(0xC0, 0x23, 0)` -- polls for response data. The first byte
/// is the status: 0x01 = not ready, 0x02 = data follows.
pub const EEPROM_REG_READ_BLOCK_35: u8 = 0x23;

/// EEPROM register: read 67-byte calibration block response.
///
/// `ReadI2C(0xC0, 0x43, 0)` -- polls for response data.
pub const EEPROM_REG_READ_BLOCK_67: u8 = 0x43;

/// EEPROM read status: not ready, try again.
pub const EEPROM_STATUS_NOT_READY: u8 = 0x01;

/// EEPROM read status: data is ready (followed by data bytes).
pub const EEPROM_STATUS_READY: u8 = 0x02;

/// Maximum number of poll attempts before giving up on a read.
///
/// From pcap: reads typically complete within 2-6 polls. 20 is generous.
pub const MAX_POLL_ATTEMPTS: usize = 20;

// ---------------------------------------------------------------------------
// EepromCalibration: parsed calibration data
// ---------------------------------------------------------------------------

/// Calibration data read from the I2C EEPROM at address 0xC0.
///
/// This holds the raw calibration blocks as read from the device. The
/// exact interpretation of each block is device-specific and will be
/// refined as more pcap data is analyzed.
///
/// From pcap Phase 6:
/// - `device_id`: 5 bytes from register 0x04
/// - `block_35`: 34 bytes of calibration data from the 35-byte block
///   (first byte is status, remaining 34 are data)
/// - `block_67`: 66 bytes of calibration data from the 67-byte block
///   (first byte is status, remaining 66 are data)
/// - `cal_params_written`: the 26 bytes written via register 0x1C
#[derive(Debug, Clone)]
pub struct EepromCalibration {
    /// 5-byte device identification from register 0x04.
    ///
    /// Observed values: `[0x02, 0x04, 0x11, 0x33, 0x43]`
    pub device_id: Vec<u8>,

    /// Calibration data from the 35-byte block (offset 0x23).
    ///
    /// This is the data portion only (status byte stripped). Typically
    /// 34 bytes. Likely contains ADC calibration parameters.
    pub block_35: Vec<u8>,

    /// Calibration data from the 67-byte block (offset 0x43).
    ///
    /// This is the data portion only (status byte stripped). Typically
    /// 66 bytes. Likely contains analog front-end calibration parameters.
    pub block_67: Vec<u8>,

    /// Calibration parameters that were written back to the device via
    /// register 0x1C. 26 bytes observed in pcap.
    pub cal_params_written: Vec<u8>,
}

// ---------------------------------------------------------------------------
// EEPROM operations
// ---------------------------------------------------------------------------

/// Write to an EEPROM register via the EP1 I2C tunnel.
///
/// Sends `WriteI2C(i2c_addr, register, data)` and returns the response.
///
/// # Arguments
///
/// - `channel`: EP1 command channel (must be initialized).
/// - `i2c_addr`: 8-bit I2C device address.
/// - `register`: Register/command byte.
/// - `data`: Additional data bytes to write (may be empty).
pub fn eeprom_write_register(
    channel: &mut Ep1CommandChannel,
    i2c_addr: u8,
    register: u8,
    data: &[u8],
) -> DriverResult<Vec<u8>> {
    debug!(
        i2c_addr = format_args!("{i2c_addr:#04x}"),
        register = format_args!("{register:#04x}"),
        data_len = data.len(),
        "EEPROM write register"
    );

    channel.write_i2c(i2c_addr, register, data)
}

/// Read from an EEPROM register via the EP1 I2C tunnel.
///
/// Sends `ReadI2C(i2c_addr, register, count)` and returns the raw response.
/// This is a single-shot read without polling. For polled reads, use
/// [`eeprom_read_register_poll`].
///
/// # Arguments
///
/// - `channel`: EP1 command channel.
/// - `i2c_addr`: 8-bit I2C device address.
/// - `register`: Register/command byte.
/// - `count`: Number of bytes to read (0 = read all available).
pub fn eeprom_read_register(
    channel: &mut Ep1CommandChannel,
    i2c_addr: u8,
    register: u8,
    count: u8,
) -> DriverResult<Vec<u8>> {
    debug!(
        i2c_addr = format_args!("{i2c_addr:#04x}"),
        register = format_args!("{register:#04x}"),
        count,
        "EEPROM read register"
    );

    channel.read_i2c(i2c_addr, register, count)
}

/// Read from an EEPROM register with polling until data is ready.
///
/// The I2C EEPROM controller at 0xC0 uses a polled read protocol:
/// - Send `ReadI2C(addr, register, 0)`
/// - Response byte 0 = status: 0x01 = not ready, 0x02 = data ready
/// - If not ready, retry up to [`MAX_POLL_ATTEMPTS`] times
/// - When ready, return the data bytes (status byte stripped)
///
/// # Returns
///
/// The data bytes from the response (status byte removed), or an error
/// if the maximum number of poll attempts is exceeded.
pub fn eeprom_read_register_poll(
    channel: &mut Ep1CommandChannel,
    i2c_addr: u8,
    register: u8,
) -> DriverResult<Vec<u8>> {
    debug!(
        i2c_addr = format_args!("{i2c_addr:#04x}"),
        register = format_args!("{register:#04x}"),
        "EEPROM polled read"
    );

    for attempt in 0..MAX_POLL_ATTEMPTS {
        let response = channel.read_i2c(i2c_addr, register, 0)?;

        if response.is_empty() {
            warn!(attempt, "EEPROM polled read: empty response");
            continue;
        }

        let status = response[0];
        debug!(attempt, status = format_args!("{status:#04x}"), response_len = response.len(), "EEPROM poll");

        match status {
            EEPROM_STATUS_NOT_READY => {
                // Not ready yet, try again.
                debug!(attempt, "EEPROM not ready, retrying");
                continue;
            }
            EEPROM_STATUS_READY => {
                // Data is ready. Return everything after the status byte.
                let data = response[1..].to_vec();
                debug!(
                    attempt,
                    data_len = data.len(),
                    "EEPROM polled read complete"
                );
                return Ok(data);
            }
            other => {
                // Unexpected status byte. Could be a different protocol
                // version or an error. Log it and return the full response
                // for the caller to interpret.
                warn!(
                    status = format_args!("{other:#04x}"),
                    attempt,
                    "EEPROM polled read: unexpected status byte, returning full response"
                );
                return Ok(response);
            }
        }
    }

    Err(DriverError::CommandFailed {
        command: "EepromPolledRead",
        detail: format!(
            "EEPROM at {i2c_addr:#04x} register {register:#04x} did not become ready \
             after {MAX_POLL_ATTEMPTS} attempts"
        ),
    })
}

/// Read the 5-byte device ID from the EEPROM.
///
/// Protocol: `ReadI2C(0xC0, 0x04, 0)` returns 5 bytes.
///
/// Observed values from pcap: `[0x02, 0x04, 0x11, 0x33, 0x43]`
pub fn eeprom_read_device_id(channel: &mut Ep1CommandChannel) -> DriverResult<Vec<u8>> {
    info!("Reading EEPROM device ID from I2C addr {:#04x}", EEPROM_I2C_ADDR);

    let data = eeprom_read_register(channel, EEPROM_I2C_ADDR, EEPROM_REG_DEVICE_ID, 0)?;

    if data.is_empty() {
        return Err(DriverError::CommandFailed {
            command: "EepromReadDeviceId",
            detail: "empty response from EEPROM device ID read".into(),
        });
    }

    info!(
        device_id = format_args!("{:02x?}", &data),
        "EEPROM device ID"
    );
    Ok(data)
}

/// Read a calibration data block from the EEPROM with polling.
///
/// Protocol:
/// 1. `WriteI2C(0xC0, 0x08, [block_cmd_data])` -- issue read command
/// 2. `ReadI2C(0xC0, response_reg, 0)` -- poll until data ready
/// 3. `WriteI2C(0xC0, 0x01, [])` -- acknowledge
///
/// # Arguments
///
/// - `channel`: EP1 command channel.
/// - `block_cmd_data`: Command data to write with register 0x08 to set up
///   the read. The exact bytes depend on which calibration block is being
///   read (this is device-specific and derived from pcap).
/// - `response_reg`: Register to poll for the response (0x23 for 35-byte
///   block, 0x43 for 67-byte block).
///
/// # Returns
///
/// The calibration data bytes (status byte stripped from the polled read).
pub fn eeprom_read_calibration_block(
    channel: &mut Ep1CommandChannel,
    block_cmd_data: &[u8],
    response_reg: u8,
) -> DriverResult<Vec<u8>> {
    debug!(
        cmd_data = format_args!("{:02x?}", block_cmd_data),
        response_reg = format_args!("{response_reg:#04x}"),
        "Reading EEPROM calibration block"
    );

    // Step 1: Issue the read command.
    eeprom_write_register(channel, EEPROM_I2C_ADDR, EEPROM_REG_READ_CMD, block_cmd_data)?;

    // Step 2: Poll for the response data.
    let data = eeprom_read_register_poll(channel, EEPROM_I2C_ADDR, response_reg)?;

    // Step 3: Acknowledge/reset.
    eeprom_write_register(channel, EEPROM_I2C_ADDR, EEPROM_REG_ACK, &[])?;

    info!(
        response_reg = format_args!("{response_reg:#04x}"),
        data_len = data.len(),
        "EEPROM calibration block read complete"
    );

    Ok(data)
}

/// Write calibration parameters to the EEPROM.
///
/// Protocol: `WriteI2C(0xC0, 0x1C, [data])` -- writes calibration
/// parameters to the device.
///
/// From pcap: 26 bytes are written via this register.
pub fn eeprom_write_calibration_params(
    channel: &mut Ep1CommandChannel,
    data: &[u8],
) -> DriverResult<Vec<u8>> {
    info!(
        data_len = data.len(),
        "Writing EEPROM calibration parameters"
    );

    eeprom_write_register(channel, EEPROM_I2C_ADDR, EEPROM_REG_WRITE_CAL, data)
}

/// Orchestrate reading the full EEPROM calibration sequence.
///
/// This replays the exact pcap Phase 6 sequence (frames 1651-1752):
///
/// 1. Read EEPROM device ID (register 0x04)
/// 2. Read 35-byte calibration block (write cmd to 0x08, poll 0x23)
/// 3. Read EEPROM device ID again
/// 4. Write 26-byte calibration parameters (register 0x1C)
/// 5. Read response from 35-byte register (poll 0x23)
/// 6. Acknowledge
/// 7. Read EEPROM device ID again
/// 8. Read 67-byte calibration block (write cmd to 0x08, poll 0x43)
///
/// # Arguments
///
/// - `channel`: EP1 command channel (must be initialized and in the
///   correct phase -- Device2_1 should have been sent already).
/// - `cal_params`: The 26-byte calibration parameters to write in step 4.
///   These are typically derived from the server-hosted calibration data
///   or from a previous device configuration.
///
/// # Returns
///
/// An [`EepromCalibration`] struct containing all read data.
pub fn eeprom_read_full_calibration(
    channel: &mut Ep1CommandChannel,
    cal_params: &[u8],
) -> DriverResult<EepromCalibration> {
    info!("Starting full EEPROM calibration read sequence");

    // Step 1: Read device ID.
    let device_id = eeprom_read_device_id(channel)?;

    // Step 2: Read 35-byte calibration block.
    // The pcap shows WriteI2C(0xC0, 0x08, []) before polling 0x23.
    // The command data for the first block read appears to be empty or
    // minimal -- the register 0x08 itself is the command to initiate
    // the read at the default/first calibration block offset.
    let block_35 = eeprom_read_calibration_block(channel, &[], EEPROM_REG_READ_BLOCK_35)?;

    // Step 3: Read device ID again (integrity check).
    let device_id_2 = eeprom_read_device_id(channel)?;
    if device_id != device_id_2 {
        warn!(
            first = format_args!("{:02x?}", &device_id),
            second = format_args!("{:02x?}", &device_id_2),
            "EEPROM device ID mismatch between reads"
        );
    }

    // Step 4: Write calibration parameters.
    let _write_resp = eeprom_write_calibration_params(channel, cal_params)?;

    // Step 5: Read response from 35-byte register (may contain updated data).
    let _block_35_updated = eeprom_read_register_poll(
        channel,
        EEPROM_I2C_ADDR,
        EEPROM_REG_READ_BLOCK_35,
    )?;

    // Step 6: Acknowledge.
    eeprom_write_register(channel, EEPROM_I2C_ADDR, EEPROM_REG_ACK, &[])?;

    // Step 7: Read device ID again.
    let device_id_3 = eeprom_read_device_id(channel)?;
    if device_id != device_id_3 {
        warn!(
            first = format_args!("{:02x?}", &device_id),
            third = format_args!("{:02x?}", &device_id_3),
            "EEPROM device ID mismatch on third read"
        );
    }

    // Step 8: Read 67-byte calibration block.
    // The pcap shows a WriteI2C(0xC0, 0x08, [data]) before polling 0x43.
    // The exact command data depends on the block being requested.
    let block_67 = eeprom_read_calibration_block(channel, &[], EEPROM_REG_READ_BLOCK_67)?;

    let calibration = EepromCalibration {
        device_id,
        block_35,
        block_67,
        cal_params_written: cal_params.to_vec(),
    };

    info!(
        device_id = format_args!("{:02x?}", &calibration.device_id),
        block_35_len = calibration.block_35.len(),
        block_67_len = calibration.block_67.len(),
        cal_params_len = calibration.cal_params_written.len(),
        "Full EEPROM calibration read complete"
    );

    Ok(calibration)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_proto::device::endpoint;
    use saleae_proto::obfuscation::EncryptionMode;
    use saleae_usb::mock::MockTransport;

    /// Wrapper to clone a MockTransport via Arc for test purposes.
    struct MockTransportWrapper(std::sync::Arc<MockTransport>);

    impl saleae_usb::transport::UsbTransport for MockTransportWrapper {
        fn control_transfer_out(
            &self,
            params: &saleae_usb::transport::ControlTransferParams,
            data: &[u8],
        ) -> saleae_usb::UsbResult<usize> {
            self.0.control_transfer_out(params, data)
        }

        fn control_transfer_in(
            &self,
            params: &saleae_usb::transport::ControlTransferParams,
            buf: &mut [u8],
        ) -> saleae_usb::UsbResult<usize> {
            self.0.control_transfer_in(params, buf)
        }

        fn bulk_transfer_out(&self, endpoint: u8, data: &[u8]) -> saleae_usb::UsbResult<usize> {
            self.0.bulk_transfer_out(endpoint, data)
        }

        fn bulk_transfer_in(&self, endpoint: u8, buf: &mut [u8]) -> saleae_usb::UsbResult<usize> {
            self.0.bulk_transfer_in(endpoint, buf)
        }

        fn reset(&self) -> saleae_usb::UsbResult<()> {
            self.0.reset()
        }
    }

    /// Create a mock EP1 channel with EncryptionMode::None and seed=0.
    /// In None mode, commands are sent unencrypted (no control byte, no XOR).
    fn mock_channel(mock: &std::sync::Arc<MockTransport>) -> Ep1CommandChannel {
        let transport = Box::new(MockTransportWrapper(mock.clone()));
        let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::None);
        channel.encryption.reseed(0);
        channel.initialized = true;
        channel
    }

    // -------------------------------------------------------------------
    // eeprom_read_device_id tests
    // -------------------------------------------------------------------

    #[test]
    fn read_device_id_returns_5_bytes() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // ReadI2C response: 5 bytes
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![0x02, 0x04, 0x11, 0x33, 0x43],
        );

        let mut channel = mock_channel(&mock);
        let id = eeprom_read_device_id(&mut channel).unwrap();
        assert_eq!(id, vec![0x02, 0x04, 0x11, 0x33, 0x43]);
    }

    #[test]
    fn read_device_id_fails_on_empty_response() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Empty response.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![]);

        let mut channel = mock_channel(&mock);
        let err = eeprom_read_device_id(&mut channel).unwrap_err();
        assert!(err.to_string().contains("empty response"));
    }

    // -------------------------------------------------------------------
    // eeprom_read_register_poll tests
    // -------------------------------------------------------------------

    #[test]
    fn polled_read_returns_data_when_ready() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // First poll: not ready (status 0x01).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![EEPROM_STATUS_NOT_READY]);
        // Second poll: ready with 3 data bytes.
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![EEPROM_STATUS_READY, 0xAA, 0xBB, 0xCC],
        );

        let mut channel = mock_channel(&mock);
        let data = eeprom_read_register_poll(
            &mut channel,
            EEPROM_I2C_ADDR,
            EEPROM_REG_READ_BLOCK_35,
        )
        .unwrap();

        assert_eq!(data, vec![0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn polled_read_ready_on_first_attempt() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Immediately ready.
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![EEPROM_STATUS_READY, 0x42, 0xFF],
        );

        let mut channel = mock_channel(&mock);
        let data = eeprom_read_register_poll(
            &mut channel,
            EEPROM_I2C_ADDR,
            EEPROM_REG_READ_BLOCK_35,
        )
        .unwrap();

        assert_eq!(data, vec![0x42, 0xFF]);
    }

    #[test]
    fn polled_read_multiple_not_ready_then_ready() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // 5 not-ready responses, then ready.
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![EEPROM_STATUS_NOT_READY]);
        }
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![EEPROM_STATUS_READY, 0x01, 0x02],
        );

        let mut channel = mock_channel(&mock);
        let data = eeprom_read_register_poll(
            &mut channel,
            EEPROM_I2C_ADDR,
            EEPROM_REG_READ_BLOCK_67,
        )
        .unwrap();

        assert_eq!(data, vec![0x01, 0x02]);
    }

    #[test]
    fn polled_read_unexpected_status_returns_full_response() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Unexpected status byte 0x03.
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![0x03, 0xDE, 0xAD],
        );

        let mut channel = mock_channel(&mock);
        let data = eeprom_read_register_poll(
            &mut channel,
            EEPROM_I2C_ADDR,
            EEPROM_REG_READ_BLOCK_35,
        )
        .unwrap();

        // Returns the full response including the status byte.
        assert_eq!(data, vec![0x03, 0xDE, 0xAD]);
    }

    // -------------------------------------------------------------------
    // eeprom_read_calibration_block tests
    // -------------------------------------------------------------------

    #[test]
    fn read_calibration_block_full_sequence() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Step 1: WriteI2C response (write command to 0x08).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // Step 2: ReadI2C poll - not ready first.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![EEPROM_STATUS_NOT_READY]);
        // Step 2: ReadI2C poll - ready with data.
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![EEPROM_STATUS_READY, 0x10, 0x20, 0x30],
        );
        // Step 3: WriteI2C acknowledge.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel(&mock);
        let data = eeprom_read_calibration_block(
            &mut channel,
            &[],
            EEPROM_REG_READ_BLOCK_35,
        )
        .unwrap();

        assert_eq!(data, vec![0x10, 0x20, 0x30]);
    }

    // -------------------------------------------------------------------
    // eeprom_write_calibration_params tests
    // -------------------------------------------------------------------

    #[test]
    fn write_calibration_params_sends_correct_data() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // WriteI2C response.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel(&mock);
        let cal_data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        eeprom_write_calibration_params(&mut channel, &cal_data).unwrap();

        // Verify the WriteI2C was sent with correct address and register.
        let ops = mock.operations();
        let out_ops: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                saleae_usb::mock::UsbOperation::BulkOut { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(out_ops.len(), 1);
        // In None mode: [opcode=0x87, i2c_addr=0xC0, reg=0x1C, data...]
        let sent = &out_ops[0];
        assert_eq!(sent[0], 0x87, "opcode should be WriteI2C");
        assert_eq!(sent[1], EEPROM_I2C_ADDR, "I2C address should be 0xC0");
        assert_eq!(sent[2], EEPROM_REG_WRITE_CAL, "register should be 0x1C");
        assert_eq!(&sent[3..], &cal_data, "data should match");
    }

    // -------------------------------------------------------------------
    // eeprom_read_full_calibration tests
    // -------------------------------------------------------------------

    #[test]
    fn full_calibration_read_sequence() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Step 1: Read device ID (ReadI2C response).
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![0x02, 0x04, 0x11, 0x33, 0x43],
        );

        // Step 2: Read 35-byte calibration block.
        //   2a: WriteI2C to 0x08 (issue read command).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        //   2b: ReadI2C poll - ready immediately with 3 bytes.
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![EEPROM_STATUS_READY, 0xAA, 0xBB, 0xCC],
        );
        //   2c: WriteI2C acknowledge.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // Step 3: Read device ID again.
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![0x02, 0x04, 0x11, 0x33, 0x43],
        );

        // Step 4: Write calibration params (WriteI2C response).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // Step 5: Polled read (response from 0x23 after write).
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![EEPROM_STATUS_READY, 0xDD, 0xEE],
        );

        // Step 6: Acknowledge.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // Step 7: Read device ID again.
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![0x02, 0x04, 0x11, 0x33, 0x43],
        );

        // Step 8: Read 67-byte calibration block.
        //   8a: WriteI2C to 0x08 (issue read command).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        //   8b: ReadI2C poll - not ready first, then ready.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![EEPROM_STATUS_NOT_READY]);
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![EEPROM_STATUS_READY, 0x11, 0x22, 0x33, 0x44],
        );
        //   8c: WriteI2C acknowledge.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel(&mock);
        let cal_params = vec![0x01, 0x02, 0x03]; // 3-byte test params

        let cal = eeprom_read_full_calibration(&mut channel, &cal_params).unwrap();

        assert_eq!(cal.device_id, vec![0x02, 0x04, 0x11, 0x33, 0x43]);
        assert_eq!(cal.block_35, vec![0xAA, 0xBB, 0xCC]);
        assert_eq!(cal.block_67, vec![0x11, 0x22, 0x33, 0x44]);
        assert_eq!(cal.cal_params_written, vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn full_calibration_detects_device_id_mismatch() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Step 1: Read device ID.
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![0x02, 0x04, 0x11, 0x33, 0x43],
        );

        // Step 2: 35-byte block (immediate ready).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // write cmd
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![EEPROM_STATUS_READY, 0xAA],
        );
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // ack

        // Step 3: Device ID differs!
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        );

        // Step 4: Write cal params.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // Step 5: Polled read.
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![EEPROM_STATUS_READY, 0x42],
        );

        // Step 6: Acknowledge.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // Step 7: Device ID (matches original again).
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![0x02, 0x04, 0x11, 0x33, 0x43],
        );

        // Step 8: 67-byte block (immediate ready).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // write cmd
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![EEPROM_STATUS_READY, 0xBB],
        );
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // ack

        let mut channel = mock_channel(&mock);

        // Should succeed (mismatch is a warning, not an error).
        let cal = eeprom_read_full_calibration(&mut channel, &[]).unwrap();
        assert_eq!(cal.device_id, vec![0x02, 0x04, 0x11, 0x33, 0x43]);
        assert_eq!(cal.block_35, vec![0xAA]);
        assert_eq!(cal.block_67, vec![0xBB]);
    }

    // -------------------------------------------------------------------
    // eeprom_write_register tests
    // -------------------------------------------------------------------

    #[test]
    fn write_register_sends_correct_command() {
        let mock = std::sync::Arc::new(MockTransport::new());

        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel(&mock);
        let resp = eeprom_write_register(
            &mut channel,
            EEPROM_I2C_ADDR,
            EEPROM_REG_ACK,
            &[],
        )
        .unwrap();

        assert_eq!(resp, vec![0x00]);

        let ops = mock.operations();
        let out_ops: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                saleae_usb::mock::UsbOperation::BulkOut { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(out_ops.len(), 1);
        // In None mode: [0x87, 0xC0, 0x01]
        assert_eq!(out_ops[0][0], 0x87);
        assert_eq!(out_ops[0][1], EEPROM_I2C_ADDR);
        assert_eq!(out_ops[0][2], EEPROM_REG_ACK);
    }

    // -------------------------------------------------------------------
    // Constants sanity checks
    // -------------------------------------------------------------------

    #[test]
    fn eeprom_constants_match_pcap() {
        assert_eq!(EEPROM_I2C_ADDR, 0xC0);
        assert_eq!(EEPROM_REG_DEVICE_ID, 0x04);
        assert_eq!(EEPROM_REG_READ_CMD, 0x08);
        assert_eq!(EEPROM_REG_ACK, 0x01);
        assert_eq!(EEPROM_REG_WRITE_CAL, 0x1C);
        assert_eq!(EEPROM_REG_READ_BLOCK_35, 0x23);
        assert_eq!(EEPROM_REG_READ_BLOCK_67, 0x43);
        assert_eq!(EEPROM_STATUS_NOT_READY, 0x01);
        assert_eq!(EEPROM_STATUS_READY, 0x02);
    }
}
