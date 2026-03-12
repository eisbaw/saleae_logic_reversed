//! 8-phase device initialization orchestrator.
//!
//! This module orchestrates the full Logic Pro 8 initialization sequence,
//! delegating to the canonical implementations in each subsystem:
//!
//! - **Phase 2** (FPGA upload): delegates to [`fpga::program_fpga`]
//! - **Phase 3** (post-FPGA init): delegates to [`adc::init_ad9637`]
//! - **Phase 7** (capture start): delegates to [`adc::power_on_for_capture`]
//!   and [`capture::configure_capture`]
//! - **Phase 8** (teardown): delegates to [`adc::power_down`]
//!
//! The overall sequence matches the protocol captured in
//! `re/notes/init-sequence-pcap.md` (928 EP1 frames, 0 unknown opcodes).
//!
//! # Phase summary
//!
//! | Phase | Name | Description |
//! |-------|------|-------------|
//! | 1 | Handshake | Reseed, Ping, GetDeviceInfo, ReadVersion, ResetFpga |
//! | 2 | FPGA Upload | 334 bitstream chunks (~340 KB) |
//! | 3 | Post-FPGA Init | ADC init, JESD link, calibration coefficients |
//! | 4 | Channel Config | Reseed #2, digital channel enable registers |
//! | 5 | Updated Config | Reseed #3, updated channel config (user change) |
//! | 6 | EEPROM Calibration | Reseed #4, I2C reads from EEPROM at 0xC0 |
//! | 7 | Capture Start | ADC re-init, JESD re-establish, capture params, StartSampling |
//! | 8 | Teardown | StopStream, power down registers |
//!
//! # Reseed timing
//!
//! The pcap shows 4 reseeds at phase boundaries:
//! - Reseed #1: start of Phase 1 (initial connection)
//! - Reseed #2: start of Phase 4 (after FPGA init complete)
//! - Reseed #3: start of Phase 5 (user changed channel config)
//! - Reseed #4: start of Phase 6 (before EEPROM calibration)
//!
//! Reseed packets are always encrypted with the initial/default seed
//! (0x354b248e), not the rolling LFSR state. This is handled correctly
//! by `SimpleEncryption::encrypt_reseed`.

use saleae_proto::device::{DeviceProfile, UsbSpeed};
use saleae_proto::fpga_regs;
use tracing::{debug, info};

use crate::capture::CaptureConfig;
use crate::ep1::Ep1CommandChannel;
use crate::error::{DriverError, DriverResult};

// ---------------------------------------------------------------------------
// Phase tracking
// ---------------------------------------------------------------------------

/// The 8 phases of device initialization, matching the pcap sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InitPhase {
    /// Before any communication.
    NotStarted,
    /// Phase 1: Reseed + Ping + GetDeviceInfo + ReadVersion + ResetFpga.
    Handshake,
    /// Phase 2: FPGA bitstream upload (334 chunks, ~340 KB).
    FpgaUpload,
    /// Phase 3: Post-FPGA register init (ADC, JESD, calibration coefficients).
    PostFpgaInit,
    /// Phase 4: Reseed #2 + digital channel configuration.
    ChannelConfig,
    /// Phase 5: Reseed #3 + updated channel configuration.
    UpdatedConfig,
    /// Phase 6: Reseed #4 + EEPROM calibration data reads.
    EepromCalibration,
    /// Phase 7: ADC re-init, JESD re-establish, capture parameters, start.
    CaptureStart,
    /// Phase 8: Stop stream, power down.
    Teardown,
}

impl std::fmt::Display for InitPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStarted => write!(f, "NotStarted"),
            Self::Handshake => write!(f, "Phase 1: Handshake"),
            Self::FpgaUpload => write!(f, "Phase 2: FPGA Upload"),
            Self::PostFpgaInit => write!(f, "Phase 3: Post-FPGA Init"),
            Self::ChannelConfig => write!(f, "Phase 4: Channel Config"),
            Self::UpdatedConfig => write!(f, "Phase 5: Updated Config"),
            Self::EepromCalibration => write!(f, "Phase 6: EEPROM Calibration"),
            Self::CaptureStart => write!(f, "Phase 7: Capture Start"),
            Self::Teardown => write!(f, "Phase 8: Teardown"),
        }
    }
}

