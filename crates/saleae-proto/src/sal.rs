//! `.sal` file reader and writer for Saleae Logic 2 compatible capture archives.
//!
//! A `.sal` file is a standard ZIP archive (zlib compression level 1)
//! containing:
//!
//! - `meta.json`: session metadata, capture settings, channel manifest
//! - `digital-N.bin`: per-channel digital data (transition-encoded)
//! - `analog-N.bin`: per-channel analog data (raw f32 voltage samples)
//!
//! # Binary Formats
//!
//! ## Digital channels (`digital-N.bin`)
//!
//! The Saleae native format uses cereal binary archives with RLE
//! transition encoding. Our writer produces a simplified format:
//!
//! ```text
//! [8 bytes] transition_count (u64 LE, cereal container prefix)
//! [N * 8 bytes] transition timestamps (u64 LE sample indices)
//! ```
//!
//! The initial state is always low (0). Each timestamp marks a
//! transition (toggle) of the signal. This matches the cereal binary
//! archive convention of prefixing containers with a u64 size.
//!
//! **Caveat**: The real Saleae format includes additional lookup table
//! data and initial-state metadata within the cereal archive. Our
//! simplified format may not load correctly in Saleae Logic 2 without
//! those fields. This is documented as a known limitation.
//!
//! ## Analog channels (`analog-N.bin`)
//!
//! The Saleae native format uses cereal binary archives with
//! PackedWaveformData chunks containing int16_t samples plus LOD
//! statistics. Our writer produces a simplified single-chunk format:
//!
//! ```text
//! [8 bytes] chunk_count (u64 LE, always 1)
//! [8 bytes] sample_start (u64 LE, always 0)
//! [8 bytes] sample_end (u64 LE, sample_count)
//! [88 bytes] padding (zeroed PackedWaveformData header fields)
//! [8 bytes] sample_count (u64 LE)
//! [N * 4 bytes] f32 LE voltage samples
//! [8 bytes] stats_count (u64 LE, always 0)
//! ```
//!
//! **Caveat**: The real format uses int16_t samples with a voltage
//! transform applied at export time, plus multi-level LOD statistics.
//! Our simplified format writes f32 voltages directly and omits LOD
//! stats. This is sufficient for data preservation but may not render
//! correctly in Saleae Logic 2.
//!
//! # References
//!
//! - `re/notes/sal-file-format.md` -- RE'd format specification
//! - `re/notes/waveform-persistence.md` -- binary data format details

