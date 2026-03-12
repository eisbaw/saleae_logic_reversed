//! EP1 command channel for communicating with Saleae devices.
//!
//! After firmware upload, all device control flows through EP1 bulk
//! endpoints using the command/response protocol documented in
//! `doc/protocol/ep1-commands.md`. On Pro devices (Logic Pro 8/16),
//! all traffic is XOR-encrypted using [`SimpleEncryption`].
//!
//! The [`Ep1CommandChannel`] wraps a [`UsbTransport`] and manages the
//! encryption state (seed, LFSR advancement) automatically.

use saleae_proto::device::endpoint;
use saleae_proto::ep1_opcodes;
use saleae_proto::obfuscation::{
    validate_ping_response, EncryptionMode, SimpleEncryption, PING_EXPECTED_RESPONSE, PING_OPCODE,
    PING_TEST_BYTE,
};
use saleae_usb::transport::UsbTransport;
use tracing::{debug, info, warn};

use crate::error::{DriverError, DriverResult};

/// Default FPGA bitstream chunk size for Pro devices at USB 3.0.
///
/// The actual chunk size depends on device type and USB speed. See
/// [`DeviceProfile::fpga_chunk_size`](saleae_proto::device::DeviceProfile::fpga_chunk_size).
///
/// Firmware maximums (decompiled from ELF VA 0x1c544b0):
/// - Non-Pro: 62 bytes (assertion: `data_size <= 0x3E`)
/// - Pro USB 2.0: 509 bytes (we use 508, conservative by 1)
/// - Pro USB 3.0: 1021 bytes (we use 1020, conservative by 1)
///
/// The maximums are calculated so opcode + header + data fills exactly
/// one USB max packet size (64 / 512 / 1024 bytes respectively).
pub const DEFAULT_CONFIG_CHUNK_SIZE: usize = 1020;

/// EP1 command channel for a Saleae device.
///
/// Manages the XOR encryption state and provides typed methods for each
/// EP1 command. The channel must be [`initialize`](Ep1CommandChannel::initialize)d
/// before use -- this sends the ReseedDevice + Ping handshake that
/// establishes the shared encryption seed.
///
/// # Lockstep requirement
///
/// The host and firmware LFSR states must remain synchronized. Every
/// `send_command` advances the seed once (after encrypt), and every
/// `recv_response` advances it again (after decrypt). If any transfer
/// is lost or duplicated, all subsequent communication is garbled.
pub struct Ep1CommandChannel {
    transport: Box<dyn UsbTransport>,
    pub(crate) encryption: SimpleEncryption,
    pub(crate) initialized: bool,
}

impl Ep1CommandChannel {
    /// Create a new command channel.
    ///
    /// The channel starts uninitialized. Call [`initialize`](Self::initialize)
    /// to perform the ReseedDevice + Ping handshake before sending commands.
    ///
    /// # Arguments
    ///
    /// - `transport`: USB transport handle (must already have interface claimed).
    /// - `encryption_mode`: Encryption mode for this device type.
    pub fn new(transport: Box<dyn UsbTransport>, encryption_mode: EncryptionMode) -> Self {
        Self {
            transport,
            encryption: SimpleEncryption::new(
                saleae_proto::obfuscation::DEFAULT_INITIAL_SEED,
                encryption_mode,
            ),
            initialized: false,
        }
    }

    /// Whether the channel has been initialized (ReseedDevice + Ping done).
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Return a reference to the underlying transport (for direct access).
    pub fn transport(&self) -> &dyn UsbTransport {
        self.transport.as_ref()
    }

    // -----------------------------------------------------------------------
    // Initialization: ReseedDevice + Ping
    // -----------------------------------------------------------------------