// ---------------------------------------------------------------------------
// Channel configuration
// ---------------------------------------------------------------------------

/// Digital channel enable configuration.
///
/// Registers 0x0f, 0x10, 0x11 control which digital channels are enabled.
/// From pcap Phase 4-5:
/// - r0x0f = digital channel enable low
/// - r0x10 = digital channel enable mid
/// - r0x11 = digital channel enable high
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelConfig {
    /// Register 0x0f value (digital channel enable low).
    pub digital_enable_low: u8,
    /// Register 0x10 value (digital channel enable mid).
    pub digital_enable_mid: u8,
    /// Register 0x11 value (digital channel enable high).
    pub digital_enable_high: u8,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            digital_enable_low: 0x0F,
            digital_enable_mid: 0x0F,
            digital_enable_high: 0x0F,
        }
    }
}

// CaptureParams removed -- use capture::CaptureConfig instead, which
// computes register values from high-level parameters (sample rate, channels).
// The old CaptureParams was a literal pcap replay (hardcoded register values)
// that could not adapt to different sample rates or channel configurations.

// ---------------------------------------------------------------------------
// Init result
// ---------------------------------------------------------------------------

/// Result of the handshake phase (Phase 1).
#[derive(Debug, Clone)]
pub struct HandshakeResult {
    /// Device info string (e.g. "Logic Pro Firmware Compiled Oct 25 2022 16:42:00").
    pub device_info: String,
    /// Raw version data from first ReadVersion call.
    pub version_data: Vec<u8>,
    /// Raw version string from second ReadVersion call (e.g. "REV.").
    pub version_string: Vec<u8>,
}

/// Result of the full initialization (Phases 1-3).
#[derive(Debug, Clone)]
pub struct InitResult {
    /// Phase 1 handshake result.
    pub handshake: HandshakeResult,
    /// Calibration coefficients read from ReadVersion in Phase 3d.
    pub calibration_data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// DeviceSession: the 8-phase orchestrator
// ---------------------------------------------------------------------------

/// Orchestrates the pcap-validated 8-phase initialization protocol.
///
/// Tracks which phase has been completed and manages reseed timing.
/// Each phase method validates that the prerequisite phase has been
/// completed before proceeding.
///
/// # Usage
///
/// ```ignore
/// let mut session = DeviceSession::new(ep1, profile, usb_speed);
///
/// // Phases 1-3: Full device initialization
/// let result = session.initialize(bitstream)?;
///
/// // Phase 4: Configure channels
/// session.configure_channels(channel_config)?;
///
/// // Phase 5: Update configuration (optional, only if user changes settings)
/// session.update_config(new_channel_config)?;
///
/// // Phase 6: Read EEPROM calibration
/// let cal_data = session.read_eeprom_calibration(&cal_params)?;
///
/// // Phase 7: Start capture
/// session.start_capture(capture_params)?;
///
/// // Phase 8: Stop capture
/// session.stop_capture()?;
/// ```
pub struct DeviceSession<'a> {
    ep1: &'a mut Ep1CommandChannel,
    profile: DeviceProfile,
    usb_speed: UsbSpeed,
    phase: InitPhase,
}

impl<'a> DeviceSession<'a> {
    /// Create a new device session.
    ///
    /// The EP1 channel should already be constructed (with transport and
    /// encryption mode set) but NOT yet initialized -- Phase 1 will
    /// perform the first reseed + ping handshake.
    pub fn new(
        ep1: &'a mut Ep1CommandChannel,
        profile: DeviceProfile,
        usb_speed: UsbSpeed,
    ) -> Self {
        Self {
            ep1,
            profile,
            usb_speed,
            phase: InitPhase::NotStarted,
        }
    }

    /// Return the current phase.
    pub fn phase(&self) -> InitPhase {
        self.phase
    }

    /// Return a reference to the EP1 channel (for direct access if needed).
    pub fn ep1(&mut self) -> &mut Ep1CommandChannel {
        self.ep1
    }