use std::io::{self, Cursor, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Metadata for the capture session.
///
/// Maps to the `data` field in `meta.json`.
#[derive(Debug, Clone)]
pub struct SalCaptureInfo {
    /// Capture name (displayed as tab label in Saleae Logic 2).
    pub name: String,
    /// Device type string (e.g. "LogicPro8", "Logic8").
    pub device_type: String,
    /// Device serial / ID string.
    pub device_id: String,
    /// Digital sample rate in Hz (0 if no digital channels).
    pub digital_sample_rate: u64,
    /// Analog sample rate in Hz (0 if no analog channels).
    pub analog_sample_rate: u64,
    /// Total capture duration in seconds.
    pub capture_duration_seconds: f64,
}

/// Digital channel data to include in the .sal file.
#[derive(Debug, Clone)]
pub struct SalDigitalChannel {
    /// Device channel index (0-based).
    pub channel: u8,
    /// Transition timestamps as sample indices.
    ///
    /// The initial state is assumed low (0). Each entry marks a sample
    /// index where the signal toggles. The list must be sorted in
    /// ascending order.
    pub transitions: Vec<u64>,
}

/// Analog channel data to include in the .sal file.
#[derive(Debug, Clone)]
pub struct SalAnalogChannel {
    /// Device channel index (0-based).
    pub channel: u8,
    /// Voltage samples (f32 values in volts).
    pub voltages: Vec<f32>,
}

/// Writer for Saleae Logic 2 compatible `.sal` capture archives.
///
/// Collects channel data, then writes a complete ZIP archive.
///
/// # Example
///
/// ```no_run
/// use saleae_proto::sal::{SalWriter, SalCaptureInfo, SalDigitalChannel};
///
/// let info = SalCaptureInfo {
///     name: "My Capture".into(),
///     device_type: "LogicPro8".into(),
///     device_id: "F4F31".into(),
///     digital_sample_rate: 25_000_000,
///     analog_sample_rate: 0,
///     capture_duration_seconds: 1.0,
/// };
///
/// let mut writer = SalWriter::new(info);
/// writer.add_digital_channel(SalDigitalChannel {
///     channel: 0,
///     transitions: vec![100, 200, 300],
/// });
/// writer.write_to_file("capture.sal").unwrap();
/// ```
pub struct SalWriter {
    info: SalCaptureInfo,
    digital_channels: Vec<SalDigitalChannel>,
    analog_channels: Vec<SalAnalogChannel>,
}

impl SalWriter {
    /// Create a new `.sal` writer with the given capture metadata.
    pub fn new(info: SalCaptureInfo) -> Self {
        Self {
            info,
            digital_channels: Vec::new(),
            analog_channels: Vec::new(),
        }
    }

    /// Add a digital channel to the archive.
    pub fn add_digital_channel(&mut self, channel: SalDigitalChannel) {
        self.digital_channels.push(channel);
    }

    /// Add an analog channel to the archive.
    pub fn add_analog_channel(&mut self, channel: SalAnalogChannel) {
        self.analog_channels.push(channel);
    }

    /// Write the `.sal` archive to a file.
    pub fn write_to_file(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let file = std::fs::File::create(path.as_ref())?;
        self.write_to(file)
    }

    /// Write the `.sal` archive to an in-memory buffer and return it.
    pub fn write_to_vec(&self) -> io::Result<Vec<u8>> {
        let cursor = Cursor::new(Vec::new());
        let cursor = self.write_to_inner(cursor)?;
        Ok(cursor.into_inner())
    }

    /// Write the `.sal` archive to any `Write + Seek` destination.
    pub fn write_to<W: Write + io::Seek>(&self, writer: W) -> io::Result<()> {
        self.write_to_inner(writer)?;
        Ok(())
    }

    fn write_to_inner<W: Write + io::Seek>(&self, writer: W) -> io::Result<W> {
        let options = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(1));

        let mut zip = zip::ZipWriter::new(writer);

        // 1. Write meta.json
        let meta = self.build_meta_json();
        let meta_bytes = serde_json::to_vec_pretty(&meta)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        zip.start_file("meta.json", options).map_err(zip_to_io)?;
        zip.write_all(&meta_bytes)?;

        // 2. Write digital channel .bin files
        for ch in &self.digital_channels {
            let filename = format!("digital-{}.bin", ch.channel);
            zip.start_file(&filename, options).map_err(zip_to_io)?;
            write_digital_bin(&mut zip, ch)?;
        }

        // 3. Write analog channel .bin files
        for ch in &self.analog_channels {
            let filename = format!("analog-{}.bin", ch.channel);
            zip.start_file(&filename, options).map_err(zip_to_io)?;
            write_analog_bin(&mut zip, ch)?;
        }

        let writer = zip.finish().map_err(zip_to_io)?;
        Ok(writer)
    }

    fn build_meta_json(&self) -> MetaJson {
        let total_samples_digital = if self.info.digital_sample_rate > 0 {
            (self.info.digital_sample_rate as f64 * self.info.capture_duration_seconds) as u64
        } else {
            0
        };
        let total_samples_analog = if self.info.analog_sample_rate > 0 {
            (self.info.analog_sample_rate as f64 * self.info.capture_duration_seconds) as u64
        } else {
            0
        };

        // Compute time range in nanoseconds.
        // Use the max of digital/analog durations.
        let end_time_ns = (self.info.capture_duration_seconds * 1e9) as u64;

        // Build channel lists.
        let digital_channels: Vec<u32> = self
            .digital_channels
            .iter()
            .map(|c| c.channel as u32)
            .collect();
        let analog_channels: Vec<u32> = self
            .analog_channels
            .iter()
            .map(|c| c.channel as u32)
            .collect();

        // Sample rates.
        let mut digital_sample_rates = Vec::new();
        if self.info.digital_sample_rate > 0 {
            digital_sample_rates.push(self.info.digital_sample_rate);
        }
        let mut analog_sample_rates = Vec::new();
        if self.info.analog_sample_rate > 0 {
            analog_sample_rates.push(self.info.analog_sample_rate);
        }

        // Build enabled channels for captureSettings.
        let mut enabled_channels = Vec::new();
        for ch in &digital_channels {
            enabled_channels.push(EnabledChannel {
                channel_type: "Digital".to_string(),
                index: *ch,
            });
        }
        for ch in &analog_channels {
            enabled_channels.push(EnabledChannel {
                channel_type: "Analog".to_string(),
                index: *ch,
            });
        }

        // Build binData entries.
        let mut bin_data = Vec::new();
        for ch in &self.digital_channels {
            bin_data.push(BinDataEntry {
                category: "legacy".to_string(),
                data_type: "Digital".to_string(),
                device_channel: ch.channel as u32,
                file: format!("./digital-{}.bin", ch.channel),
            });
        }
        for ch in &self.analog_channels {
            bin_data.push(BinDataEntry {
                category: "legacy".to_string(),
                data_type: "Analog".to_string(),
                device_channel: ch.channel as u32,
                file: format!("./analog-{}.bin", ch.channel),
            });
        }

        let _ = total_samples_digital;
        let _ = total_samples_analog;

        MetaJson {
            version: 16,
            data: MetaData {
                name: self.info.name.clone(),
                start_time: 0,
                end_time: end_time_ns,
                digital_channels,
                analog_channels,
                digital_sample_rates,
                analog_sample_rates,
                capture_settings: CaptureSettings {
                    buffer_size_mb: 300,
                    capture_mode: "AfterTime".to_string(),
                    stop_after_seconds: self.info.capture_duration_seconds,
                    trim_after_capture: false,
                    trim_time_seconds: 0,
                    connected_device: ConnectedDevice {
                        device_id: self.info.device_id.clone(),
                        name: device_type_display_name(&self.info.device_type),
                        device_type: self.info.device_type.clone(),
                        is_simulation: false,
                        settings: DeviceSettings {
                            enabled_channels,
                            sample_rate: SampleRateConfig {
                                digital: self.info.digital_sample_rate,
                                analog: self.info.analog_sample_rate,
                            },
                            digital_threshold: DigitalThreshold {
                                name: "3.3+ Volts".to_string(),
                                voltage: 1.65,
                            },
                        },
                    },
                    glitch_filter: GlitchFilter {
                        enabled: false,
                        channels: Vec::new(),
                    },
                },
                analyzers: Vec::new(),
                high_level_analyzers: Vec::new(),
                measurements: Vec::new(),
                digital_trigger_time: 0.0,
                render_view_state: RenderViewState {
                    left_edge_time_sec: 0.0,
                    time_sec_per_pixel: self.info.capture_duration_seconds / 1000.0,
                },
                rows_settings: Vec::new(),
                timing_markers: TimingMarkers {
                    pairs: Vec::new(),
                    a1: None,
                    a2: None,
                },
                data_table: DataTable {
                    search_string: String::new(),
                    column_widths: Vec::new(),
                },
                capture_progress: CaptureProgress {
                    max_collected_time: self.info.capture_duration_seconds,
                    processed_interval: self.info.capture_duration_seconds,
                    memory_used_mb: 0.0,
                },
            },
            bin_data,
        }
    }
}

// ---------------------------------------------------------------------------
// Binary writers
// ---------------------------------------------------------------------------

/// Write digital channel data in transition-encoded format.
///
/// Format:
/// - u64 LE: number of transitions (cereal container size prefix)
/// - N * u64 LE: transition sample indices
///
/// Initial state is always 0 (low). Each transition toggles the state.
fn write_digital_bin<W: Write>(writer: &mut W, channel: &SalDigitalChannel) -> io::Result<()> {
    // Write transition count as u64 LE (cereal container prefix).
    let count = channel.transitions.len() as u64;
    writer.write_all(&count.to_le_bytes())?;

    // Write each transition timestamp.
    for &t in &channel.transitions {
        writer.write_all(&t.to_le_bytes())?;
    }

    Ok(())
}

