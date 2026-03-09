//! Digital capture: configure, start, stream, stop.
//!
//! This module implements the capture lifecycle for digital channels.
//! The sequence mirrors the original software's `StartSampling` flow:
//!
//! 1. Validate and build capture configuration.
//! 2. Write capture parameters to FPGA registers via EP1.
//! 3. Send configuration data blob via WriteConfigurationData (0x7F).
//! 4. Send START_READ command (0x01) via EP1.
//! 5. Read data from EP2 IN (0x82) as `DataChunk`s.
//! 6. Send StopDataPath (0x86) + STOP_STREAM (0x02) to end capture.
//!
//! # Hardware testing
//!
//! This code has NOT been validated against hardware. The device is
//! currently stuck (red LED). All testing is via mock transport only.
//!
//! # PLACEHOLDER values
//!
//! Many register values and the capture configuration blob format are
//! UNKNOWN. The code structure (sequence of operations) is based on RE
//! analysis; specific register values need USB capture confirmation.

use saleae_proto::capture_data::{self, DataChunk};
use saleae_proto::device::endpoint;
use saleae_proto::fpga_regs;
use tracing::{debug, info, warn};

use crate::ep1::Ep1CommandChannel;
use crate::error::{DriverError, DriverResult};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// EP2 IN endpoint for capture data.
pub const EP2_CAPTURE_IN: u8 = endpoint::CAPTURE_IN;

/// Default USB bulk transfer size for EP2 reads (16 x 1024 = 16 KiB).
///
/// This matches the USB 3.0 SuperSpeed bulk burst: bMaxBurst=15 means
/// up to 16 packets of 1024 bytes per service interval.
pub const DEFAULT_TRANSFER_SIZE: usize = 16 * 1024;

/// Default samples per chunk. Must be a multiple of 256 (WithStats).
///
/// PLACEHOLDER: The actual value depends on sample rate, channel count,
/// and FPGA type. 256 is the minimum for Xilinx.
pub const DEFAULT_SAMPLES_PER_CHUNK: u32 = 256;

// ---------------------------------------------------------------------------
// CaptureConfig
// ---------------------------------------------------------------------------

/// Configuration for a digital capture session.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Sample rate in Hz.
    pub sample_rate: u64,
    /// Enabled digital channel indices (0-based).
    pub digital_channels: Vec<u8>,
    /// Number of samples to capture. 0 = capture until stop.
    pub duration_samples: u64,
    /// Voltage threshold for digital logic level (volts).
    ///
    /// PLACEHOLDER: Default 1.65V (half of 3.3V LVCMOS).
    pub voltage_threshold: f64,
    /// Whether to enable per-packet stats collection.
    pub stats_enabled: bool,
    /// Samples per chunk (must be multiple of 256 when stats enabled).
    pub samples_per_chunk: u32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate: 1_000_000,
            digital_channels: vec![0, 1, 2, 3],
            duration_samples: 0,
            voltage_threshold: 1.65,
            stats_enabled: true,
            samples_per_chunk: DEFAULT_SAMPLES_PER_CHUNK,
        }
    }
}