    // -------------------------------------------------------------------
    // Phase 1: Handshake
    // -------------------------------------------------------------------

    /// Execute Phase 1: Initial handshake.
    ///
    /// Sequence (pcap frames 15-36):
    /// 1. Reseed (establish encryption with random seed)
    /// 2. Ping (verify 0xAA response)
    /// 3. GetDeviceInfo (read device identification string)
    /// 4. ReadVersion x2 (read firmware/hardware version)
    /// 5. ResetFpga (prepare for bitstream upload)
    pub fn handshake(&mut self) -> DriverResult<HandshakeResult> {
        self.require_phase(InitPhase::NotStarted, "handshake")?;

        info!("Phase 1: Handshake");

        // Step 1-2: Reseed + Ping (handled by ep1.initialize())
        self.ep1.initialize()?;

        // Step 3: GetDeviceInfo
        let device_info = self.ep1.get_device_info()?;
        info!(device_info = %device_info, "Device info");

        // Step 4: ReadVersion (first call -- firmware version)
        let version_data = self
            .ep1
            .read_version(&[0x33, 0x81, 0x08, 0x00, 0x08, 0x00])?;
        debug!(len = version_data.len(), "ReadVersion #1");

        // Step 4b: ReadVersion (second call -- hardware revision)
        let version_string = self
            .ep1
            .read_version(&[0x33, 0x81, 0x20, 0x00, 0x06, 0x00])?;
        debug!(len = version_string.len(), "ReadVersion #2");

        // Step 5: ResetFpga
        self.ep1.reset_fpga()?;

        self.phase = InitPhase::Handshake;

        let result = HandshakeResult {
            device_info,
            version_data,
            version_string,
        };

        info!("Phase 1 complete: handshake successful");
        Ok(result)
    }

    // -------------------------------------------------------------------
    // Phase 2: FPGA Upload
    // -------------------------------------------------------------------

    /// Execute Phase 2: FPGA bitstream upload.
    ///
    /// Delegates to [`fpga::program_fpga`], which performs the complete
    /// hardware-validated sequence: ResetFpga, chunked bitstream upload,
    /// and r0x7F write/readback verification.
    ///
    /// Note: Phase 1 already sent ResetFpga, and `program_fpga` sends it
    /// again. The double reset is harmless (idempotent) and keeps the
    /// delegation clean -- `program_fpga` is the canonical FPGA programming
    /// entry point.
    pub fn upload_fpga(&mut self, bitstream: &[u8]) -> DriverResult<()> {
        self.require_phase(InitPhase::Handshake, "upload_fpga")?;

        info!(
            bitstream_len = bitstream.len(),
            "Phase 2: FPGA bitstream upload"
        );

        crate::fpga::program_fpga(self.ep1, bitstream, &self.profile, self.usb_speed)?;

        self.phase = InitPhase::FpgaUpload;
        info!("Phase 2 complete: FPGA bitstream uploaded and verified");
        Ok(())
    }

    // -------------------------------------------------------------------
    // Phase 3: Post-FPGA Init
    // -------------------------------------------------------------------

    /// Execute Phase 3: Post-FPGA register initialization.
    ///
    /// Delegates to [`adc::init_ad9637`], which is the hardware-validated
    /// Xilinx Spartan-6 / AD9637 init sequence. This replaces the previous
    /// inline pcap replay that duplicated the same logic.
    ///
    /// The sequence (now encapsulated in `adc::init_ad9637`):
    /// - ADC register configuration (reset, output modes, DCO, VREF)
    /// - ADC power off
    /// - SetAd9637Enabled(false) -- SPI commit + DCM cascade
    /// - SetAd9637Enabled(true) -- enable, reset cycle, DCM lock + bitslip
    /// - Calibration coefficient writes + post-cal cascade
    ///
    /// # Note on FPGA verification
    ///
    /// FPGA r0x7F readback verification is handled by [`fpga::program_fpga`]
    /// in Phase 2, so it is not repeated here.
    pub fn post_fpga_init(&mut self) -> DriverResult<Vec<u8>> {
        self.require_phase(InitPhase::FpgaUpload, "post_fpga_init")?;

        info!("Phase 3: Post-FPGA register initialization");

        // Read calibration coefficients from EEPROM (offset 0x10, 16 bytes).
        let calibration_data = self
            .ep1
            .read_version(&[0x33, 0x81, 0x10, 0x00, 0x10, 0x00])?;
        debug!(len = calibration_data.len(), "ReadVersion calibration data");

        // Delegate to the hardware-validated AD9637 init sequence.
        let dcm_result = crate::adc::init_ad9637(self.ep1, &calibration_data)?;
        info!(
            locked = dcm_result.locked,
            status_40 = format_args!("{:#04x}", dcm_result.status_40),
            "AD9637 init complete"
        );

        self.phase = InitPhase::PostFpgaInit;
        info!("Phase 3 complete: post-FPGA init done");
        Ok(calibration_data)
    }