    /// Initialize the command channel.
    ///
    /// Performs the ReseedDevice + Ping handshake:
    ///
    /// 1. Generate 4 random bytes as the encryption seed.
    /// 2. Encrypt the reseed command with the current LFSR state (using
    ///    `prepare_buffer` with the `| 0x20` control byte flag).
    /// 3. Send the encrypted reseed (5 bytes) on EP1 OUT.
    /// 4. Set the LFSR state to the new seed.
    /// 5. Send an encrypted Ping command `[0x7D, 0x55]`.
    /// 6. Read and decrypt the 1-byte response, verify it equals `0xAA`.
    ///
    /// This matches the `TestFirmwareResponsivnessAndReseed` sequence in
    /// `libgraph_server_shared.so`. The reseed is encrypted with the
    /// current LFSR state (validated against pcap frame 15).
    pub fn initialize(&mut self) -> DriverResult<()> {
        // Step 1: Generate random seed.
        let mut seed_bytes = [0u8; 4];
        getrandom::fill(&mut seed_bytes).map_err(|e| DriverError::CommandFailed {
            command: "ReseedDevice",
            detail: format!("failed to generate random seed: {e}"),
        })?;
        let seed = u32::from_le_bytes(seed_bytes);

        // Avoid seed of 0 -- the LFSR gets stuck at 0.
        let seed = if seed == 0 { 0x0000_0001 } else { seed };
        let seed_bytes = seed.to_le_bytes();

        info!(
            seed = format_args!("{seed:#010x}"),
            "ReseedDevice: sending seed"
        );

        // Step 2: Encrypt the reseed command with current LFSR state.
        // For XorAll mode, this produces a 5-byte packet (control byte +
        // 4 XOR'd seed bytes). For None mode, this returns raw 4 bytes.
        let encrypted = self.encryption.encrypt_reseed(&seed_bytes);

        // Step 3: Send encrypted reseed on EP1 OUT.
        self.transport
            .bulk_transfer_out(endpoint::BULK_OUT, &encrypted)
            .map_err(|e| DriverError::CommandFailed {
                command: "ReseedDevice",
                detail: format!("bulk OUT failed: {e}"),
            })?;

        // Step 4: Set LFSR state to the new seed (discards the advance_seed
        // result from encrypt_reseed).
        self.encryption.reseed(seed);

        // Step 5: Send encrypted Ping.
        debug!("ReseedDevice: sending Ping");
        self.send_command(PING_OPCODE, &[PING_TEST_BYTE])?;

        // Step 6: Read and decrypt response.
        let mut resp_buf = [0u8; 64];
        let n = self.recv_response(&mut resp_buf)?;

        if n < 1 {
            return Err(DriverError::CommandFailed {
                command: "Ping",
                detail: "no response received (0 bytes)".into(),
            });
        }

        let response = resp_buf[0];
        if !validate_ping_response(PING_TEST_BYTE, response) {
            return Err(DriverError::CommandFailed {
                command: "Ping",
                detail: format!(
                    "ping response mismatch: sent {PING_TEST_BYTE:#04x}, \
                     expected {PING_EXPECTED_RESPONSE:#04x}, got {response:#04x}. \
                     This likely means the LFSR implementation is wrong."
                ),
            });
        }

        info!("Ping successful: firmware is responsive, encryption is in sync");
        self.initialized = true;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Low-level send/receive
    // -----------------------------------------------------------------------

    /// Encrypt and send a command on EP1 OUT.
    ///
    /// The encryption layer prepends a control byte and XOR-encrypts the
    /// opcode + payload, then advances the LFSR.
    pub fn send_command(&mut self, opcode: u8, payload: &[u8]) -> DriverResult<()> {
        let encrypted = self.encryption.encrypt_command(opcode, payload, false);
        debug!(
            opcode = format_args!("{opcode:#04x}"),
            payload_len = payload.len(),
            wire_len = encrypted.len(),
            "EP1 OUT"
        );

        self.transport
            .bulk_transfer_out(endpoint::BULK_OUT, &encrypted)
            .map_err(DriverError::Usb)?;

        Ok(())
    }

    /// Read and decrypt a response from EP1 IN.
    ///
    /// Decrypts the received data in place and advances the LFSR.
    /// Returns the number of decrypted bytes.
    pub fn recv_response(&mut self, buf: &mut [u8]) -> DriverResult<usize> {
        let n = self
            .transport
            .bulk_transfer_in(endpoint::BULK_IN, buf)
            .map_err(DriverError::Usb)?;

        self.encryption.decrypt_response(&mut buf[..n], false);
        debug!(len = n, "EP1 IN (decrypted)");
        Ok(n)
    }

    // -----------------------------------------------------------------------
    // Typed command methods
    // -----------------------------------------------------------------------

    /// Send a Ping command and verify the response.
    ///
    /// The firmware returns the bitwise complement of the test byte.
    /// This can be used to verify the encryption is still in sync
    /// at any point during operation.
    pub fn ping(&mut self) -> DriverResult<()> {
        self.send_command(PING_OPCODE, &[PING_TEST_BYTE])?;

        let mut buf = [0u8; 64];
        let n = self.recv_response(&mut buf)?;

        if n < 1 {
            return Err(DriverError::CommandFailed {
                command: "Ping",
                detail: "no response received".into(),
            });
        }

        if !validate_ping_response(PING_TEST_BYTE, buf[0]) {
            return Err(DriverError::CommandFailed {
                command: "Ping",
                detail: format!(
                    "expected {PING_EXPECTED_RESPONSE:#04x}, got {:#04x}",
                    buf[0]
                ),
            });
        }

        debug!("Ping OK");
        Ok(())
    }

    /// Write FPGA registers (opcode 0x80).
    ///
    /// Sends batch register writes. For Pro devices, reads back a
    /// 1-byte acknowledgment.
    ///
    /// # Panics
    ///
    /// Panics if `pairs` is empty or has more than 31 entries.
    ///
    /// The firmware assertion is `num_registers < 32`, so the valid range
    /// is 1..=31. Pro devices return a 1-byte ACK that must be 0x00;
    /// non-Pro devices are fire-and-forget (no response read).
    ///
    /// Decompiled from ELF VA 0x1c535f0 (non-Pro) and 0x1c549d0 (Pro).
    pub fn write_registers(&mut self, pairs: &[(u8, u8)]) -> DriverResult<()> {
        assert!(
            !pairs.is_empty() && pairs.len() < 32,
            "register count must be 1..31, got {}",
            pairs.len()
        );

        let mut payload = Vec::with_capacity(1 + pairs.len() * 2);
        payload.push(pairs.len() as u8);
        for &(addr, val) in pairs {
            payload.push(addr);
            payload.push(val);
        }

        self.send_command(ep1_opcodes::WRITE_REGISTERS, &payload)?;

        // Pro devices read a 1-byte response and throw if non-zero.
        // Non-Pro devices skip the response read entirely.
        // Since our target is Pro 8, we always expect the ACK.
        let mut ack = [0u8; 64];
        let n = self.recv_response(&mut ack)?;
        debug!(
            ack_len = n,
            ack_byte = if n > 0 {
                format!("{:#04x}", ack[0])
            } else {
                "none".into()
            },
            "WriteRegisters ACK"
        );

        if n >= 1 && ack[0] != 0x00 {
            return Err(DriverError::CommandFailed {
                command: "WriteRegisters",
                detail: format!("expected 0x00 ack, got {:#04x}", ack[0]),
            });
        }

        Ok(())
    }

    /// Read FPGA registers (opcode 0x81).
    ///
    /// Returns the register values in the same order as the addresses.
    ///
    /// # Panics
    ///
    /// Panics if `addrs` is empty or has more than 62 entries.
    ///
    /// The firmware assertion is `(count - 1) <= 0x3D`, so the valid range
    /// is 1..=62 (decompiled from ELF VA 0x1c53af0).
    pub fn read_registers(&mut self, addrs: &[u8]) -> DriverResult<Vec<u8>> {
        assert!(
            !addrs.is_empty() && addrs.len() <= 62,
            "register count must be 1..62, got {}",
            addrs.len()
        );

        let mut payload = Vec::with_capacity(1 + addrs.len());
        payload.push(addrs.len() as u8);
        payload.extend_from_slice(addrs);

        self.send_command(ep1_opcodes::READ_REGISTERS, &payload)?;

        let mut buf = [0u8; 64];
        let n = self.recv_response(&mut buf)?;

        if n < addrs.len() {
            warn!(
                expected = addrs.len(),
                got = n,
                "ReadRegisters: fewer bytes than expected"
            );
        }

        Ok(buf[..n].to_vec())
    }

    /// Reset the FPGA (opcode 0x7E).
    ///
    /// Must be called before programming a new bitstream.
    pub fn reset_fpga(&mut self) -> DriverResult<()> {
        self.send_command(ep1_opcodes::RESET_FPGA, &[])?;

        let mut buf = [0u8; 64];
        let n = self.recv_response(&mut buf)?;

        if n >= 1 && buf[0] != 0x00 {
            warn!(
                response = format_args!("{:#04x}", buf[0]),
                "ResetFpga: unexpected response byte"
            );
        }

        debug!("ResetFpga complete");
        Ok(())
    }

    /// Write configuration data (opcode 0x7F), chunked for the device.
    ///
    /// Used for FPGA bitstream download and capture configuration.
    /// Uses the default Pro USB 3.0 chunk size (1020 bytes). For
    /// device-specific chunk sizes, use
    /// [`write_config_data_chunked`](Self::write_config_data_chunked).
    pub fn write_config_data(&mut self, data: &[u8]) -> DriverResult<()> {
        self.write_config_data_chunked(data, DEFAULT_CONFIG_CHUNK_SIZE, true)
    }

    /// Write configuration data (opcode 0x7F) with explicit chunk size.
    ///
    /// The chunk size should come from
    /// [`DeviceProfile::fpga_chunk_size`](saleae_proto::device::DeviceProfile::fpga_chunk_size).
    ///
    /// # Arguments
    ///
    /// - `data`: The configuration/bitstream data to send.
    /// - `chunk_size`: Maximum data bytes per chunk (62, 508, or 1020).
    /// - `read_ack`: Whether to read a response ACK after each chunk.
    ///   Pro devices verify writes; non-Pro devices are fire-and-forget.
    ///
    /// # Wire format
    ///
    /// This method uses the **Pro** wire format:
    /// `[0x7F][len_lo][len_hi][data...]`, with optional 1-byte ACK read.
    ///
    /// For non-Pro devices, use [`write_config_data_non_pro`](Self::write_config_data_non_pro)
    /// which uses the 2-byte header format: `[0x7F][len_8bit][data...]`.
    pub fn write_config_data_chunked(
        &mut self,
        data: &[u8],
        chunk_size: usize,
        read_ack: bool,
    ) -> DriverResult<()> {
        let total = data.len();
        let mut offset = 0;

        while offset < total {
            let remaining = total - offset;
            let chunk_len = remaining.min(chunk_size);
            let chunk = &data[offset..offset + chunk_len];

            // Pro format: [len_lo] [len_hi] [data...]
            let len16 = chunk_len as u16;
            let mut payload = Vec::with_capacity(2 + chunk_len);
            payload.push(len16 as u8); // len_lo
            payload.push((len16 >> 8) as u8); // len_hi
            payload.extend_from_slice(chunk);

            self.send_command(ep1_opcodes::WRITE_CONFIG_DATA, &payload)?;

            if read_ack {
                // Read success response (0x00).
                let mut ack = [0u8; 64];
                let n = self.recv_response(&mut ack)?;

                if n >= 1 && ack[0] != 0x00 {
                    return Err(DriverError::CommandFailed {
                        command: "WriteConfigurationData",
                        detail: format!(
                            "expected 0x00 ack, got {:#04x} at offset {offset}",
                            ack[0]
                        ),
                    });
                }
            }

            offset += chunk_len;
            debug!(
                offset,
                total, chunk_len, "WriteConfigurationData chunk sent"
            );
        }

        info!(total_bytes = total, "WriteConfigurationData complete");
        Ok(())
    }

    /// Write configuration data using the non-Pro wire format.
    ///
    /// Non-Pro devices (Logic 4, Logic 8) use a simpler 2-byte header with
    /// an 8-bit length field and no response ACK:
    ///
    /// ```text
    /// EP1 OUT: [0x7F] [len_8bit] [data...]   (max data: 62 bytes)
    /// ```
    ///
    /// No response is read -- non-Pro configuration writes are fire-and-forget.
    ///
    /// From `re/notes/usb2-fallback.md`:
    /// - Max data per chunk: 62 bytes
    /// - Assertion: `data_size <= 0x3E`
    /// - Source: `FpgaDeviceFeatures::WriteConfigurationData` at ELF VA 0x1c544b0
    ///
    /// # Arguments
    ///
    /// - `data`: The configuration/bitstream data to send.
    /// - `chunk_size`: Maximum data bytes per chunk (should be 62 for non-Pro).
    pub fn write_config_data_non_pro(
        &mut self,
        data: &[u8],
        chunk_size: usize,
    ) -> DriverResult<()> {
        assert!(
            chunk_size <= 62,
            "non-Pro chunk size must be <= 62, got {chunk_size}"
        );

        let total = data.len();
        let mut offset = 0;

        while offset < total {
            let remaining = total - offset;
            let chunk_len = remaining.min(chunk_size);
            let chunk = &data[offset..offset + chunk_len];

            // Non-Pro format: [len_8bit] [data...]
            let mut payload = Vec::with_capacity(1 + chunk_len);
            payload.push(chunk_len as u8); // 8-bit length
            payload.extend_from_slice(chunk);

            self.send_command(ep1_opcodes::WRITE_CONFIG_DATA, &payload)?;

            // No ACK read -- non-Pro is fire-and-forget.

            offset += chunk_len;
            debug!(
                offset,
                total, chunk_len, "WriteConfigurationData (non-Pro) chunk sent"
            );
        }

        info!(
            total_bytes = total,
            "WriteConfigurationData (non-Pro) complete"
        );
        Ok(())
    }

    /// Write configuration data using the correct protocol for the device.
    ///
    /// Dispatches to either the Pro or non-Pro wire format based on the
    /// FPGA type. This is the preferred entry point for device-agnostic
    /// code.
    ///
    /// # Arguments
    ///
    /// - `data`: The configuration/bitstream data to send.
    /// - `profile`: Device hardware profile.
    /// - `usb_speed`: Current USB connection speed.
    pub fn write_config_data_for_device(
        &mut self,
        data: &[u8],
        profile: &saleae_proto::device::DeviceProfile,
        usb_speed: saleae_proto::device::UsbSpeed,
    ) -> DriverResult<()> {
        let chunk_size = profile.fpga_chunk_size(usb_speed);
        let read_ack = profile.register_write_verifies();

        match profile.fpga_type {
            saleae_proto::device::FpgaType::Pro => {
                self.write_config_data_chunked(data, chunk_size, read_ack)
            }
            saleae_proto::device::FpgaType::NonPro => {
                self.write_config_data_non_pro(data, chunk_size)
            }
            saleae_proto::device::FpgaType::Mso => {
                // MSO is out of scope. Fall back to Pro format as placeholder.
                self.write_config_data_chunked(data, chunk_size, read_ack)
            }
        }
    }

    /// Start sampling (opcode 0x01).
    ///
    /// Tells the FX3 to begin forwarding FPGA sample data to EP2 IN.
    pub fn start_sampling(&mut self) -> DriverResult<()> {
        self.send_command(ep1_opcodes::START_READ, &[])?;

        let mut buf = [0u8; 64];
        let n = self.recv_response(&mut buf)?;

        if n >= 2 {
            debug!(
                byte0 = format_args!("{:#04x}", buf[0]),
                byte1 = format_args!("{:#04x}", buf[1]),
                "StartSampling response"
            );
        }

        Ok(())
    }

    /// Stop data stream (opcode 0x02).
    ///
    /// Tells the FX3 to stop forwarding data on EP2 IN.
    pub fn stop_stream(&mut self) -> DriverResult<()> {
        self.send_command(ep1_opcodes::STOP_STREAM, &[])?;

        let mut buf = [0u8; 64];
        let _n = self.recv_response(&mut buf)?;

        debug!("StopStream complete");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // 16-bit register access (raw path)
    // -----------------------------------------------------------------------

    /// Write a 32-bit value to a 16-bit addressed FPGA register.
    ///
    /// This uses the raw register path: a 6-byte packet sent directly on
    /// EP1 OUT, without opcode framing:
    ///
    /// ```text
    /// [addr_hi] [addr_lo] [val_3] [val_2] [val_1] [val_0]
    /// ```
    ///
    /// The value is sent big-endian on the wire.
    ///
    /// # Encryption
    ///
    /// On Pro devices, this packet is encrypted using the same XOR scheme
    /// as opcode-based commands. We treat `addr_hi` as the "opcode" for
    /// the control byte computation and the remaining 5 bytes as payload.
    /// This is a hypothesis based on the fact that all EP1 traffic on Pro
    /// devices goes through the SimpleEncryption layer. If hardware testing
    /// shows this is wrong, we may need a separate raw-send path.
    ///
    /// # When to use
    ///
    /// Use this for register addresses > 0xFF (e.g., 0x01C0 COMMIT_CONTROL).
    /// For addresses <= 0xFF, prefer [`write_registers`](Self::write_registers)
    /// (opcode 0x80) which is known-good.
    pub fn write_register_16(&mut self, addr: u16, value: u32) -> DriverResult<()> {
        let addr_hi = (addr >> 8) as u8;
        let addr_lo = (addr & 0xFF) as u8;
        let val_bytes = value.to_be_bytes();

        // Send as: opcode=addr_hi, payload=[addr_lo, val3, val2, val1, val0]
        let payload = [
            addr_lo,
            val_bytes[0],
            val_bytes[1],
            val_bytes[2],
            val_bytes[3],
        ];

        debug!(
            addr = format_args!("{addr:#06x}"),
            value = format_args!("{value:#010x}"),
            "FPGA WriteRegister (16-bit path)"
        );

        self.send_command(addr_hi, &payload)?;

        // Read ACK (Pro devices send a response for all EP1 writes).
        let mut ack = [0u8; 64];
        let n = self.recv_response(&mut ack)?;
        debug!(ack_len = n, "WriteRegister16 ACK");

        Ok(())
    }

    /// Read a 32-bit value from a 16-bit addressed FPGA register.
    ///
    /// Sends a 2-byte address packet on EP1 OUT and reads a 4-byte
    /// big-endian value from EP1 IN:
    ///
    /// ```text
    /// Send:    [addr_hi] [addr_lo]
    /// Receive: [val_3] [val_2] [val_1] [val_0]
    /// ```
    ///
    /// See [`write_register_16`](Self::write_register_16) for encryption notes.
    pub fn read_register_16(&mut self, addr: u16) -> DriverResult<u32> {
        let addr_hi = (addr >> 8) as u8;
        let addr_lo = (addr & 0xFF) as u8;

        debug!(
            addr = format_args!("{addr:#06x}"),
            "FPGA ReadRegister (16-bit path)"
        );

        self.send_command(addr_hi, &[addr_lo])?;

        let mut buf = [0u8; 64];
        let n = self.recv_response(&mut buf)?;

        if n < 4 {
            return Err(DriverError::CommandFailed {
                command: "ReadRegister16",
                detail: format!("expected 4 bytes, got {n}"),
            });
        }

        let value = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        debug!(
            addr = format_args!("{addr:#06x}"),
            value = format_args!("{value:#010x}"),
            "FPGA ReadRegister (16-bit path) result"
        );

        Ok(value)
    }

    /// Write a single 8-bit register via the opcode-based path.
    ///
    /// Convenience wrapper around [`write_registers`](Self::write_registers)
    /// for single-register writes.
    pub fn write_register_8(&mut self, addr: u8, value: u8) -> DriverResult<()> {
        self.write_registers(&[(addr, value)])
    }

    /// Read a single 8-bit register via the opcode-based path.
    ///
    /// Convenience wrapper around [`read_registers`](Self::read_registers)
    /// for single-register reads.
    pub fn read_register_8(&mut self, addr: u8) -> DriverResult<u8> {
        let values = self.read_registers(&[addr])?;
        if values.is_empty() {
            return Err(DriverError::CommandFailed {
                command: "ReadRegister8",
                detail: "no value returned".into(),
            });
        }
        Ok(values[0])
    }

    /// Write an FPGA register, automatically selecting the correct path.
    ///
    /// - Addresses <= 0xFF use the opcode-based 8-bit path (opcode 0x80).
    /// - Addresses > 0xFF use the raw 16-bit path.
    ///
    /// The value is truncated to 8 bits for the opcode-based path.
    /// For 8-bit path writes, only the low byte of `value` is used.
    pub fn write_fpga_register(&mut self, addr: u16, value: u32) -> DriverResult<()> {
        if saleae_proto::fpga_regs::needs_wide_path(addr) {
            self.write_register_16(addr, value)
        } else {
            // 8-bit path: addr and value must fit in u8.
            self.write_register_8(addr as u8, value as u8)
        }
    }

    /// Read an FPGA register, automatically selecting the correct path.
    ///
    /// - Addresses <= 0xFF use the opcode-based 8-bit path (opcode 0x81).
    /// - Addresses > 0xFF use the raw 16-bit path.
    ///
    /// For 8-bit path reads, the returned value is zero-extended to u32.
    pub fn read_fpga_register(&mut self, addr: u16) -> DriverResult<u32> {
        if saleae_proto::fpga_regs::needs_wide_path(addr) {
            self.read_register_16(addr)
        } else {
            let val = self.read_register_8(addr as u8)?;
            Ok(val as u32)
        }
    }

    /// Write a single bit in an 8-bit FPGA register (read-modify-write).
    ///
    /// This is the Rust equivalent of `FpgaDeviceFeatures::WriteRegisterBit`
    /// (decompiled from ELF VA 0x1c53e30).
    ///
    /// # Arguments
    ///
    /// - `addr`: 8-bit register address.
    /// - `bit_value`: `true` to set the bit, `false` to clear it.
    /// - `bit_position`: Which bit to modify (0..7).
    ///
    /// # Panics
    ///
    /// Panics if `bit_position >= 8`.
    pub fn write_register_bit(
        &mut self,
        addr: u8,
        bit_value: bool,
        bit_position: u8,
    ) -> DriverResult<()> {
        assert!(
            bit_position < 8,
            "bit_position must be 0..7, got {bit_position}"
        );

        let current = self.read_register_8(addr)?;
        let mask = 1u8 << bit_position;
        let new_val = if bit_value {
            current | mask
        } else {
            current & !mask
        };

        debug!(
            addr = format_args!("{addr:#04x}"),
            bit = bit_position,
            set = bit_value,
            old = format_args!("{current:#04x}"),
            new = format_args!("{new_val:#04x}"),
            "WriteRegisterBit"
        );

        self.write_register_8(addr, new_val)
    }

    // -----------------------------------------------------------------------
    // EEPROM / I2C tunnel
    // -----------------------------------------------------------------------

    /// Read the 64-bit device ID from EEPROM via the I2C tunnel (opcode 0x88).
    ///
    /// The device ID is stored in EEPROM and is used to download per-device
    /// calibration data from `https://downloads.saleae.com/calibration/{id:x}.cal`.
    ///
    /// # Wire format (HYPOTHESIS)
    ///
    /// The I2C tunnel parameters (slave address, register offset, read length)
    /// are UNKNOWN. Based on RE analysis, opcode 0x88 triggers an I2C read
    /// from the EEPROM. The exact payload format needs USB capture confirmation.
    ///
    /// Current hypothesis:
    /// ```text
    /// EP1 OUT: [0x88]              (no payload -- firmware knows what to read)
    /// EP1 IN:  [8 bytes]           (64-bit device ID, little-endian)
    /// ```
    ///
    /// # Returns
    ///
    /// The 64-bit device ID, or an error if the read fails.
    pub fn read_eeprom_device_id(&mut self) -> DriverResult<u64> {
        info!("Reading EEPROM device ID (opcode 0x88)");

        // PLACEHOLDER: We send opcode 0x88 with no payload. The actual
        // I2C parameters (slave address, register offset) may need to be
        // included in the payload. This needs USB capture confirmation.
        self.send_command(ep1_opcodes::READ_I2C, &[])?;

        let mut buf = [0u8; 64];
        let n = self.recv_response(&mut buf)?;

        if n < 8 {
            return Err(DriverError::CommandFailed {
                command: "ReadEepromDeviceId",
                detail: format!("expected at least 8 bytes for device ID, got {n}"),
            });
        }

        // Parse as little-endian u64 (HYPOTHESIS -- endianness needs
        // USB capture confirmation).
        let device_id = u64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]);

        info!(
            device_id = format_args!("{device_id:#018x}"),
            "EEPROM device ID read"
        );
        Ok(device_id)
    }