/// Write analog channel data in simplified single-chunk format.
///
/// Format:
/// - u64 LE: chunk_count (1)
/// - u64 LE: sample_start (0)
/// - u64 LE: sample_end (sample_count)
/// - 88 bytes: zeroed header fields (PackedWaveformData placeholders)
/// - u64 LE: sample_count
/// - N * f32 LE: voltage samples
/// - u64 LE: stats_count (0)
///
/// Note: The real Saleae format uses int16_t with voltage transforms and
/// multi-level LOD stats. This simplified format is for data preservation.
fn write_analog_bin<W: Write>(writer: &mut W, channel: &SalAnalogChannel) -> io::Result<()> {
    let sample_count = channel.voltages.len() as u64;

    // chunk_count = 1
    writer.write_all(&1u64.to_le_bytes())?;

    // SampleInterval: start=0, end=sample_count
    writer.write_all(&0u64.to_le_bytes())?;
    writer.write_all(&sample_count.to_le_bytes())?;

    // PackedWaveformData header fields (simplified: all zeros).
    // The real header has ~88 bytes of fields between the sample interval
    // and the analog data. We write zeros as placeholders.
    //
    // From waveform-persistence.md, the fields are:
    //   field_0x00 (8) + field_0x08 (8) + field_0x10 (8)
    //   + nested group 1: field_0x18 (8), field_0x20 (8)
    //   + field_0x28 (8) + field_0x30 (8)
    //   + field_0x58 (8) + field_0x60 (1)
    //   + nested group 2: field_0x68 (8), field_0x70 (8), field_0x78 (8)
    //   + field_0x80 (8)
    //   + nested group 3: field_0x88 (2), field_0x8a (2)
    //   = 8*11 + 1 + 2 + 2 = 93 bytes
    //
    // But note: the sample interval (start, end) at 0x18/0x20 is part of
    // the header too. Since we already wrote it separately as the
    // SampleInterval before the header, the cereal traversal order in
    // the serialize function writes:
    //   sample_interval (16 bytes) -- already written above
    //   then the header fields in cereal traversal order
    //
    // Looking at the wire format more carefully:
    //   field_0x00 (8) + field_0x08 (8) + field_0x10 (8) = 24
    //   nested group 1: field_0x18 (8) + field_0x20 (8) = 16
    //   field_0x28 (8) + field_0x30 (8) = 16
    //   field_0x58 (8) = 8
    //   field_0x60 (1) = 1
    //   nested group 2: field_0x68 (8) + field_0x70 (8) + field_0x78 (8) = 24
    //   field_0x80 (8) = 8
    //   nested group 3: field_0x88 (2) + field_0x8a (2) = 4
    //   Total = 24 + 16 + 16 + 8 + 1 + 24 + 8 + 4 = 101 bytes
    let header_padding = [0u8; 101];
    writer.write_all(&header_padding)?;

    // analog_data_count
    writer.write_all(&sample_count.to_le_bytes())?;

    // Write f32 LE samples.
    // Note: the real format uses int16_t. We write f32 for simplicity,
    // which means the byte count per sample is 4 instead of 2.
    // This is a known deviation from the real format.
    for &v in &channel.voltages {
        writer.write_all(&v.to_le_bytes())?;
    }

    // stats_count = 0 (no LOD statistics)
    writer.write_all(&0u64.to_le_bytes())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// SAL Reader
// ---------------------------------------------------------------------------

/// Parsed metadata from a `.sal` file's `meta.json`.
#[derive(Debug, Clone)]
pub struct SalMeta {
    /// Format version (current: 16).
    pub version: u32,
    /// Capture name.
    pub name: String,
    /// Digital channel indices present in the archive.
    pub digital_channels: Vec<u32>,
    /// Analog channel indices present in the archive.
    pub analog_channels: Vec<u32>,
    /// Digital sample rates (Hz).
    pub digital_sample_rates: Vec<u64>,
    /// Analog sample rates (Hz).
    pub analog_sample_rates: Vec<u64>,
    /// Start time (nanoseconds).
    pub start_time: u64,
    /// End time (nanoseconds).
    pub end_time: u64,
    /// Device type string (e.g., "LogicPro8").
    pub device_type: String,
}

/// Reader for Saleae Logic 2 `.sal` capture archives.
///
/// Reads the ZIP archive, parses `meta.json`, and extracts channel data
/// from the binary `.bin` files.
///
/// # Limitations
///
/// - Digital channels: reads our simplified transition-encoded format
///   (u64 count + N * u64 timestamps). Does NOT parse the real Saleae
///   cereal archive format with lookup tables and initial-state metadata.
///   To compare against real Saleae captures, the Saleae Logic 2 CSV
///   export should be used instead.
///
/// - Analog channels: reads our simplified f32 format. Does NOT parse
///   the real Saleae int16_t cereal archive format.
///
/// Both limitations are acceptable for the E2E comparison use case where
/// we compare our own output (which uses our simplified format) against
/// Saleae's CSV exports.
pub struct SalReader {
    meta: SalMeta,
    archive_data: Vec<u8>,
}

impl SalReader {
    /// Open a `.sal` file and parse its metadata.
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let data = std::fs::read(path.as_ref())?;
        Self::from_bytes(data)
    }

    /// Parse a `.sal` archive from an in-memory byte buffer.
    pub fn from_bytes(data: Vec<u8>) -> io::Result<Self> {
        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor).map_err(zip_to_io)?;

        // Read and parse meta.json.
        let mut meta_file = archive.by_name("meta.json").map_err(zip_to_io)?;
        let mut meta_str = String::new();
        meta_file.read_to_string(&mut meta_str)?;
        drop(meta_file);

        let meta_json: SalMetaJsonRead = serde_json::from_str(&meta_str)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let meta = SalMeta {
            version: meta_json.version,
            name: meta_json.data.name,
            digital_channels: meta_json.data.digital_channels,
            analog_channels: meta_json.data.analog_channels,
            digital_sample_rates: meta_json.data.digital_sample_rates,
            analog_sample_rates: meta_json.data.analog_sample_rates,
            start_time: meta_json.data.start_time,
            end_time: meta_json.data.end_time,
            device_type: meta_json
                .data
                .capture_settings
                .and_then(|cs| cs.connected_device)
                .map(|cd| cd.device_type)
                .unwrap_or_default(),
        };

        Ok(Self {
            meta,
            archive_data: data,
        })
    }

    /// Get the parsed metadata.
    pub fn meta(&self) -> &SalMeta {
        &self.meta
    }

    /// Read a digital channel's transition data.
    ///
    /// Returns the transition timestamps (sample indices where signal toggles).
    /// Initial state is assumed low (0).
    ///
    /// This reads our simplified format: u64 count + N * u64 timestamps.
    /// Will return an error for real Saleae cereal archives.
    pub fn read_digital_transitions(&self, channel: u32) -> io::Result<Vec<u64>> {
        let filename = format!("digital-{channel}.bin");
        let bin_data = self.read_zip_entry(&filename)?;

        if bin_data.len() < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{filename}: too short for transition count header"),
            ));
        }

        let count = u64::from_le_bytes(bin_data[0..8].try_into().unwrap()) as usize;
        let expected_len = 8 + count * 8;

        if bin_data.len() < expected_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{filename}: expected {expected_len} bytes for {count} transitions, got {}",
                    bin_data.len()
                ),
            ));
        }

        let mut transitions = Vec::with_capacity(count);
        for i in 0..count {
            let offset = 8 + i * 8;
            let t = u64::from_le_bytes(bin_data[offset..offset + 8].try_into().unwrap());
            transitions.push(t);
        }

        Ok(transitions)
    }

    /// Read a digital channel and expand transitions into boolean samples.
    ///
    /// `total_samples` is the number of samples to expand to. If 0, uses
    /// the last transition index + 1.
    pub fn read_digital_samples(&self, channel: u32, total_samples: u64) -> io::Result<Vec<bool>> {
        let transitions = self.read_digital_transitions(channel)?;
        Ok(crate::compare::expand_transitions(
            &transitions,
            total_samples,
        ))
    }

    /// Read an analog channel's voltage samples.
    ///
    /// This reads our simplified format: single chunk with f32 voltages.
    /// Returns voltages as f64 for comparison precision.
    pub fn read_analog_voltages(&self, channel: u32) -> io::Result<Vec<f64>> {
        let filename = format!("analog-{channel}.bin");
        let bin_data = self.read_zip_entry(&filename)?;

        if bin_data.len() < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{filename}: too short"),
            ));
        }

        let chunk_count = u64::from_le_bytes(bin_data[0..8].try_into().unwrap());
        if chunk_count == 0 {
            return Ok(Vec::new());
        }

        // Skip: chunk_count(8) + sample_start(8) + sample_end(8) + header(101)
        let data_count_offset = 8 + 8 + 8 + 101;
        if bin_data.len() < data_count_offset + 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{filename}: too short for analog header"),
            ));
        }

        let sample_count = u64::from_le_bytes(
            bin_data[data_count_offset..data_count_offset + 8]
                .try_into()
                .unwrap(),
        ) as usize;

        let samples_start = data_count_offset + 8;
        let samples_end = samples_start + sample_count * 4;

        if bin_data.len() < samples_end {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{filename}: expected {} bytes for {sample_count} f32 samples, have {}",
                    samples_end,
                    bin_data.len()
                ),
            ));
        }

        let mut voltages = Vec::with_capacity(sample_count);
        for i in 0..sample_count {
            let offset = samples_start + i * 4;
            let v = f32::from_le_bytes(bin_data[offset..offset + 4].try_into().unwrap());
            voltages.push(v as f64);
        }

        Ok(voltages)
    }

    /// List all files in the archive.
    pub fn list_files(&self) -> io::Result<Vec<String>> {
        let cursor = Cursor::new(&self.archive_data);
        let mut archive = zip::ZipArchive::new(cursor).map_err(zip_to_io)?;
        let mut names = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let file = archive.by_index(i).map_err(zip_to_io)?;
            names.push(file.name().to_string());
        }
        Ok(names)
    }

    fn read_zip_entry(&self, name: &str) -> io::Result<Vec<u8>> {
        let cursor = Cursor::new(&self.archive_data);
        let mut archive = zip::ZipArchive::new(cursor).map_err(zip_to_io)?;
        let mut file = archive.by_name(name).map_err(zip_to_io)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        Ok(data)
    }
}