impl CaptureConfig {
    /// Validate the capture configuration.
    pub fn validate(&self) -> DriverResult<()> {
        if self.sample_rate == 0 {
            return Err(DriverError::InvalidCaptureConfig {
                reason: "sample rate must be > 0".into(),
            });
        }

        if self.digital_channels.is_empty() {
            return Err(DriverError::InvalidCaptureConfig {
                reason: "at least one digital channel must be enabled".into(),
            });
        }

        for &ch in &self.digital_channels {
            if ch >= 16 {
                return Err(DriverError::InvalidCaptureConfig {
                    reason: format!("channel {ch} out of range (max 15)"),
                });
            }
        }

        if self.stats_enabled
            && self.samples_per_chunk
                % (capture_data::SAMPLES_PER_PACKET_MULTIPLE_WITH_STATS as u32)
                != 0
        {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "samples_per_chunk ({}) must be a multiple of {} when stats enabled",
                    self.samples_per_chunk,
                    capture_data::SAMPLES_PER_PACKET_MULTIPLE_WITH_STATS
                ),
            });
        }

        if self.samples_per_chunk < 64 {
            return Err(DriverError::InvalidCaptureConfig {
                reason: format!(
                    "samples_per_chunk ({}) must be >= 64",
                    self.samples_per_chunk
                ),
            });
        }

        Ok(())
    }

    /// Number of digital channel groups (2 or 4).
    ///
    /// The FPGA always captures in groups of 2 or 4 channels.
    fn channel_group_count(&self) -> usize {
        let max_ch = self.digital_channels.iter().max().copied().unwrap_or(0);
        if max_ch < 2 {
            2
        } else {
            4
        }
    }

    /// Bytes per sample row for this configuration (digital only).
    ///
    /// Each sample row has `channel_group_count / 8` bytes of digital
    /// data. For 2 or 4 channels captured together, one byte holds
    /// multiple sample periods.
    ///
    /// PLACEHOLDER: This is a simplification. The actual row width
    /// depends on the GPIF bus width and whether analog channels are
    /// also enabled.
    pub fn bytes_per_sample_row(&self) -> usize {
        // For digital-only: one byte holds 8/channel_count samples.
        // So bytes_per_row = 1 for both 2 and 4 channels.
        // The sample data in each chunk is: samples_per_chunk / (8/channel_count) bytes.
        1
    }

    /// Total sample data bytes per chunk.
    pub fn sample_data_bytes_per_chunk(&self) -> usize {
        // Each byte holds (8 / channel_group_count) samples.
        let samples_per_byte = 8 / self.channel_group_count();
        let byte_count = (self.samples_per_chunk as usize + samples_per_byte - 1) / samples_per_byte;
        byte_count
    }
}

// ---------------------------------------------------------------------------
// Capture sequence: configure FPGA
// ---------------------------------------------------------------------------

/// Write capture configuration to FPGA registers.
///
/// This programs the FPGA with the capture parameters before starting:
/// - Digital channel enables (registers 0x0170+)
/// - Voltage threshold (register 0x0104)
/// - Capture control (register 0x0040)
/// - Configuration commit (register 0x01C0)
///
/// # PLACEHOLDER
///
/// The exact register values for sample rate configuration are UNKNOWN.
/// The capture configuration data blob format sent via
/// WriteConfigurationData (0x7F) is also UNKNOWN. This function writes
/// reasonable placeholder values that will need USB capture validation.
pub fn configure_capture(
    ep1: &mut Ep1CommandChannel,
    config: &CaptureConfig,
) -> DriverResult<()> {
    config.validate()?;

    info!(
        sample_rate = config.sample_rate,
        channels = ?config.digital_channels,
        samples_per_chunk = config.samples_per_chunk,
        "Configuring digital capture"
    );

    // Step 1: Set voltage threshold.
    //
    // PLACEHOLDER: The threshold register (0x0104) takes a value that
    // maps voltage to a DAC code. The exact mapping is unknown.
    // For now, write a placeholder value.
    let threshold_code = voltage_to_threshold_code(config.voltage_threshold);
    debug!(
        voltage = config.voltage_threshold,
        code = format_args!("{threshold_code:#010x}"),
        "Setting digital voltage threshold"
    );
    ep1.write_fpga_register(fpga_regs::REG_DIGITAL_THRESHOLD, threshold_code)?;

    // Step 2: Configure per-channel digital enables.
    //
    // PLACEHOLDER: Each channel has a config register at 0x0170 + ch*4.
    // The value format is unknown. Writing 0x01 = enabled, 0x00 = disabled
    // is a reasonable guess.
    let group_count = config.channel_group_count();
    for ch in 0..group_count as u8 {
        let addr = fpga_regs::digital_ch_config_addr(ch);
        let enabled = config.digital_channels.contains(&ch);
        let value: u32 = if enabled { 0x01 } else { 0x00 };
        debug!(
            channel = ch,
            addr = format_args!("{addr:#06x}"),
            enabled,
            "Setting digital channel config"
        );
        ep1.write_fpga_register(addr, value)?;
    }

    // Step 3: Write capture configuration data blob via 0x7F.
    //
    // PLACEHOLDER: The capture configuration blob is sent via
    // WriteConfigurationData. Its exact format is unknown. The blob
    // likely contains sample rate dividers, channel masks, trigger
    // config, etc. For now, we send a minimal placeholder.
    let config_blob = build_capture_config_blob(config);
    if !config_blob.is_empty() {
        debug!(
            blob_len = config_blob.len(),
            "Sending capture configuration data"
        );
        ep1.write_config_data(&config_blob)?;
    }

    // Step 4: Commit configuration.
    debug!("Committing capture configuration");
    ep1.write_fpga_register(fpga_regs::REG_COMMIT_CONTROL, 0x0000_0001)?;

    info!("Capture configuration complete");
    Ok(())
}