    // -----------------------------------------------------------------------
    // Device info commands
    // -----------------------------------------------------------------------

    /// Get device info string (opcode 0x8B).
    ///
    /// Returns the device identification string, e.g.
    /// "Logic Pro Firmware Compiled Oct 25 2022 16:42:00".
    ///
    /// The response is a variable-length null-terminated string (up to 128
    /// bytes observed in pcap frame 24).
    pub fn get_device_info(&mut self) -> DriverResult<String> {
        self.send_command(ep1_opcodes::GET_DEVICE_INFO, &[])?;

        let mut buf = [0u8; 256];
        let n = self.recv_response(&mut buf)?;

        // Find null terminator or use full length.
        let end = buf[..n].iter().position(|&b| b == 0).unwrap_or(n);
        let info = String::from_utf8_lossy(&buf[..end]).to_string();

        debug!(info = %info, "GetDeviceInfo response");
        Ok(info)
    }

    /// Read firmware/hardware version (opcode 0x07).
    ///
    /// Sends a 6-byte parameter block and receives a variable-length
    /// response. The parameters observed in the pcap are:
    ///
    /// - `[0x33, 0x81, 0x08, 0x00, 0x08, 0x00]` -- returns 8 bytes
    /// - `[0x33, 0x81, 0x20, 0x00, 0x06, 0x00]` -- returns "REV." + version
    /// - `[0x33, 0x81, 0x10, 0x00, 0x10, 0x00]` -- returns 16 bytes (calibration?)
    ///
    /// The parameter format is: `[0x33, 0x81, offset_lo, offset_hi,
    /// length_lo, length_hi]`. This is the FX3 I2C EEPROM read tunnel.
    pub fn read_version(&mut self, params: &[u8]) -> DriverResult<Vec<u8>> {
        self.send_command(ep1_opcodes::READ_VERSION, params)?;

        let mut buf = [0u8; 256];
        let n = self.recv_response(&mut buf)?;

        debug!(len = n, "ReadVersion response");
        Ok(buf[..n].to_vec())
    }