// Deserialization types for reading meta.json (permissive, only fields we need).

#[derive(Deserialize)]
struct SalMetaJsonRead {
    version: u32,
    data: SalMetaDataRead,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SalMetaDataRead {
    name: String,
    start_time: u64,
    end_time: u64,
    #[serde(default)]
    digital_channels: Vec<u32>,
    #[serde(default)]
    analog_channels: Vec<u32>,
    #[serde(default)]
    digital_sample_rates: Vec<u64>,
    #[serde(default)]
    analog_sample_rates: Vec<u64>,
    #[serde(default)]
    capture_settings: Option<SalCaptureSettingsRead>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SalCaptureSettingsRead {
    connected_device: Option<SalConnectedDeviceRead>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SalConnectedDeviceRead {
    device_type: String,
}

// ---------------------------------------------------------------------------
// CSV readers for comparison
// ---------------------------------------------------------------------------

/// Read digital samples from a CSV file (Saleae Logic 2 or our export format).
///
/// Expected format:
/// ```text
/// timestamp,Channel 0,Channel 1,...
/// 0.000000000,1,0,...
/// ```
///
/// Returns per-channel boolean samples. The channel order matches the CSV
/// column order (not the channel number in the header).
pub fn read_digital_csv(path: impl AsRef<Path>) -> io::Result<Vec<Vec<bool>>> {
    let content = std::fs::read_to_string(path.as_ref())?;
    let mut lines = content.lines();

    let header = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "empty CSV file"))?;

    let num_channels = header.split(',').count() - 1; // subtract timestamp column
    if num_channels == 0 {
        return Ok(Vec::new());
    }

    let mut channels: Vec<Vec<bool>> = vec![Vec::new(); num_channels];

    for line in lines {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < num_channels + 1 {
            continue;
        }
        for (ch, field) in fields[1..=num_channels].iter().enumerate() {
            let val = field.trim() == "1";
            channels[ch].push(val);
        }
    }

    Ok(channels)
}

/// Read analog samples from a CSV file (Saleae Logic 2 or our export format).
///
/// Expected format:
/// ```text
/// timestamp,Channel 0,Channel 1,...
/// 0.000000000,1.234567,7.654321,...
/// ```
///
/// Returns per-channel f64 voltage samples.
pub fn read_analog_csv(path: impl AsRef<Path>) -> io::Result<Vec<Vec<f64>>> {
    let content = std::fs::read_to_string(path.as_ref())?;
    let mut lines = content.lines();

    let header = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "empty CSV file"))?;

    let num_channels = header.split(',').count() - 1;
    if num_channels == 0 {
        return Ok(Vec::new());
    }

    let mut channels: Vec<Vec<f64>> = vec![Vec::new(); num_channels];

    for line in lines {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < num_channels + 1 {
            continue;
        }
        for (ch, field) in fields[1..=num_channels].iter().enumerate() {
            let val: f64 = field.trim().parse().map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("bad float: {e}"))
            })?;
            channels[ch].push(val);
        }
    }

    Ok(channels)
}