/// Convert voltage threshold to DAC code.
///
/// PLACEHOLDER: The exact mapping is unknown. This is a linear
/// approximation assuming 0-3.3V range mapped to 0-255.
fn voltage_to_threshold_code(voltage: f64) -> u32 {
    let clamped = voltage.clamp(0.0, 3.3);
    let code = ((clamped / 3.3) * 255.0) as u32;
    code
}

/// Build the capture configuration data blob.
///
/// PLACEHOLDER: The blob format is entirely unknown. This returns
/// a minimal 8-byte placeholder with sample rate and channel count.
fn build_capture_config_blob(config: &CaptureConfig) -> Vec<u8> {
    // PLACEHOLDER: minimal config blob.
    // Actual format needs Ghidra decompilation of SetCaptureParameters.
    let mut blob = Vec::with_capacity(8);
    // Bytes 0-3: sample rate as u32 LE (truncated from u64)
    blob.extend_from_slice(&(config.sample_rate as u32).to_le_bytes());
    // Bytes 4-7: channel mask as u32 LE
    let mut mask: u32 = 0;
    for &ch in &config.digital_channels {
        mask |= 1 << ch;
    }
    blob.extend_from_slice(&mask.to_le_bytes());
    blob
}

// ---------------------------------------------------------------------------
// Capture stream
// ---------------------------------------------------------------------------

/// Handle for an active capture stream.
///
/// Created by [`start_digital_capture`]. Reads data chunks from EP2 IN
/// and provides parsed `DataChunk`s. Must be [`stop`](CaptureStream::stop)ped
/// when done.
pub struct CaptureStream {
    /// Read buffer for EP2 IN bulk transfers.
    read_buf: Vec<u8>,
    /// Whether a start packet has been received for each channel.
    channel_started: [bool; capture_data::MAX_DIGITAL_CHANNELS],
    /// Total samples read so far.
    total_samples: u64,
    /// Target sample count (0 = unlimited).
    target_samples: u64,
    /// Samples per chunk.
    samples_per_chunk: u32,
    /// Sample data bytes per chunk.
    sample_data_bytes: usize,
    /// Whether stats are enabled.
    stats_enabled: bool,
    /// Whether the stream has been stopped.
    stopped: bool,
}

impl CaptureStream {
    fn new(config: &CaptureConfig) -> Self {
        Self {
            read_buf: vec![0u8; DEFAULT_TRANSFER_SIZE],
            channel_started: [false; capture_data::MAX_DIGITAL_CHANNELS],
            total_samples: 0,
            target_samples: config.duration_samples,
            samples_per_chunk: config.samples_per_chunk,
            sample_data_bytes: config.sample_data_bytes_per_chunk(),
            stats_enabled: config.stats_enabled,
            stopped: false,
        }
    }