    /// Read a block from the FX3 on-board EEPROM.
    ///
    /// Uses the ReadVersion (opcode 0x07) I2C tunnel with params
    /// `[0x33, 0x81, addr_lo, addr_hi, count_lo, count_hi]`.
    ///
    /// Decompiled from `Fx3DeviceFeatures::ReadEeprom` (ELF VA 0x1c52460).
    /// The firmware uses the same opcode for both version reads and EEPROM
    /// reads -- the `0x33, 0x81` prefix selects the I2C EEPROM path.
    ///
    /// # Arguments
    ///
    /// - `address`: EEPROM byte offset (0x0000..0x0FFF for 4K EEPROM).
    /// - `count`: Number of bytes to read (max 256 per call).
    pub fn read_eeprom(&mut self, address: u16, count: u16) -> DriverResult<Vec<u8>> {
        let params = [
            0x33,
            0x81,
            (address & 0xFF) as u8,
            (address >> 8) as u8,
            (count & 0xFF) as u8,
            (count >> 8) as u8,
        ];
        debug!(
            address = format_args!("{address:#06x}"),
            count, "ReadEeprom"
        );
        self.read_version(&params)
    }

    /// Write a block to the FX3 on-board EEPROM.
    ///
    /// Uses the ReadVersion (opcode 0x07) I2C tunnel. The write is a
    /// two-step EP1 transaction:
    ///
    /// 1. Send command: opcode 0x07, params `[0x33, 0x81, addr_lo, count]`
    /// 2. Send data: opcode 0x07, params = data bytes
    ///
    /// Decompiled from `Fx3DeviceFeatures::WriteEeprom` (ELF VA 0x1c50d80).
    /// The original retries up to 3 times on exception.
    ///
    /// # Arguments
    ///
    /// - `address`: EEPROM byte offset (low byte used -- chunks of 256).
    /// - `data`: Bytes to write (max 256 bytes per call).
    ///
    /// # Warning
    ///
    /// **Destructive** -- writes to device EEPROM. Incorrect data can
    /// brick the device. The first 8 bytes (CY header + VID/PID) should
    /// not be overwritten unless you know what you're doing.
    pub fn write_eeprom(&mut self, address: u16, data: &[u8]) -> DriverResult<()> {
        let count = data.len();
        if count > 256 {
            return Err(DriverError::CommandFailed {
                command: "WriteEeprom",
                detail: format!("data too large: {count} bytes (max 256)"),
            });
        }
        debug!(
            address = format_args!("{address:#06x}"),
            count, "WriteEeprom"
        );

        // Step 1: Send the address/count header via ReadVersion command.
        let header = [0x33, 0x81, (address & 0xFF) as u8, count as u8];
        self.send_command(ep1_opcodes::READ_VERSION, &header)?;
        let mut buf = [0u8; 256];
        let _n = self.recv_response(&mut buf)?;

        // Step 2: Send the data payload via a second ReadVersion command.
        // The firmware expects the data as a follow-up to the address header.
        self.send_command(ep1_opcodes::READ_VERSION, data)?;
        let _n = self.recv_response(&mut buf)?;

        Ok(())
    }