    /// Execute Phases 1-3 in sequence (convenience method).
    ///
    /// Returns the handshake result and calibration data.
    pub fn initialize(&mut self, bitstream: &[u8]) -> DriverResult<InitResult> {
        let handshake = self.handshake()?;
        self.upload_fpga(bitstream)?;
        let calibration_data = self.post_fpga_init()?;

        Ok(InitResult {
            handshake,
            calibration_data,
        })
    }

    // -------------------------------------------------------------------
    // Phase 4: Channel Config
    // -------------------------------------------------------------------

    /// Execute Phase 4: Channel configuration.
    ///
    /// Sequence (pcap frames 1627-1636):
    /// 1. Reseed #2
    /// 2. Ping
    /// 3. WriteRegisters: r0x0f, r0x10, r0x11 (digital channel enables)
    pub fn configure_channels(&mut self, config: &ChannelConfig) -> DriverResult<()> {
        self.require_phase_at_least(InitPhase::PostFpgaInit, "configure_channels")?;

        info!("Phase 4: Channel configuration");

        // Step 1-2: Reseed + Ping
        self.ep1.initialize()?;

        // Step 3: Write channel enable registers
        self.ep1.write_registers(&[
            (0x0F, config.digital_enable_low),
            (0x10, config.digital_enable_mid),
            (0x11, config.digital_enable_high),
        ])?;

        self.phase = InitPhase::ChannelConfig;
        info!("Phase 4 complete: channels configured");
        Ok(())
    }

    // -------------------------------------------------------------------
    // Phase 5: Updated Config
    // -------------------------------------------------------------------

    /// Execute Phase 5: Updated channel configuration.
    ///
    /// This is the same as Phase 4 but with different register values.
    /// In the pcap, this happened when the user changed settings in the
    /// Saleae GUI (21.4s gap from Phase 4).
    ///
    /// Sequence (pcap frames 1639-1648):
    /// 1. Reseed #3
    /// 2. Ping
    /// 3. WriteRegisters: r0x0f, r0x10, r0x11 (updated values)
    pub fn update_config(&mut self, config: &ChannelConfig) -> DriverResult<()> {
        self.require_phase_at_least(InitPhase::ChannelConfig, "update_config")?;

        info!("Phase 5: Updated channel configuration");

        // Step 1-2: Reseed + Ping
        self.ep1.initialize()?;

        // Step 3: Write updated channel enable registers
        self.ep1.write_registers(&[
            (0x0F, config.digital_enable_low),
            (0x10, config.digital_enable_mid),
            (0x11, config.digital_enable_high),
        ])?;

        self.phase = InitPhase::UpdatedConfig;
        info!("Phase 5 complete: config updated");
        Ok(())
    }

    // -------------------------------------------------------------------
    // Phase 6: EEPROM Calibration
    // -------------------------------------------------------------------