    /// Read the next data chunk from EP2 IN.
    ///
    /// Returns `None` when the target sample count is reached or the
    /// stream has been stopped.
    ///
    /// # Arguments
    ///
    /// - `transport`: USB transport for EP2 IN reads.
    pub fn read_chunk(
        &mut self,
        transport: &dyn saleae_usb::transport::UsbTransport,
    ) -> DriverResult<Option<DataChunk>> {
        if self.stopped {
            return Ok(None);
        }

        if self.target_samples > 0 && self.total_samples >= self.target_samples {
            return Ok(None);
        }

        // Read from EP2 IN.
        let n = transport
            .bulk_transfer_in(EP2_CAPTURE_IN, &mut self.read_buf)
            .map_err(DriverError::Usb)?;

        if n == 0 {
            debug!("EP2 IN: zero-length read");
            return Ok(None);
        }

        let data = &self.read_buf[..n];

        // Parse chunk.
        let (chunk, _consumed) =
            capture_data::parse_data_chunk(data, self.sample_data_bytes, self.stats_enabled)
                .map_err(|e| DriverError::CommandFailed {
                    command: "ParseDataChunk",
                    detail: e.to_string(),
                })?;

        // Track start packets.
        let ch = chunk.header.channel as usize;
        if chunk.header.is_start {
            if ch < self.channel_started.len() {
                self.channel_started[ch] = true;
                debug!(
                    channel = ch,
                    config_id = chunk.header.config_id,
                    "Got start packet"
                );
            }
        } else if ch < self.channel_started.len() && !self.channel_started[ch] {
            warn!(
                channel = ch,
                "Received digital data without start packet"
            );
        }

        // Count samples.
        if !chunk.header.is_start {
            let sample_count = chunk.sample_data.len() as u64 * 8
                / (self.samples_per_chunk as u64 / self.sample_data_bytes as u64).max(1);
            self.total_samples += sample_count;
        }

        Ok(Some(chunk))
    }

    /// Stop the capture stream.
    ///
    /// Sends StopDataPath (0x86) and STOP_STREAM (0x02) commands,
    /// then drains any remaining EP2 data.
    pub fn stop(&mut self, ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
        if self.stopped {
            return Ok(());
        }

        info!(
            total_samples = self.total_samples,
            "Stopping capture stream"
        );

        // Step 1: Stop data path (opcode 0x86).
        debug!("Sending StopDataPath (0x86)");
        ep1.stop_data_path()?;

        // Step 2: Stop stream (opcode 0x02).
        debug!("Sending StopStream (0x02)");
        ep1.stop_stream()?;

        self.stopped = true;
        info!("Capture stream stopped");
        Ok(())
    }

    /// Whether the stream has been stopped.
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// Total samples read so far.
    pub fn total_samples(&self) -> u64 {
        self.total_samples
    }
}

// ---------------------------------------------------------------------------
// Start capture
// ---------------------------------------------------------------------------