    /// Device2_1 command (opcode 0x89).
    ///
    /// Observed in pcap Phase 6 before EEPROM calibration reads. The exact
    /// purpose is unknown -- possibly configures EEPROM/I2C access mode.
    /// Response is a single byte (0x00 observed).
    pub fn device2_1(&mut self) -> DriverResult<u8> {
        self.send_command(ep1_opcodes::DEVICE2_1, &[])?;

        let mut buf = [0u8; 64];
        let n = self.recv_response(&mut buf)?;

        let response = if n > 0 { buf[0] } else { 0 };
        debug!(
            response = format_args!("{response:#04x}"),
            "Device2_1 response"
        );
        Ok(response)
    }

    /// Write to I2C device (opcode 0x87).
    ///
    /// Tunnels an I2C write through the FX3. Used for EEPROM writes and
    /// other I2C peripherals.
    ///
    /// # Arguments
    ///
    /// - `i2c_addr`: 8-bit I2C device address (includes R/W bit).
    /// - `reg`: Register/command byte.
    /// - `data`: Additional data bytes to write.
    pub fn write_i2c(&mut self, i2c_addr: u8, reg: u8, data: &[u8]) -> DriverResult<Vec<u8>> {
        let mut payload = Vec::with_capacity(2 + data.len());
        payload.push(i2c_addr);
        payload.push(reg);
        payload.extend_from_slice(data);

        self.send_command(ep1_opcodes::WRITE_I2C, &payload)?;

        let mut buf = [0u8; 256];
        let n = self.recv_response(&mut buf)?;

        debug!(len = n, "WriteI2C response");
        Ok(buf[..n].to_vec())
    }