    /// Execute Phase 6: EEPROM calibration data read.
    ///
    /// Sequence (pcap frames 1651-1752):
    /// 1. Reseed #4
    /// 2. Ping
    /// 3. Device2_1 (configure EEPROM access)
    /// 4. Full EEPROM calibration read via I2C at address 0xC0:
    ///    - Read device ID (5 bytes from register 0x04)
    ///    - Read 35-byte calibration block (with polling)
    ///    - Write 26-byte calibration parameters (register 0x1C)
    ///    - Read 67-byte calibration block (with polling)
    ///
    /// # Arguments
    ///
    /// - `cal_params`: Calibration parameters to write back to the device
    ///   via EEPROM register 0x1C. Typically 26 bytes derived from the
    ///   server-hosted calibration data. If empty, the write step is
    ///   still performed (with empty data) to match the pcap sequence.
    ///
    /// Returns the parsed [`EepromCalibration`] containing all read data.
    pub fn read_eeprom_calibration(
        &mut self,
        cal_params: &[u8],
    ) -> DriverResult<crate::eeprom::EepromCalibration> {
        self.require_phase_at_least(InitPhase::ChannelConfig, "read_eeprom_calibration")?;

        info!("Phase 6: EEPROM calibration data read");

        // Step 1-2: Reseed + Ping
        self.ep1.initialize()?;

        // Step 3: Device2_1 (configure EEPROM access)
        let _resp = self.ep1.device2_1()?;

        // Step 4: Full EEPROM calibration read sequence.
        let calibration = crate::eeprom::eeprom_read_full_calibration(self.ep1, cal_params)?;

        self.phase = InitPhase::EepromCalibration;
        info!(
            device_id = format_args!("{:02x?}", &calibration.device_id),
            block_35_len = calibration.block_35.len(),
            block_67_len = calibration.block_67.len(),
            "Phase 6 complete: EEPROM calibration read"
        );
        Ok(calibration)
    }

    // -------------------------------------------------------------------
    // Phase 7: Capture Start
    // -------------------------------------------------------------------

    /// Execute Phase 7: Start capture.
    ///
    /// Delegates to [`adc::power_on_for_capture`] for ADC re-init and
    /// [`capture::configure_capture`] for register computation, write,
    /// and readback verification. This replaces the previous inline
    /// pcap replay that used hardcoded register values.
    ///
    /// Sequence:
    /// 1. ADC power-on + DCM re-lock (`adc::power_on_for_capture`)
    /// 2. Compute + write + verify capture registers (`capture::configure_capture`)
    /// 3. StartSampling (0x01)
    /// 4. Trigger capture (r0x00=0x01)
    pub fn start_capture(&mut self, config: &CaptureConfig) -> DriverResult<()> {
        self.require_phase_at_least(InitPhase::ChannelConfig, "start_capture")?;

        info!("Phase 7: Capture start");

        // Step 1: ADC re-initialization and DCM lock.
        let dcm_result = crate::adc::power_on_for_capture(self.ep1)?;
        debug!(
            locked = dcm_result.locked,
            status_40 = format_args!("{:#04x}", dcm_result.status_40),
            "ADC powered on for capture"
        );

        // Step 2: Write capture registers (computed from config) + readback verify.
        crate::capture::configure_capture(self.ep1, config)?;

        // Step 3: StartSampling.
        self.ep1.start_sampling()?;

        // Step 4: Trigger capture (r0x00=0x01).
        self.ep1
            .write_register_8(fpga_regs::FEAT_CAPTURE_INIT, 0x01)?;

        self.phase = InitPhase::CaptureStart;
        info!("Phase 7 complete: capture started");
        Ok(())
    }

    // -------------------------------------------------------------------
    // Phase 8: Teardown
    // -------------------------------------------------------------------

    /// Execute Phase 8: Capture teardown.
    ///
    /// Sequence:
    /// 1. Write r0x00=0x00 (stop capture trigger)
    /// 2. StopStream (0x02)
    /// 3. Power down ADC (`adc::power_down`)
    pub fn stop_capture(&mut self) -> DriverResult<()> {
        self.require_phase_at_least(InitPhase::CaptureStart, "stop_capture")?;

        info!("Phase 8: Capture teardown");

        // Step 1: Stop capture trigger.
        self.ep1
            .write_register_8(fpga_regs::FEAT_CAPTURE_INIT, 0x00)?;

        // Step 2: StopStream.
        self.ep1.stop_stream()?;

        // Step 3: Power down ADC.
        crate::adc::power_down(self.ep1)?;

        self.phase = InitPhase::Teardown;
        info!("Phase 8 complete: capture torn down");
        Ok(())
    }