/// Start a digital capture session.
///
/// Configures the FPGA, sends the start command, and returns a
/// [`CaptureStream`] for reading data.
///
/// # Arguments
///
/// - `ep1`: Initialized EP1 command channel.
/// - `config`: Capture configuration.
///
/// # Errors
///
/// Returns an error if configuration validation fails, FPGA register
/// writes fail, or the start command fails.
pub fn start_digital_capture(
    ep1: &mut Ep1CommandChannel,
    config: &CaptureConfig,
) -> DriverResult<CaptureStream> {
    config.validate()?;

    info!(
        sample_rate = config.sample_rate,
        channels = ?config.digital_channels,
        duration_samples = config.duration_samples,
        "Starting digital capture"
    );

    // Step 1: Configure FPGA.
    configure_capture(ep1, config)?;

    // Step 2: Send START_READ command (opcode 0x01).
    debug!("Sending StartSampling (0x01)");
    ep1.start_sampling()?;

    info!("Digital capture started");
    Ok(CaptureStream::new(config))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_proto::capture_data::{ChunkHeader, StartPacketFields};
    use saleae_proto::device::endpoint as ep;
    use saleae_proto::obfuscation::EncryptionMode;
    use saleae_usb::mock::{MockTransport, UsbOperation};

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

    /// Create a mock channel with seed=0 (identity XOR).
    fn mock_channel(mock: &std::sync::Arc<MockTransport>) -> Ep1CommandChannel {
        let transport = Box::new(MockTransportWrapper(mock.clone()));
        let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::XorAll);
        channel.encryption.reseed(0);
        channel.initialized = true;
        channel
    }

    /// Queue ACK responses for capture configuration sequence.
    ///
    /// The configure_capture function writes:
    /// 1. Digital threshold register (1 write, 1 ACK)
    /// 2. Per-channel config registers (N writes, N ACKs)
    /// 3. WriteConfigurationData blob (1 write, 1 ACK)
    /// 4. Commit control register (1 write, 1 ACK)
    fn queue_configure_acks(mock: &MockTransport, num_channels: usize) {
        // Threshold write ACK
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);
        // Channel config ACKs
        for _ in 0..num_channels {
            mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);
        }
        // WriteConfigurationData ACK
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);
        // Commit control ACK
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);
    }

    // -------------------------------------------------------------------
    // CaptureConfig validation tests
    // -------------------------------------------------------------------

    #[test]
    fn config_default_is_valid() {
        CaptureConfig::default().validate().unwrap();
    }

    #[test]
    fn config_zero_sample_rate_invalid() {
        let mut config = CaptureConfig::default();
        config.sample_rate = 0;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("sample rate"));
    }

    #[test]
    fn config_no_channels_invalid() {
        let mut config = CaptureConfig::default();
        config.digital_channels = vec![];
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("channel"));
    }

    #[test]
    fn config_channel_out_of_range() {
        let mut config = CaptureConfig::default();
        config.digital_channels = vec![0, 16]; // 16 is out of range
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("16"));
    }

    #[test]
    fn config_misaligned_samples_per_chunk() {
        let mut config = CaptureConfig::default();
        config.samples_per_chunk = 100; // not multiple of 256
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("multiple of 256"));
    }

    #[test]
    fn config_small_samples_per_chunk() {
        let mut config = CaptureConfig::default();
        config.samples_per_chunk = 32;
        config.stats_enabled = false; // disable stats so the size check triggers, not alignment
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains(">= 64"));
    }

    #[test]
    fn config_valid_custom() {
        let config = CaptureConfig {
            sample_rate: 10_000_000,
            digital_channels: vec![0, 1],
            duration_samples: 1_000_000,
            voltage_threshold: 2.5,
            stats_enabled: true,
            samples_per_chunk: 512,
        };
        config.validate().unwrap();
    }

    // -------------------------------------------------------------------
    // configure_capture tests
    // -------------------------------------------------------------------

    #[test]
    fn configure_capture_sends_correct_sequence() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0, 1],
            duration_samples: 0,
            voltage_threshold: 1.65,
            stats_enabled: true,
            samples_per_chunk: 256,
        };

        // Queue ACKs: threshold(1) + channels(2, since group=2) + config_data(1) + commit(1) = 5
        queue_configure_acks(&mock, 2);

        let mut channel = mock_channel(&mock);
        configure_capture(&mut channel, &config).unwrap();

        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        // 1 threshold + 2 channels + 1 config data + 1 commit = 5
        assert_eq!(out_count, 5, "expected 5 OUT transfers");
    }

    #[test]
    fn configure_capture_4_channels() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0, 1, 2, 3],
            duration_samples: 0,
            voltage_threshold: 1.65,
            stats_enabled: true,
            samples_per_chunk: 256,
        };

        // Queue ACKs: threshold(1) + channels(4) + config_data(1) + commit(1) = 7
        queue_configure_acks(&mock, 4);

        let mut channel = mock_channel(&mock);
        configure_capture(&mut channel, &config).unwrap();

        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        assert_eq!(out_count, 7, "expected 7 OUT transfers for 4 channels");
    }

    #[test]
    fn configure_capture_rejects_invalid_config() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig {
            sample_rate: 0, // invalid
            ..CaptureConfig::default()
        };

        let mut channel = mock_channel(&mock);
        let err = configure_capture(&mut channel, &config).unwrap_err();
        assert!(err.to_string().contains("sample rate"));
    }

    // -------------------------------------------------------------------
    // start_digital_capture tests
    // -------------------------------------------------------------------

    #[test]
    fn start_capture_sends_configure_then_start() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig {
            digital_channels: vec![0, 1],
            ..CaptureConfig::default()
        };

        // Queue ACKs for configure (5) + start_sampling response (1)
        queue_configure_acks(&mock, 2);
        // StartSampling (0x01) response
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00, 0x00]);

        let mut channel = mock_channel(&mock);
        let stream = start_digital_capture(&mut channel, &config).unwrap();
        assert!(!stream.is_stopped());
        assert_eq!(stream.total_samples(), 0);
    }

    #[test]
    fn start_capture_returns_stream() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let config = CaptureConfig::default();

        // Queue ACKs for configure (7 for 4 channels) + start response
        queue_configure_acks(&mock, 4);
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00, 0x00]);

        let mut channel = mock_channel(&mock);
        let stream = start_digital_capture(&mut channel, &config).unwrap();
        assert!(!stream.is_stopped());
    }

    // -------------------------------------------------------------------
    // CaptureStream tests
    // -------------------------------------------------------------------

    #[test]
    fn stream_read_chunk_start_packet() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let config = CaptureConfig::default();
        let mut stream = CaptureStream::new(&config);

        // Build a start packet for EP2 IN.
        let header = ChunkHeader {
            channel: 0,
            software: false,
            is_start: true,
            config_id: 1,
        };
        let start = StartPacketFields {
            timestamp: 100,
            time: 200,
        };
        let mut packet = Vec::new();
        packet.extend_from_slice(&header.to_bytes());
        packet.extend_from_slice(&start.to_bytes());

        mock.push_bulk_in_response(EP2_CAPTURE_IN, packet);

        let chunk = stream.read_chunk(mock.as_ref()).unwrap().unwrap();
        assert!(chunk.header.is_start);
        assert_eq!(chunk.header.channel, 0);
        assert_eq!(chunk.start.unwrap().timestamp, 100);
    }

    #[test]
    fn stream_read_chunk_data() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let config = CaptureConfig {
            stats_enabled: false,
            ..CaptureConfig::default()
        };
        let mut stream = CaptureStream::new(&config);

        // Build a data packet for EP2 IN.
        let header = ChunkHeader {
            channel: 0,
            software: false,
            is_start: false,
            config_id: 1,
        };
        let sample_data = vec![0xFF; 32]; // 32 bytes of sample data
        let mut packet = Vec::new();
        packet.extend_from_slice(&header.to_bytes());
        packet.extend_from_slice(&sample_data);

        mock.push_bulk_in_response(EP2_CAPTURE_IN, packet);

        // Mark channel as started to avoid warning.
        stream.channel_started[0] = true;

        let chunk = stream.read_chunk(mock.as_ref()).unwrap().unwrap();
        assert!(!chunk.header.is_start);
        assert!(!chunk.sample_data.is_empty());
    }

    #[test]
    fn stream_stop_sends_commands() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let config = CaptureConfig::default();
        let mut stream = CaptureStream::new(&config);

        // Queue responses for stop sequence:
        // StopDataPath (0x86) may or may not get a response.
        // We need to set up the mock channel to handle this.

        // For stop, we need the EP1 command channel.
        // StopDataPath sends 0x86 and tries to read response.
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]); // StopDataPath response
        // StopStream sends 0x02 and reads response.
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]); // StopStream response

        let mut channel = mock_channel(&mock);
        stream.stop(&mut channel).unwrap();
        assert!(stream.is_stopped());

        // Verify commands were sent.
        let ops = mock.operations();
        let out_ops: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                UsbOperation::BulkOut { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(out_ops.len(), 2, "should send 2 commands (stop_data_path + stop_stream)");

        // First command: StopDataPath (0x86)
        assert_eq!(out_ops[0][1], 0x86, "first stop command should be 0x86");
        // Second command: StopStream (0x02)
        assert_eq!(out_ops[1][1], 0x02, "second stop command should be 0x02");
    }

    #[test]
    fn stream_stop_idempotent() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let config = CaptureConfig::default();
        let mut stream = CaptureStream::new(&config);

        // Queue stop responses.
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(ep::BULK_IN, vec![0x00]);

        let mut channel = mock_channel(&mock);
        stream.stop(&mut channel).unwrap();
        // Calling stop again should be a no-op.
        stream.stop(&mut channel).unwrap();
        assert!(stream.is_stopped());
    }

    #[test]
    fn stream_read_after_stop_returns_none() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let config = CaptureConfig::default();
        let mut stream = CaptureStream::new(&config);
        stream.stopped = true;

        let result = stream.read_chunk(mock.as_ref()).unwrap();
        assert!(result.is_none());
    }

    // -------------------------------------------------------------------
    // Threshold conversion tests
    // -------------------------------------------------------------------

    #[test]
    fn threshold_code_at_boundaries() {
        assert_eq!(voltage_to_threshold_code(0.0), 0);
        assert_eq!(voltage_to_threshold_code(3.3), 255);
        // 1.65V should be about half (127 or 128)
        let mid = voltage_to_threshold_code(1.65);
        assert!(mid >= 126 && mid <= 128, "mid should be ~127, got {mid}");
    }

    #[test]
    fn threshold_code_clamps() {
        assert_eq!(voltage_to_threshold_code(-1.0), 0);
        assert_eq!(voltage_to_threshold_code(5.0), 255);
    }

    // -------------------------------------------------------------------
    // Config blob tests
    // -------------------------------------------------------------------

    #[test]
    fn config_blob_contains_sample_rate_and_mask() {
        let config = CaptureConfig {
            sample_rate: 1_000_000,
            digital_channels: vec![0, 2],
            ..CaptureConfig::default()
        };

        let blob = build_capture_config_blob(&config);
        assert_eq!(blob.len(), 8);

        // First 4 bytes: sample rate as u32 LE
        let rate = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
        assert_eq!(rate, 1_000_000);

        // Next 4 bytes: channel mask
        let mask = u32::from_le_bytes([blob[4], blob[5], blob[6], blob[7]]);
        assert_eq!(mask, 0b0000_0101); // channels 0 and 2
    }

    // -------------------------------------------------------------------
    // Channel group count tests
    // -------------------------------------------------------------------

    #[test]
    fn channel_group_count_2() {
        let config = CaptureConfig {
            digital_channels: vec![0, 1],
            ..CaptureConfig::default()
        };
        assert_eq!(config.channel_group_count(), 2);
    }

    #[test]
    fn channel_group_count_4() {
        let config = CaptureConfig {
            digital_channels: vec![0, 1, 2, 3],
            ..CaptureConfig::default()
        };
        assert_eq!(config.channel_group_count(), 4);
    }

    #[test]
    fn channel_group_count_with_ch2() {
        // If any channel >= 2, group count is 4.
        let config = CaptureConfig {
            digital_channels: vec![0, 2],
            ..CaptureConfig::default()
        };
        assert_eq!(config.channel_group_count(), 4);
    }
}