    /// Read from I2C device (opcode 0x88).
    ///
    /// Tunnels an I2C read through the FX3. The payload format varies:
    /// when used with count=0, reads all available data. When used with
    /// a specific count, reads that many bytes.
    ///
    /// # Arguments
    ///
    /// - `i2c_addr`: 8-bit I2C device address (includes R/W bit).
    /// - `reg`: Register/command byte.
    /// - `count`: Number of bytes to read (0 = read all available).
    pub fn read_i2c(&mut self, i2c_addr: u8, reg: u8, count: u8) -> DriverResult<Vec<u8>> {
        let payload = [i2c_addr, reg, count];
        self.send_command(ep1_opcodes::READ_I2C, &payload)?;

        let mut buf = [0u8; 256];
        let n = self.recv_response(&mut buf)?;

        debug!(len = n, "ReadI2C response");
        Ok(buf[..n].to_vec())
    }

    // -----------------------------------------------------------------------
    // Capture control
    // -----------------------------------------------------------------------

    /// Stop data path (opcode 0x86).
    ///
    /// Part of the capture stop sequence.
    pub fn stop_data_path(&mut self) -> DriverResult<()> {
        self.send_command(ep1_opcodes::STOP_DATA_PATH, &[])?;

        // The response format for this command is not fully documented.
        // Try to read a response, but don't fail if none comes.
        let mut buf = [0u8; 64];
        match self.transport.bulk_transfer_in(endpoint::BULK_IN, &mut buf) {
            Ok(n) => {
                self.encryption.decrypt_response(&mut buf[..n], false);
                debug!(len = n, "StopDataPath response");
            }
            Err(e) => {
                // Some devices may not send a response for 0x86.
                debug!(error = %e, "StopDataPath: no response (may be normal)");
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_proto::obfuscation::EncryptionMode;
    use saleae_usb::mock::{MockTransport, UsbOperation};

    /// Create a mock channel with XorAll encryption and a known seed.
    fn mock_channel_with_seed(
        mock: &std::sync::Arc<MockTransport>,
        seed: u32,
    ) -> Ep1CommandChannel {
        let transport = Box::new(MockTransportWrapper(mock.clone()));
        let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::XorAll);
        // Manually set the seed (bypassing initialize).
        channel.encryption.reseed(seed);
        channel.initialized = true;
        channel
    }

    /// Wrapper to clone a MockTransport via Arc for test purposes.
    struct MockTransportWrapper(std::sync::Arc<MockTransport>);

    impl UsbTransport for MockTransportWrapper {
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

    #[test]
    fn new_channel_is_uninitialized() {
        let mock = Box::new(MockTransport::new());
        let channel = Ep1CommandChannel::new(mock, EncryptionMode::XorAll);
        assert!(!channel.is_initialized());
    }

    #[test]
    fn new_channel_uses_firmware_default_seed() {
        let mock = Box::new(MockTransport::new());
        let channel = Ep1CommandChannel::new(mock, EncryptionMode::XorAll);
        assert_eq!(
            channel.encryption.seed(),
            saleae_proto::obfuscation::DEFAULT_INITIAL_SEED,
            "Ep1CommandChannel must start with firmware's hardcoded seed"
        );
    }

    #[test]
    fn initialize_sends_seed_then_ping() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // The initialize sequence:
        // 1. Bulk OUT: encrypted reseed (5 bytes for XorAll, 4 raw for None)
        // 2. Bulk OUT: encrypted ping command
        // 3. Bulk IN: encrypted ping response

        // Test with None mode to verify the protocol flow. In None mode,
        // encrypt_reseed returns raw 4-byte seed (no control byte, no XOR).
        // For encryption correctness, see pcap_validation tests in
        // obfuscation.rs.
        let transport = Box::new(MockTransportWrapper(mock.clone()));
        let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::None);

        // Queue response for the ping (unencrypted since mode is None).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);

        channel.initialize().unwrap();
        assert!(channel.is_initialized());

        let ops = mock.operations();
        // Op 0: Bulk OUT (4-byte seed, raw in None mode)
        // Op 1: Bulk OUT (ping command: [0x7D, 0x55])
        // Op 2: Bulk IN (ping response)
        assert_eq!(ops.len(), 3, "expected 3 operations, got {}", ops.len());

        // Verify seed is 4 bytes
        match &ops[0] {
            UsbOperation::BulkOut { endpoint, data } => {
                assert_eq!(*endpoint, endpoint::BULK_OUT);
                assert_eq!(data.len(), 4, "seed should be 4 bytes");
            }
            other => panic!("expected BulkOut for seed, got {other:?}"),
        }

        // Verify ping command
        match &ops[1] {
            UsbOperation::BulkOut { endpoint, data } => {
                assert_eq!(*endpoint, endpoint::BULK_OUT);
                // In None mode, no control byte: [0x7D, 0x55]
                assert_eq!(data, &[PING_OPCODE, PING_TEST_BYTE]);
            }
            other => panic!("expected BulkOut for ping, got {other:?}"),
        }
    }

    #[test]
    fn ping_verifies_complement() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Use seed 0 so XOR is identity (encryption is effectively off).
        let seed = 0u32;

        // Queue: encrypted ping response. With seed=0 the response value
        // is unchanged, so 0xAA is correct. But seed=0 stays 0, so the
        // advance_seed is a no-op. This means the encrypt for the command
        // also leaves it unchanged.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        channel.ping().unwrap();
    }