    // -------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------

    /// Require that the current phase is exactly `expected`.
    fn require_phase(&self, expected: InitPhase, operation: &str) -> DriverResult<()> {
        if self.phase != expected {
            return Err(DriverError::InvalidState {
                current: format!("{}", self.phase),
                required: format!("{expected} (for {operation})"),
            });
        }
        Ok(())
    }

    /// Require that the current phase is at least `minimum`.
    fn require_phase_at_least(&self, minimum: InitPhase, operation: &str) -> DriverResult<()> {
        if self.phase < minimum {
            return Err(DriverError::InvalidState {
                current: format!("{}", self.phase),
                required: format!("{minimum} or later (for {operation})"),
            });
        }
        Ok(())
    }

    // verify_fpga_id and adc_init removed -- now handled by
    // fpga::program_fpga (FPGA verify) and adc::init_ad9637 (ADC init).

    /// Test-only: set phase directly to skip expensive init sequences.
    #[cfg(test)]
    fn set_phase(&mut self, phase: InitPhase) {
        self.phase = phase;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_proto::device::{endpoint, LOGIC_PRO_8_PROFILE};
    use saleae_proto::obfuscation::EncryptionMode;
    use saleae_usb::mock::{MockTransport, UsbOperation};

    /// Create a mock EP1 channel in EncryptionMode::None for easier testing.
    /// In None mode, no control byte is added and no XOR is applied.
    fn mock_channel_none(mock: &std::sync::Arc<MockTransport>) -> Ep1CommandChannel {
        let transport: Box<dyn saleae_usb::transport::UsbTransport> = Box::new(mock.clone());
        Ep1CommandChannel::new(transport, EncryptionMode::None, true)
    }

    /// Queue the responses needed for Phase 1 (Handshake) in None mode.
    fn queue_phase1_responses(mock: &MockTransport) {
        // Reseed: no response needed (just bulk OUT)
        // Ping response: 0xAA
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);
        // GetDeviceInfo response
        mock.push_bulk_in_response(endpoint::BULK_IN, b"Logic Pro Firmware\0".to_vec());
        // ReadVersion #1 response (8 bytes)
        mock.push_bulk_in_response(
            endpoint::BULK_IN,
            vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08],
        );
        // ReadVersion #2 response ("REV." + version)
        mock.push_bulk_in_response(endpoint::BULK_IN, b"REV.1\0".to_vec());
        // ResetFpga response
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    }

    /// Queue responses for Phase 2 (FPGA Upload via `fpga::program_fpga`).
    ///
    /// `program_fpga` does: ResetFpga + chunked upload + 2x WriteRegisters(r0x7F=0xAA) +
    /// ReadRegisters(r0x7F) -> 0xAA.
    fn queue_phase2_responses(mock: &MockTransport, num_chunks: usize) {
        // ResetFpga ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // WriteConfigurationData ACKs (one per chunk)
        for _ in 0..num_chunks {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }
        // WriteRegisters r0x7F=0xAA (twice) -> 2 ACKs
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // ReadRegisters r0x7F -> 0xAA
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);
    }

    /// Queue responses for Phase 4 or 5 (Channel Config).
    fn queue_channel_config_responses(mock: &MockTransport) {
        // Reseed: no response (bulk OUT only in None mode)
        // Ping response
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);
        // WriteRegisters (3 pairs) -> ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
    }

    // -------------------------------------------------------------------
    // Phase tracking tests
    // -------------------------------------------------------------------

    #[test]
    fn session_starts_at_not_started() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut ep1 = mock_channel_none(&mock);
        let session = DeviceSession::new(&mut ep1, LOGIC_PRO_8_PROFILE.clone(), UsbSpeed::Super);
        assert_eq!(session.phase(), InitPhase::NotStarted);
    }

    #[test]
    fn phase_ordering_is_correct() {
        assert!(InitPhase::NotStarted < InitPhase::Handshake);
        assert!(InitPhase::Handshake < InitPhase::FpgaUpload);
        assert!(InitPhase::FpgaUpload < InitPhase::PostFpgaInit);
        assert!(InitPhase::PostFpgaInit < InitPhase::ChannelConfig);
        assert!(InitPhase::ChannelConfig < InitPhase::UpdatedConfig);
        assert!(InitPhase::UpdatedConfig < InitPhase::EepromCalibration);
        assert!(InitPhase::EepromCalibration < InitPhase::CaptureStart);
        assert!(InitPhase::CaptureStart < InitPhase::Teardown);
    }

    // -------------------------------------------------------------------
    // Phase 1: Handshake
    // -------------------------------------------------------------------

    #[test]
    fn handshake_executes_correct_sequence() {
        let mock = std::sync::Arc::new(MockTransport::new());
        queue_phase1_responses(&mock);

        let mut ep1 = mock_channel_none(&mock);
        let mut session =
            DeviceSession::new(&mut ep1, LOGIC_PRO_8_PROFILE.clone(), UsbSpeed::Super);

        let result = session.handshake().unwrap();
        assert_eq!(session.phase(), InitPhase::Handshake);
        assert!(result.device_info.contains("Logic Pro"));
        assert_eq!(result.version_data.len(), 8);

        // Verify command sequence from mock operations.
        let ops = mock.operations();
        let out_ops: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                UsbOperation::BulkOut { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect();

        // In None mode:
        // 0: Reseed (4 bytes raw seed)
        // 1: Ping [0x7D, 0x55]
        // 2: GetDeviceInfo [0x8B]
        // 3: ReadVersion [0x07, ...]
        // 4: ReadVersion [0x07, ...]
        // 5: ResetFpga [0x7E]
        assert_eq!(out_ops.len(), 6, "expected 6 OUT transfers in Phase 1");

        // Verify command opcodes (in None mode, first byte is opcode)
        assert_eq!(out_ops[1], vec![0x7D, 0x55], "Ping command");
        assert_eq!(out_ops[2], vec![0x8B], "GetDeviceInfo command");
        assert_eq!(out_ops[3][0], 0x07, "ReadVersion #1");
        assert_eq!(out_ops[4][0], 0x07, "ReadVersion #2");
        assert_eq!(out_ops[5], vec![0x7E], "ResetFpga command");
    }

    #[test]
    fn handshake_rejects_double_call() {
        let mock = std::sync::Arc::new(MockTransport::new());
        queue_phase1_responses(&mock);
        queue_phase1_responses(&mock);

        let mut ep1 = mock_channel_none(&mock);
        let mut session =
            DeviceSession::new(&mut ep1, LOGIC_PRO_8_PROFILE.clone(), UsbSpeed::Super);

        session.handshake().unwrap();
        let err = session.handshake().unwrap_err();
        assert!(err.to_string().contains("NotStarted"));
    }

    // -------------------------------------------------------------------
    // Phase 2: FPGA Upload (now delegates to fpga::program_fpga)
    // -------------------------------------------------------------------

    #[test]
    fn upload_fpga_requires_handshake() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut ep1 = mock_channel_none(&mock);
        let mut session =
            DeviceSession::new(&mut ep1, LOGIC_PRO_8_PROFILE.clone(), UsbSpeed::Super);

        let err = session.upload_fpga(&[0xFF; 100]).unwrap_err();
        assert!(err.to_string().contains("Handshake"));
    }

    #[test]
    fn upload_fpga_advances_phase() {
        let mock = std::sync::Arc::new(MockTransport::new());
        queue_phase1_responses(&mock);
        // program_fpga: ResetFpga + 1 chunk + 2x WriteRegisters + 1 ReadRegisters
        queue_phase2_responses(&mock, 1);

        let mut ep1 = mock_channel_none(&mock);
        let mut session =
            DeviceSession::new(&mut ep1, LOGIC_PRO_8_PROFILE.clone(), UsbSpeed::Super);

        session.handshake().unwrap();
        session.upload_fpga(&[0xFF; 100]).unwrap();
        assert_eq!(session.phase(), InitPhase::FpgaUpload);
    }

    // -------------------------------------------------------------------
    // Phase 4: Channel Config
    // -------------------------------------------------------------------

    #[test]
    fn configure_channels_requires_post_fpga_init() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut ep1 = mock_channel_none(&mock);
        let mut session =
            DeviceSession::new(&mut ep1, LOGIC_PRO_8_PROFILE.clone(), UsbSpeed::Super);

        let config = ChannelConfig::default();
        let err = session.configure_channels(&config).unwrap_err();
        assert!(err.to_string().contains("Post-FPGA"));
    }

    // -------------------------------------------------------------------
    // Phase 7: Capture start (now uses CaptureConfig)
    // -------------------------------------------------------------------

    #[test]
    fn start_capture_requires_channel_config() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut ep1 = mock_channel_none(&mock);
        let mut session =
            DeviceSession::new(&mut ep1, LOGIC_PRO_8_PROFILE.clone(), UsbSpeed::Super);

        let config = CaptureConfig::default();
        let err = session.start_capture(&config).unwrap_err();
        assert!(err.to_string().contains("Channel Config"));
    }

    // -------------------------------------------------------------------
    // Phase 8: Teardown
    // -------------------------------------------------------------------

    #[test]
    fn stop_capture_requires_capture_start() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let mut ep1 = mock_channel_none(&mock);
        let mut session =
            DeviceSession::new(&mut ep1, LOGIC_PRO_8_PROFILE.clone(), UsbSpeed::Super);

        let err = session.stop_capture().unwrap_err();
        assert!(err.to_string().contains("Capture Start"));
    }

    // -------------------------------------------------------------------
    // Reseed timing verification
    // -------------------------------------------------------------------

    #[test]
    fn reseed_happens_at_correct_phase_boundaries() {
        // Verify that Phases 4 and 5 each perform a reseed by checking
        // that ep1.initialize() is called (which sends reseed + ping).
        //
        // Uses set_phase to skip the expensive Phase 2-3 init (which
        // involves hardware delays from adc::init_ad9637). Those phases
        // are tested independently in their own modules.
        let mock = std::sync::Arc::new(MockTransport::new());

        // Phase 4 responses (reseed + ping + write_registers)
        queue_channel_config_responses(&mock);
        // Phase 5 responses (reseed + ping + write_registers)
        queue_channel_config_responses(&mock);

        let mut ep1 = mock_channel_none(&mock);
        let mut session =
            DeviceSession::new(&mut ep1, LOGIC_PRO_8_PROFILE.clone(), UsbSpeed::Super);

        // Skip directly to PostFpgaInit (Phases 1-3 tested elsewhere).
        session.set_phase(InitPhase::PostFpgaInit);

        let config = ChannelConfig {
            digital_enable_low: 0x00,
            digital_enable_mid: 0x0F,
            digital_enable_high: 0x00,
        };
        session.configure_channels(&config).unwrap();
        assert_eq!(session.phase(), InitPhase::ChannelConfig);

        let updated = ChannelConfig {
            digital_enable_low: 0x0F,
            digital_enable_mid: 0x0F,
            digital_enable_high: 0x0F,
        };
        session.update_config(&updated).unwrap();
        assert_eq!(session.phase(), InitPhase::UpdatedConfig);

        // Count Ping commands as a proxy for reseeds: each reseed is
        // always followed by a Ping. In None mode, Ping = [0x7D, 0x55].
        let ops = mock.operations();
        let ping_count = ops
            .iter()
            .filter(|op| match op {
                UsbOperation::BulkOut { data, .. } => {
                    data.len() == 2 && data[0] == 0x7D && data[1] == 0x55
                }
                _ => false,
            })
            .count();
        // Phase 4: 1 ping (after reseed #2)
        // Phase 5: 1 ping (after reseed #3)
        assert_eq!(
            ping_count, 2,
            "expected 2 pings (one per reseed in Phases 4 and 5)"
        );
    }

    // -------------------------------------------------------------------
    // InitPhase display
    // -------------------------------------------------------------------

    #[test]
    fn init_phase_display() {
        assert_eq!(format!("{}", InitPhase::NotStarted), "NotStarted");
        assert_eq!(format!("{}", InitPhase::Handshake), "Phase 1: Handshake");
        assert_eq!(format!("{}", InitPhase::Teardown), "Phase 8: Teardown");
    }
}