// ---------------------------------------------------------------------------
// meta.json serde types (write)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MetaJson {
    version: u32,
    data: MetaData,
    #[serde(rename = "binData")]
    bin_data: Vec<BinDataEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MetaData {
    name: String,
    start_time: u64,
    end_time: u64,
    digital_channels: Vec<u32>,
    analog_channels: Vec<u32>,
    digital_sample_rates: Vec<u64>,
    analog_sample_rates: Vec<u64>,
    capture_settings: CaptureSettings,
    analyzers: Vec<serde_json::Value>,
    high_level_analyzers: Vec<serde_json::Value>,
    measurements: Vec<serde_json::Value>,
    digital_trigger_time: f64,
    render_view_state: RenderViewState,
    rows_settings: Vec<serde_json::Value>,
    timing_markers: TimingMarkers,
    data_table: DataTable,
    capture_progress: CaptureProgress,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureSettings {
    buffer_size_mb: u32,
    capture_mode: String,
    stop_after_seconds: f64,
    trim_after_capture: bool,
    trim_time_seconds: u32,
    connected_device: ConnectedDevice,
    glitch_filter: GlitchFilter,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectedDevice {
    device_id: String,
    name: String,
    device_type: String,
    is_simulation: bool,
    settings: DeviceSettings,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceSettings {
    enabled_channels: Vec<EnabledChannel>,
    sample_rate: SampleRateConfig,
    digital_threshold: DigitalThreshold,
}

#[derive(Serialize)]
struct EnabledChannel {
    #[serde(rename = "type")]
    channel_type: String,
    index: u32,
}

#[derive(Serialize)]
struct SampleRateConfig {
    digital: u64,
    analog: u64,
}

#[derive(Serialize)]
struct DigitalThreshold {
    name: String,
    voltage: f64,
}

#[derive(Serialize)]
struct GlitchFilter {
    enabled: bool,
    channels: Vec<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderViewState {
    left_edge_time_sec: f64,
    time_sec_per_pixel: f64,
}

#[derive(Serialize)]
struct TimingMarkers {
    pairs: Vec<serde_json::Value>,
    a1: Option<serde_json::Value>,
    a2: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DataTable {
    search_string: String,
    column_widths: Vec<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaptureProgress {
    max_collected_time: f64,
    processed_interval: f64,
    memory_used_mb: f64,
}

#[derive(Serialize)]
struct BinDataEntry {
    category: String,
    #[serde(rename = "type")]
    data_type: String,
    #[serde(rename = "deviceChannel")]
    device_channel: u32,
    file: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn device_type_display_name(device_type: &str) -> String {
    match device_type {
        "LogicPro16" => "Logic Pro 16".to_string(),
        "LogicPro8" => "Logic Pro 8".to_string(),
        "Logic8" => "Logic 8".to_string(),
        "Logic4" => "Logic 4".to_string(),
        "Logic16" => "Logic 16".to_string(),
        "Logic" => "Logic".to_string(),
        _ => device_type.to_string(),
    }
}

fn zip_to_io(e: zip::result::ZipError) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn test_capture_info() -> SalCaptureInfo {
        SalCaptureInfo {
            name: "Test Capture".into(),
            device_type: "LogicPro8".into(),
            device_id: "TEST01".into(),
            digital_sample_rate: 25_000_000,
            analog_sample_rate: 6_250_000,
            capture_duration_seconds: 1.0,
        }
    }

    #[test]
    fn sal_creates_valid_zip() {
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_digital_channel(SalDigitalChannel {
            channel: 0,
            transitions: vec![100, 200, 300],
        });
        writer.add_analog_channel(SalAnalogChannel {
            channel: 0,
            voltages: vec![1.0, 2.0, 3.0],
        });

        let data = writer.write_to_vec().unwrap();

        // Verify it's a valid ZIP.
        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();

        // Check expected files exist.
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();

        assert!(names.contains(&"meta.json".to_string()));
        assert!(names.contains(&"digital-0.bin".to_string()));
        assert!(names.contains(&"analog-0.bin".to_string()));
    }

    #[test]
    fn sal_meta_json_has_version_16() {
        let writer = SalWriter::new(test_capture_info());
        let data = writer.write_to_vec().unwrap();

        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let mut meta_file = archive.by_name("meta.json").unwrap();
        let mut meta_str = String::new();
        meta_file.read_to_string(&mut meta_str).unwrap();

        let meta: serde_json::Value = serde_json::from_str(&meta_str).unwrap();

        assert_eq!(meta["version"], 16);
        assert!(meta["data"].is_object());
        assert!(meta["binData"].is_array());
    }

    #[test]
    fn sal_meta_json_schema() {
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_digital_channel(SalDigitalChannel {
            channel: 0,
            transitions: vec![],
        });
        writer.add_digital_channel(SalDigitalChannel {
            channel: 3,
            transitions: vec![],
        });
        writer.add_analog_channel(SalAnalogChannel {
            channel: 1,
            voltages: vec![],
        });

        let data = writer.write_to_vec().unwrap();
        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let mut meta_file = archive.by_name("meta.json").unwrap();
        let mut meta_str = String::new();
        meta_file.read_to_string(&mut meta_str).unwrap();

        let meta: serde_json::Value = serde_json::from_str(&meta_str).unwrap();
        let data_obj = &meta["data"];

        // Check required data fields.
        assert!(data_obj["name"].is_string());
        assert_eq!(data_obj["name"], "Test Capture");
        assert!(data_obj["startTime"].is_u64());
        assert!(data_obj["endTime"].is_u64());
        assert!(data_obj["digitalChannels"].is_array());
        assert!(data_obj["analogChannels"].is_array());
        assert!(data_obj["digitalSampleRates"].is_array());
        assert!(data_obj["analogSampleRates"].is_array());
        assert!(data_obj["captureSettings"].is_object());
        assert!(data_obj["analyzers"].is_array());
        assert!(data_obj["highLevelAnalyzers"].is_array());
        assert!(data_obj["measurements"].is_array());
        assert!(data_obj["renderViewState"].is_object());
        assert!(data_obj["rowsSettings"].is_array());
        assert!(data_obj["timingMarkers"].is_object());
        assert!(data_obj["dataTable"].is_object());
        assert!(data_obj["captureProgress"].is_object());

        // Check channel lists.
        let dig_channels = data_obj["digitalChannels"].as_array().unwrap();
        assert_eq!(dig_channels.len(), 2);
        assert_eq!(dig_channels[0], 0);
        assert_eq!(dig_channels[1], 3);

        let ana_channels = data_obj["analogChannels"].as_array().unwrap();
        assert_eq!(ana_channels.len(), 1);
        assert_eq!(ana_channels[0], 1);

        // Check sample rates.
        let dig_rates = data_obj["digitalSampleRates"].as_array().unwrap();
        assert_eq!(dig_rates.len(), 1);
        assert_eq!(dig_rates[0], 25_000_000u64);

        let ana_rates = data_obj["analogSampleRates"].as_array().unwrap();
        assert_eq!(ana_rates.len(), 1);
        assert_eq!(ana_rates[0], 6_250_000u64);

        // Check binData.
        let bin_data = meta["binData"].as_array().unwrap();
        assert_eq!(bin_data.len(), 3);

        assert_eq!(bin_data[0]["category"], "legacy");
        assert_eq!(bin_data[0]["type"], "Digital");
        assert_eq!(bin_data[0]["deviceChannel"], 0);
        assert_eq!(bin_data[0]["file"], "./digital-0.bin");

        assert_eq!(bin_data[1]["category"], "legacy");
        assert_eq!(bin_data[1]["type"], "Digital");
        assert_eq!(bin_data[1]["deviceChannel"], 3);
        assert_eq!(bin_data[1]["file"], "./digital-3.bin");

        assert_eq!(bin_data[2]["category"], "legacy");
        assert_eq!(bin_data[2]["type"], "Analog");
        assert_eq!(bin_data[2]["deviceChannel"], 1);
        assert_eq!(bin_data[2]["file"], "./analog-1.bin");
    }

    #[test]
    fn sal_meta_json_capture_settings() {
        let writer = SalWriter::new(test_capture_info());
        let data = writer.write_to_vec().unwrap();

        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let mut meta_file = archive.by_name("meta.json").unwrap();
        let mut meta_str = String::new();
        meta_file.read_to_string(&mut meta_str).unwrap();

        let meta: serde_json::Value = serde_json::from_str(&meta_str).unwrap();
        let settings = &meta["data"]["captureSettings"];

        assert_eq!(settings["bufferSizeMb"], 300);
        assert_eq!(settings["captureMode"], "AfterTime");
        assert_eq!(settings["stopAfterSeconds"], 1.0);

        let device = &settings["connectedDevice"];
        assert_eq!(device["deviceId"], "TEST01");
        assert_eq!(device["name"], "Logic Pro 8");
        assert_eq!(device["deviceType"], "LogicPro8");
        assert_eq!(device["isSimulation"], false);

        let sr = &device["settings"]["sampleRate"];
        assert_eq!(sr["digital"], 25_000_000u64);
        assert_eq!(sr["analog"], 6_250_000u64);
    }

    #[test]
    fn sal_digital_channel_round_trip() {
        let transitions = vec![10, 50, 100, 200, 500];
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_digital_channel(SalDigitalChannel {
            channel: 2,
            transitions: transitions.clone(),
        });

        let data = writer.write_to_vec().unwrap();
        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let mut bin_file = archive.by_name("digital-2.bin").unwrap();
        let mut bin_data = Vec::new();
        bin_file.read_to_end(&mut bin_data).unwrap();

        // Parse the binary: u64 count + N * u64 transitions
        assert!(bin_data.len() >= 8);
        let count = u64::from_le_bytes(bin_data[0..8].try_into().unwrap());
        assert_eq!(count, 5);

        let mut read_transitions = Vec::new();
        for i in 0..count as usize {
            let offset = 8 + i * 8;
            let t = u64::from_le_bytes(bin_data[offset..offset + 8].try_into().unwrap());
            read_transitions.push(t);
        }
        assert_eq!(read_transitions, transitions);

        // Total size: 8 + 5*8 = 48
        assert_eq!(bin_data.len(), 48);
    }

    #[test]
    fn sal_digital_channel_empty_transitions() {
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_digital_channel(SalDigitalChannel {
            channel: 0,
            transitions: vec![],
        });

        let data = writer.write_to_vec().unwrap();
        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let mut bin_file = archive.by_name("digital-0.bin").unwrap();
        let mut bin_data = Vec::new();
        bin_file.read_to_end(&mut bin_data).unwrap();

        // Just the count (0).
        assert_eq!(bin_data.len(), 8);
        let count = u64::from_le_bytes(bin_data[0..8].try_into().unwrap());
        assert_eq!(count, 0);
    }

    #[test]
    fn sal_analog_channel_round_trip() {
        let voltages: Vec<f32> = vec![1.5, -0.25, 0.0, 3.3, -1.65];
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_analog_channel(SalAnalogChannel {
            channel: 1,
            voltages: voltages.clone(),
        });

        let data = writer.write_to_vec().unwrap();
        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let mut bin_file = archive.by_name("analog-1.bin").unwrap();
        let mut bin_data = Vec::new();
        bin_file.read_to_end(&mut bin_data).unwrap();

        // Parse the binary.
        let mut offset = 0;

        // chunk_count = 1
        let chunk_count = u64::from_le_bytes(bin_data[offset..offset + 8].try_into().unwrap());
        assert_eq!(chunk_count, 1);
        offset += 8;

        // sample_start = 0
        let sample_start = u64::from_le_bytes(bin_data[offset..offset + 8].try_into().unwrap());
        assert_eq!(sample_start, 0);
        offset += 8;

        // sample_end = 5
        let sample_end = u64::from_le_bytes(bin_data[offset..offset + 8].try_into().unwrap());
        assert_eq!(sample_end, 5);
        offset += 8;

        // Skip 101 bytes of header padding.
        offset += 101;

        // analog_data_count = 5
        let analog_count = u64::from_le_bytes(bin_data[offset..offset + 8].try_into().unwrap());
        assert_eq!(analog_count, 5);
        offset += 8;

        // Read f32 samples.
        let mut read_voltages = Vec::new();
        for _ in 0..5 {
            let v = f32::from_le_bytes(bin_data[offset..offset + 4].try_into().unwrap());
            read_voltages.push(v);
            offset += 4;
        }
        assert_eq!(read_voltages, voltages);

        // stats_count = 0
        let stats_count = u64::from_le_bytes(bin_data[offset..offset + 8].try_into().unwrap());
        assert_eq!(stats_count, 0);
        offset += 8;

        // Verify we consumed all bytes.
        assert_eq!(offset, bin_data.len());
    }

    #[test]
    fn sal_analog_channel_empty() {
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_analog_channel(SalAnalogChannel {
            channel: 0,
            voltages: vec![],
        });

        let data = writer.write_to_vec().unwrap();
        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let mut bin_file = archive.by_name("analog-0.bin").unwrap();
        let mut bin_data = Vec::new();
        bin_file.read_to_end(&mut bin_data).unwrap();

        // chunk_count(8) + sample_start(8) + sample_end(8) + header(101)
        // + analog_count(8) + stats_count(8) = 141
        assert_eq!(bin_data.len(), 141);
    }

    #[test]
    fn sal_multiple_channels() {
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_digital_channel(SalDigitalChannel {
            channel: 0,
            transitions: vec![10, 20],
        });
        writer.add_digital_channel(SalDigitalChannel {
            channel: 1,
            transitions: vec![30, 40],
        });
        writer.add_analog_channel(SalAnalogChannel {
            channel: 0,
            voltages: vec![1.0],
        });
        writer.add_analog_channel(SalAnalogChannel {
            channel: 1,
            voltages: vec![2.0],
        });

        let data = writer.write_to_vec().unwrap();
        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();

        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();

        assert!(names.contains(&"meta.json".to_string()));
        assert!(names.contains(&"digital-0.bin".to_string()));
        assert!(names.contains(&"digital-1.bin".to_string()));
        assert!(names.contains(&"analog-0.bin".to_string()));
        assert!(names.contains(&"analog-1.bin".to_string()));
        assert_eq!(names.len(), 5);
    }

    #[test]
    fn sal_write_to_file() {
        let dir = std::env::temp_dir().join(format!("sal_file_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.sal");

        let mut writer = SalWriter::new(test_capture_info());
        writer.add_digital_channel(SalDigitalChannel {
            channel: 0,
            transitions: vec![100],
        });
        writer.write_to_file(&path).unwrap();

        // Verify the file is a valid ZIP.
        let file = std::fs::File::open(&path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert!(archive.by_name("meta.json").is_ok());
        assert!(archive.by_name("digital-0.bin").is_ok());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sal_no_channels() {
        // A .sal with no channels should still produce a valid ZIP with meta.json.
        let info = SalCaptureInfo {
            name: "Empty Capture".into(),
            device_type: "LogicPro8".into(),
            device_id: "TEST01".into(),
            digital_sample_rate: 0,
            analog_sample_rate: 0,
            capture_duration_seconds: 0.0,
        };
        let writer = SalWriter::new(info);
        let data = writer.write_to_vec().unwrap();

        let cursor = Cursor::new(&data);
        let archive = zip::ZipArchive::new(cursor).unwrap();
        assert_eq!(archive.len(), 1); // just meta.json
    }

    #[test]
    fn sal_device_type_display_names() {
        assert_eq!(device_type_display_name("LogicPro16"), "Logic Pro 16");
        assert_eq!(device_type_display_name("LogicPro8"), "Logic Pro 8");
        assert_eq!(device_type_display_name("Logic8"), "Logic 8");
        assert_eq!(device_type_display_name("Logic4"), "Logic 4");
        assert_eq!(device_type_display_name("Logic16"), "Logic 16");
        assert_eq!(device_type_display_name("Logic"), "Logic");
        assert_eq!(device_type_display_name("Unknown"), "Unknown");
    }

    #[test]
    fn sal_end_time_computed_correctly() {
        let info = SalCaptureInfo {
            name: "Time Test".into(),
            device_type: "Logic8".into(),
            device_id: "T01".into(),
            digital_sample_rate: 1_000_000,
            analog_sample_rate: 0,
            capture_duration_seconds: 2.5,
        };
        let writer = SalWriter::new(info);
        let data = writer.write_to_vec().unwrap();

        let cursor = Cursor::new(&data);
        let mut archive = zip::ZipArchive::new(cursor).unwrap();
        let mut meta_file = archive.by_name("meta.json").unwrap();
        let mut meta_str = String::new();
        meta_file.read_to_string(&mut meta_str).unwrap();

        let meta: serde_json::Value = serde_json::from_str(&meta_str).unwrap();
        assert_eq!(meta["data"]["startTime"], 0);
        // 2.5 seconds = 2_500_000_000 nanoseconds
        assert_eq!(meta["data"]["endTime"], 2_500_000_000u64);
    }

    // -------------------------------------------------------------------
    // SalReader tests
    // -------------------------------------------------------------------

    #[test]
    fn sal_reader_metadata() {
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_digital_channel(SalDigitalChannel {
            channel: 0,
            transitions: vec![10, 20],
        });
        writer.add_analog_channel(SalAnalogChannel {
            channel: 1,
            voltages: vec![1.0, 2.0],
        });

        let data = writer.write_to_vec().unwrap();
        let reader = SalReader::from_bytes(data).unwrap();
        let meta = reader.meta();

        assert_eq!(meta.version, 16);
        assert_eq!(meta.name, "Test Capture");
        assert_eq!(meta.digital_channels, vec![0]);
        assert_eq!(meta.analog_channels, vec![1]);
        assert_eq!(meta.digital_sample_rates, vec![25_000_000]);
        assert_eq!(meta.analog_sample_rates, vec![6_250_000]);
        assert_eq!(meta.device_type, "LogicPro8");
    }

    #[test]
    fn sal_reader_digital_transitions_roundtrip() {
        let transitions = vec![10, 50, 100, 200, 500];
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_digital_channel(SalDigitalChannel {
            channel: 2,
            transitions: transitions.clone(),
        });

        let data = writer.write_to_vec().unwrap();
        let reader = SalReader::from_bytes(data).unwrap();
        let read_transitions = reader.read_digital_transitions(2).unwrap();
        assert_eq!(read_transitions, transitions);
    }

    #[test]
    fn sal_reader_digital_samples_roundtrip() {
        let transitions = vec![3, 7];
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_digital_channel(SalDigitalChannel {
            channel: 0,
            transitions: transitions.clone(),
        });

        let data = writer.write_to_vec().unwrap();
        let reader = SalReader::from_bytes(data).unwrap();
        let samples = reader.read_digital_samples(0, 10).unwrap();

        // Low[0..3], High[3..7], Low[7..10]
        assert_eq!(
            samples,
            vec![false, false, false, true, true, true, true, false, false, false]
        );
    }

    #[test]
    fn sal_reader_analog_roundtrip() {
        let voltages: Vec<f32> = vec![1.5, -0.25, 0.0, 3.3];
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_analog_channel(SalAnalogChannel {
            channel: 0,
            voltages: voltages.clone(),
        });

        let data = writer.write_to_vec().unwrap();
        let reader = SalReader::from_bytes(data).unwrap();
        let read_voltages = reader.read_analog_voltages(0).unwrap();

        assert_eq!(read_voltages.len(), voltages.len());
        for (read, expected) in read_voltages.iter().zip(voltages.iter()) {
            assert!(
                (*read - *expected as f64).abs() < 1e-6,
                "expected {expected}, got {read}"
            );
        }
    }

    #[test]
    fn sal_reader_list_files() {
        let mut writer = SalWriter::new(test_capture_info());
        writer.add_digital_channel(SalDigitalChannel {
            channel: 0,
            transitions: vec![],
        });
        writer.add_analog_channel(SalAnalogChannel {
            channel: 1,
            voltages: vec![],
        });

        let data = writer.write_to_vec().unwrap();
        let reader = SalReader::from_bytes(data).unwrap();
        let files = reader.list_files().unwrap();

        assert!(files.contains(&"meta.json".to_string()));
        assert!(files.contains(&"digital-0.bin".to_string()));
        assert!(files.contains(&"analog-1.bin".to_string()));
    }

    #[test]
    fn sal_reader_missing_channel_returns_error() {
        let writer = SalWriter::new(test_capture_info());
        let data = writer.write_to_vec().unwrap();
        let reader = SalReader::from_bytes(data).unwrap();

        assert!(reader.read_digital_transitions(0).is_err());
        assert!(reader.read_analog_voltages(0).is_err());
    }

    // -------------------------------------------------------------------
    // CSV reader tests
    // -------------------------------------------------------------------

    #[test]
    fn read_digital_csv_basic() {
        let dir = std::env::temp_dir().join(format!("sal_csv_dig_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("digital.csv");
        std::fs::write(
            &path,
            "timestamp,Channel 0,Channel 1\n\
             0.000000000,1,0\n\
             0.000001000,0,1\n\
             0.000002000,1,1\n",
        )
        .unwrap();

        let channels = read_digital_csv(&path).unwrap();
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0], vec![true, false, true]);
        assert_eq!(channels[1], vec![false, true, true]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn read_analog_csv_basic() {
        let dir = std::env::temp_dir().join(format!("sal_csv_ana_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("analog.csv");
        std::fs::write(
            &path,
            "timestamp,Channel 0,Channel 2\n\
             0.000000000,1.234567,7.654321\n\
             0.000001000,-0.500000,0.000000\n",
        )
        .unwrap();

        let channels = read_analog_csv(&path).unwrap();
        assert_eq!(channels.len(), 2);
        assert!((channels[0][0] - 1.234567).abs() < 1e-6);
        assert!((channels[0][1] - (-0.5)).abs() < 1e-6);
        assert!((channels[1][0] - 7.654321).abs() < 1e-6);
        assert!((channels[1][1] - 0.0).abs() < 1e-6);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