    #[test]
    fn ping_wrong_response_fails() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Queue wrong response
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x55]); // wrong!

        let mut channel = mock_channel_with_seed(&mock, seed);
        let err = channel.ping().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("expected"),
            "error should mention expected value: {msg}"
        );
    }

    #[test]
    fn write_registers_sends_correct_format() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Queue ACK response
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        channel
            .write_registers(&[(0x10, 0xAA), (0x20, 0xBB)])
            .unwrap();

        let ops = mock.operations();
        // Op 0: Bulk OUT (command)
        match &ops[0] {
            UsbOperation::BulkOut { data, .. } => {
                // With seed=0 and XorAll, the control byte is added but XOR is identity.
                // Control byte = (0x80 & 0xD7) ^ (0x00 & 0xD7) = 0x80
                // Then: 0x80, 0x02, 0x10, 0xAA, 0x20, 0xBB
                assert_eq!(data[0], 0x80 & 0xD7); // control byte
                assert_eq!(data[1], 0x80); // opcode
                assert_eq!(data[2], 0x02); // count
                assert_eq!(data[3], 0x10); // addr0
                assert_eq!(data[4], 0xAA); // val0
                assert_eq!(data[5], 0x20); // addr1
                assert_eq!(data[6], 0xBB); // val1
            }
            other => panic!("expected BulkOut, got {other:?}"),
        }
    }

    #[test]
    fn read_registers_returns_values() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Queue register values response
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x42, 0xFF]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        let values = channel.read_registers(&[0x10, 0x20]).unwrap();
        assert_eq!(values, vec![0x42, 0xFF]);
    }

    #[test]
    fn reset_fpga_sends_command() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        channel.reset_fpga().unwrap();
    }

    #[test]
    fn write_config_data_chunks_correctly() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Data larger than one chunk. Default chunk size is 1020 bytes (Pro USB 3.0).
        // 2048 bytes -> ceil(2048/1020) = 3 chunks: 1020, 1020, 8.
        let data = vec![0xABu8; 2048];

        // Queue 3 ACK responses.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        channel.write_config_data(&data).unwrap();

        // Verify 3 chunks were sent (3 OUT + 3 IN = 6 operations)
        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        assert_eq!(out_count, 3, "expected 3 OUT transfers for 2048 bytes");
    }

    #[test]
    #[should_panic(expected = "register count must be 1..31")]
    fn write_registers_panics_on_empty() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut channel = mock_channel_with_seed(&mock, 0);
        channel.write_registers(&[]).unwrap();
    }

    #[test]
    #[should_panic(expected = "register count must be 1..31")]
    fn write_registers_panics_on_too_many() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut channel = mock_channel_with_seed(&mock, 0);
        let pairs: Vec<(u8, u8)> = (0..32).map(|i| (i, 0)).collect();
        channel.write_registers(&pairs).unwrap();
    }

    #[test]
    #[should_panic(expected = "register count must be 1..62")]
    fn read_registers_panics_on_empty() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut channel = mock_channel_with_seed(&mock, 0);
        channel.read_registers(&[]).unwrap();
    }

    // -- 16-bit register path tests --

    #[test]
    fn write_register_16_sends_correct_wire_format() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Queue ACK for the write
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        channel.write_register_16(0x01C0, 0xDEADBEEF).unwrap();

        let ops = mock.operations();
        match &ops[0] {
            UsbOperation::BulkOut { data, .. } => {
                // With seed=0, XOR is identity. Control byte + opcode + payload.
                // opcode = addr_hi = 0x01
                // payload = [addr_lo=0xC0, val3=0xDE, val2=0xAD, val1=0xBE, val0=0xEF]
                //
                // control byte = (0x01 & 0xD7) ^ (0x00 & 0xD7) = 0x01
                assert_eq!(data[0], 0x01 & 0xD7); // control byte
                assert_eq!(data[1], 0x01); // opcode (addr_hi)
                assert_eq!(data[2], 0xC0); // addr_lo
                assert_eq!(data[3], 0xDE); // val[3] (MSB, big-endian)
                assert_eq!(data[4], 0xAD); // val[2]
                assert_eq!(data[5], 0xBE); // val[1]
                assert_eq!(data[6], 0xEF); // val[0] (LSB)
            }
            other => panic!("expected BulkOut, got {other:?}"),
        }
    }

    #[test]
    fn read_register_16_returns_correct_value() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Queue 4-byte big-endian response: 0xCAFEBABE
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xCA, 0xFE, 0xBA, 0xBE]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        let value = channel.read_register_16(0x0104).unwrap();
        assert_eq!(value, 0xCAFE_BABE);
    }

    #[test]
    fn read_register_16_fails_on_short_response() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Only 2 bytes instead of 4
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA, 0xBB]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        let err = channel.read_register_16(0x0104).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("expected 4 bytes"),
            "error should mention expected bytes: {msg}"
        );
    }

    #[test]
    fn write_register_8_convenience() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        channel.write_register_8(0x20, 0x01).unwrap();

        // Verify it sent a WriteRegisters command (opcode 0x80) with 1 pair.
        let ops = mock.operations();
        match &ops[0] {
            UsbOperation::BulkOut { data, .. } => {
                // control byte for opcode 0x80 with seed=0: (0x80 & 0xD7) = 0x80
                assert_eq!(data[0], 0x80 & 0xD7);
                assert_eq!(data[1], 0x80); // opcode
                assert_eq!(data[2], 0x01); // count = 1
                assert_eq!(data[3], 0x20); // addr
                assert_eq!(data[4], 0x01); // value
            }
            other => panic!("expected BulkOut, got {other:?}"),
        }
    }

    #[test]
    fn read_register_8_convenience() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x42]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        let val = channel.read_register_8(0x20).unwrap();
        assert_eq!(val, 0x42);
    }

    #[test]
    fn write_register_bit_sets_bit() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Queue response for read_register_8 (current value 0b0000_0100 = 0x04)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x04]);
        // Queue ACK for write_register_8
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        // Set bit 1 -> 0x04 | 0x02 = 0x06
        channel.write_register_bit(0x13, true, 1).unwrap();

        let ops = mock.operations();
        // Op 0: read (BulkOut for ReadRegisters)
        // Op 1: read response (BulkIn)
        // Op 2: write (BulkOut for WriteRegisters)
        // Op 3: write ACK (BulkIn)
        // Find the final write command
        let write_data = ops
            .iter()
            .filter_map(|op| {
                if let UsbOperation::BulkOut { data, .. } = op {
                    Some(data.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        // Second BulkOut should be the write with value 0x06
        assert_eq!(write_data.len(), 2);
        // The write payload (after control byte) is: 0x80, 0x01, 0x13, 0x06
        assert_eq!(write_data[1][1], 0x80); // opcode
        assert_eq!(write_data[1][2], 0x01); // count
        assert_eq!(write_data[1][3], 0x13); // addr
        assert_eq!(write_data[1][4], 0x06); // value (0x04 | 0x02)
    }

    #[test]
    fn write_register_bit_clears_bit() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Queue response for read_register_8 (current value 0b0000_0110 = 0x06)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x06]);
        // Queue ACK for write_register_8
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        // Clear bit 1 -> 0x06 & ~0x02 = 0x04
        channel.write_register_bit(0x13, false, 1).unwrap();

        let ops = mock.operations();
        let write_data = ops
            .iter()
            .filter_map(|op| {
                if let UsbOperation::BulkOut { data, .. } = op {
                    Some(data.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(write_data.len(), 2);
        assert_eq!(write_data[1][4], 0x04); // value (0x06 & !0x02)
    }

    #[test]
    #[should_panic(expected = "bit_position must be 0..7")]
    fn write_register_bit_panics_on_invalid_position() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut channel = mock_channel_with_seed(&mock, 0);
        channel.write_register_bit(0x13, true, 8).unwrap();
    }

    #[test]
    fn write_fpga_register_selects_8bit_path() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        // Address 0x20 fits in 8 bits -> should use opcode 0x80
        channel.write_fpga_register(0x0020, 0x01).unwrap();

        let ops = mock.operations();
        match &ops[0] {
            UsbOperation::BulkOut { data, .. } => {
                // Should be opcode 0x80 (WriteRegisters)
                assert_eq!(data[1], 0x80);
            }
            other => panic!("expected BulkOut, got {other:?}"),
        }
    }

    #[test]
    fn write_fpga_register_selects_16bit_path() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        // Address 0x01C0 > 0xFF -> should use 16-bit path
        channel.write_fpga_register(0x01C0, 0x0000_0001).unwrap();

        let ops = mock.operations();
        match &ops[0] {
            UsbOperation::BulkOut { data, .. } => {
                // 16-bit path: opcode = addr_hi = 0x01
                assert_eq!(data[1], 0x01);
                assert_eq!(data[2], 0xC0); // addr_lo
            }
            other => panic!("expected BulkOut, got {other:?}"),
        }
    }

    // -- EEPROM device ID read tests --

    #[test]
    fn read_eeprom_device_id_sends_opcode_0x88() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        let device_id: u64 = 0x764051b51bd4d9f1;
        let id_bytes = device_id.to_le_bytes();
        mock.push_bulk_in_response(endpoint::BULK_IN, id_bytes.to_vec());

        let mut channel = mock_channel_with_seed(&mock, seed);
        let result = channel.read_eeprom_device_id().unwrap();
        assert_eq!(result, device_id);

        // Verify opcode 0x88 was sent.
        let ops = mock.operations();
        match &ops[0] {
            UsbOperation::BulkOut { data, .. } => {
                // With seed=0, XorAll: data[1] = opcode
                assert_eq!(data[1], 0x88, "should send opcode 0x88");
            }
            other => panic!("expected BulkOut, got {other:?}"),
        }
    }

    #[test]
    fn read_eeprom_device_id_fails_on_short_response() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Only 4 bytes instead of 8.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x01, 0x02, 0x03, 0x04]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        let err = channel.read_eeprom_device_id().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("expected at least 8 bytes"),
            "error should mention expected bytes: {msg}"
        );
    }

    #[test]
    fn read_eeprom_device_id_parses_le_bytes() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Little-endian: first byte is LSB.
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        );

        let mut channel = mock_channel_with_seed(&mock, seed);
        let result = channel.read_eeprom_device_id().unwrap();
        assert_eq!(result, 1, "should parse little-endian");
    }

    // -- Non-Pro WriteConfigurationData tests --

    #[test]
    fn write_config_data_non_pro_uses_8bit_length() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // 10 bytes of data, chunk_size=62 -> 1 chunk.
        let data = vec![0xABu8; 10];

        let mut channel = mock_channel_with_seed(&mock, seed);
        channel.write_config_data_non_pro(&data, 62).unwrap();

        let ops = mock.operations();
        // Should be exactly 1 BulkOut, no BulkIn (no ACK).
        assert_eq!(ops.len(), 1, "non-Pro should have 1 operation (no ACK)");

        match &ops[0] {
            UsbOperation::BulkOut { data: sent, .. } => {
                // With seed=0 XorAll: control_byte + opcode + payload
                // opcode = 0x7F
                // payload = [len_8bit=10] + [data...]
                // control_byte = (0x7F & 0xD7) ^ (0x00 & 0xD7) = 0x57
                assert_eq!(sent[1], 0x7F, "opcode should be 0x7F");
                assert_eq!(sent[2], 10, "8-bit length field should be 10");
                // Data bytes follow.
                assert_eq!(&sent[3..13], &[0xABu8; 10], "data should match");
            }
            other => panic!("expected BulkOut, got {other:?}"),
        }
    }

    #[test]
    fn write_config_data_non_pro_no_ack_read() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // Send 120 bytes with chunk_size=62 -> 2 chunks.
        let data = vec![0xCDu8; 120];

        let mut channel = mock_channel_with_seed(&mock, seed);
        channel.write_config_data_non_pro(&data, 62).unwrap();

        let ops = mock.operations();
        // 2 chunks, each is a BulkOut with no BulkIn response read.
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        let in_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkIn { .. }))
            .count();
        assert_eq!(out_count, 2, "should have 2 OUT transfers");
        assert_eq!(in_count, 0, "non-Pro should have no IN transfers (no ACK)");
    }

    #[test]
    fn write_config_data_non_pro_chunks_at_62_bytes() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // 200 bytes -> ceil(200/62) = 4 chunks: 62, 62, 62, 14
        let data = vec![0xFFu8; 200];

        let mut channel = mock_channel_with_seed(&mock, seed);
        channel.write_config_data_non_pro(&data, 62).unwrap();

        let ops = mock.operations();
        assert_eq!(ops.len(), 4, "200 bytes at 62/chunk = 4 chunks");
    }

    #[test]
    #[should_panic(expected = "non-Pro chunk size must be <= 62")]
    fn write_config_data_non_pro_panics_on_large_chunk() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut channel = mock_channel_with_seed(&mock, 0);
        channel.write_config_data_non_pro(&[0u8; 10], 100).unwrap();
    }

    // -- write_config_data_for_device dispatch tests --

    #[test]
    fn write_config_data_for_device_pro_uses_chunked() {
        use saleae_proto::device::{UsbSpeed, LOGIC_PRO_8_PROFILE};

        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        let data = vec![0xABu8; 100];

        // Pro devices read ACK responses. Queue one ACK.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        channel
            .write_config_data_for_device(&data, &LOGIC_PRO_8_PROFILE, UsbSpeed::Super)
            .unwrap();

        let ops = mock.operations();
        // Pro path: 1 BulkOut + 1 BulkIn (ACK) = 2 operations.
        assert_eq!(ops.len(), 2, "Pro should have OUT + IN (ACK)");

        match &ops[0] {
            UsbOperation::BulkOut { data: sent, .. } => {
                // Pro format: opcode=0x7F, then [len_lo, len_hi, data...]
                assert_eq!(sent[1], 0x7F);
                // 16-bit length: 100 = 0x64, 0x00
                assert_eq!(sent[2], 100); // len_lo
                assert_eq!(sent[3], 0); // len_hi
            }
            other => panic!("expected BulkOut, got {other:?}"),
        }
    }

    #[test]
    fn write_config_data_for_device_non_pro_uses_non_pro_path() {
        use saleae_proto::device::{UsbSpeed, LOGIC_8_PROFILE};

        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        let data = vec![0xABu8; 50];

        // Non-Pro should not read any ACK.
        let mut channel = mock_channel_with_seed(&mock, seed);
        channel
            .write_config_data_for_device(&data, &LOGIC_8_PROFILE, UsbSpeed::High)
            .unwrap();

        let ops = mock.operations();
        // Non-Pro: 1 BulkOut, no BulkIn.
        assert_eq!(ops.len(), 1, "non-Pro should have only OUT (no ACK)");

        match &ops[0] {
            UsbOperation::BulkOut { data: sent, .. } => {
                assert_eq!(sent[1], 0x7F);
                // Non-Pro format: 8-bit length.
                assert_eq!(sent[2], 50); // 8-bit length
            }
            other => panic!("expected BulkOut, got {other:?}"),
        }
    }

    #[test]
    fn write_config_data_for_device_pro_usb2_chunk_size() {
        use saleae_proto::device::{UsbSpeed, LOGIC_PRO_8_PROFILE};

        let mock = std::sync::Arc::new(MockTransport::new());
        let seed = 0u32;

        // 1020 bytes at USB 2.0 -> chunk_size=508 -> ceil(1020/508) = 3 chunks
        // (508 + 508 + 4).
        let data = vec![0xABu8; 1020];

        // Queue 3 ACK responses.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel_with_seed(&mock, seed);
        channel
            .write_config_data_for_device(&data, &LOGIC_PRO_8_PROFILE, UsbSpeed::High)
            .unwrap();

        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        assert_eq!(
            out_count, 3,
            "1020 bytes at USB 2.0 Pro (508/chunk) = 3 chunks (508+508+4)"
        );
    }
}
