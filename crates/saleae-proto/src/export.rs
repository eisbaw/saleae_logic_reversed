//! Capture data export: CSV and binary formats.
//!
//! Implements streaming writers for exporting captured digital and analog
//! data, matching the Saleae Logic 2 export format documented in
//! `re/notes/export-node.md`.
//!
//! # Formats
//!
//! **CSV** (one file per data type):
//! - `digital.csv`: timestamp column + one column per channel (0/1 values)
//! - `analog.csv`: timestamp column + one column per channel (voltage values)
//!
//! **Binary** (one file per channel):
//! - `digital_N.bin`: packed bit stream (8 samples per byte, LSB first)
//! - `analog_N.bin`: raw f32 voltages in native byte order
//!
//! # Streaming
//!
//! All writers implement a batch-oriented interface: call `write_samples`
//! repeatedly with chunks of data, then `finish` to flush and close.
//! This avoids buffering entire captures in memory.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::graph_time::GraphTime;

// ---------------------------------------------------------------------------
// Export configuration
// ---------------------------------------------------------------------------

/// Export format selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// CSV text format (one file per data type).
    Csv,
    /// Binary format (one file per channel).
    Binary,
    /// Saleae Logic 2 `.sal` archive format.
    ///
    /// Produces a ZIP file compatible with `File > Open Capture` in
    /// Saleae Logic 2. See [`crate::sal`] for format details.
    Sal,
}

/// Configuration for a data export session.
#[derive(Debug, Clone)]
pub struct ExportConfig {
    /// Output directory (created if it does not exist).
    pub output_dir: PathBuf,
    /// Export format.
    pub format: ExportFormat,
    /// Sample rate in Hz (used for timestamp computation).
    pub sample_rate: u64,
    /// Channel indices being exported.
    pub channels: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Digital CSV writer
// ---------------------------------------------------------------------------

/// Streaming CSV writer for digital capture data.
///
/// Writes a `digital.csv` file with columns:
/// `timestamp,Channel 0,Channel 1,...`
///
/// Each row has a timestamp (in seconds) and 0/1 values per channel.
pub struct DigitalCsvWriter {
    writer: BufWriter<File>,
    sample_rate: u64,
    num_channels: usize,
    sample_index: u64,
}

impl DigitalCsvWriter {
    /// Create a new digital CSV writer.
    ///
    /// Creates `digital.csv` in `output_dir` with a header row.
    pub fn new(output_dir: &Path, sample_rate: u64, channels: &[u8]) -> io::Result<Self> {
        fs::create_dir_all(output_dir)?;
        let path = output_dir.join("digital.csv");
        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);

        // Write header: "timestamp,Channel 0,Channel 1,..."
        write!(writer, "timestamp")?;
        for &ch in channels {
            write!(writer, ",Channel {ch}")?;
        }
        writeln!(writer)?;

        Ok(Self {
            writer,
            sample_rate,
            num_channels: channels.len(),
            sample_index: 0,
        })
    }

    /// Write a batch of digital samples.
    ///
    /// `per_channel_samples` is a slice of per-channel boolean vectors.
    /// All channels must have the same number of samples in each batch.
    ///
    /// Returns the number of sample rows written.
    pub fn write_samples(&mut self, per_channel_samples: &[Vec<bool>]) -> io::Result<usize> {
        if per_channel_samples.is_empty() || per_channel_samples.len() != self.num_channels {
            return Ok(0);
        }

        let num_samples = per_channel_samples[0].len();
        for ch in per_channel_samples {
            if ch.len() != num_samples {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "all channels must have the same number of samples",
                ));
            }
        }

        for i in 0..num_samples {
            let t = GraphTime::from_sample_index(self.sample_index, self.sample_rate);
            let secs = t.seconds();
            let subsec = t.subsec_nanos().unsigned_abs();
            write!(self.writer, "{secs}.{subsec:09}")?;

            for ch_data in per_channel_samples {
                let val = if ch_data[i] { 1 } else { 0 };
                write!(self.writer, ",{val}")?;
            }
            writeln!(self.writer)?;

            self.sample_index += 1;
        }

        Ok(num_samples)
    }

    /// Write a batch of digital samples from packed bytes.
    ///
    /// `per_channel_bytes` contains one byte vector per channel.
    /// Each byte holds 8 samples, LSB first.
    ///
    /// Returns the number of sample rows written.
    pub fn write_packed_samples(&mut self, per_channel_bytes: &[Vec<u8>]) -> io::Result<usize> {
        if per_channel_bytes.is_empty() || per_channel_bytes.len() != self.num_channels {
            return Ok(0);
        }

        // Unpack to booleans, then delegate.
        let unpacked: Vec<Vec<bool>> = per_channel_bytes
            .iter()
            .map(|bytes| crate::capture_data::unpack_digital_samples(bytes))
            .collect();

        self.write_samples(&unpacked)
    }

    /// Flush and finish writing. Returns the underlying path.
    pub fn finish(mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

// ---------------------------------------------------------------------------
// Analog CSV writer
// ---------------------------------------------------------------------------

/// Streaming CSV writer for analog capture data.
///
/// Writes an `analog.csv` file with columns:
/// `timestamp,Channel 0,Channel 1,...`
///
/// Each row has a timestamp (in seconds) and voltage values per channel.
pub struct AnalogCsvWriter {
    writer: BufWriter<File>,
    sample_rate: u64,
    num_channels: usize,
    sample_index: u64,
}

impl AnalogCsvWriter {
    /// Create a new analog CSV writer.
    ///
    /// Creates `analog.csv` in `output_dir` with a header row.
    pub fn new(output_dir: &Path, sample_rate: u64, channels: &[u8]) -> io::Result<Self> {
        fs::create_dir_all(output_dir)?;
        let path = output_dir.join("analog.csv");
        let file = File::create(&path)?;
        let mut writer = BufWriter::new(file);

        // Write header.
        write!(writer, "timestamp")?;
        for &ch in channels {
            write!(writer, ",Channel {ch}")?;
        }
        writeln!(writer)?;

        Ok(Self {
            writer,
            sample_rate,
            num_channels: channels.len(),
            sample_index: 0,
        })
    }

    /// Write a batch of analog voltage samples.
    ///
    /// `per_channel_voltages` is a slice of per-channel f64 voltage vectors.
    /// All channels must have the same number of samples in each batch.
    ///
    /// Returns the number of sample rows written.
    pub fn write_samples(&mut self, per_channel_voltages: &[Vec<f64>]) -> io::Result<usize> {
        if per_channel_voltages.is_empty() || per_channel_voltages.len() != self.num_channels {
            return Ok(0);
        }

        let num_samples = per_channel_voltages[0].len();
        for ch in per_channel_voltages {
            if ch.len() != num_samples {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "all channels must have the same number of samples",
                ));
            }
        }

        for i in 0..num_samples {
            let t = GraphTime::from_sample_index(self.sample_index, self.sample_rate);
            let secs = t.seconds();
            let subsec = t.subsec_nanos().unsigned_abs();
            write!(self.writer, "{secs}.{subsec:09}")?;

            for ch_data in per_channel_voltages {
                write!(self.writer, ",{:.6}", ch_data[i])?;
            }
            writeln!(self.writer)?;

            self.sample_index += 1;
        }

        Ok(num_samples)
    }

    /// Flush and finish writing.
    pub fn finish(mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

// ---------------------------------------------------------------------------
// Digital binary writer
// ---------------------------------------------------------------------------

/// Streaming binary writer for digital capture data.
///
/// Creates one `digital_N.bin` file per channel. Each file contains
/// raw packed bytes (8 samples per byte, LSB first).
pub struct DigitalBinaryWriter {
    writers: Vec<BufWriter<File>>,
}

impl DigitalBinaryWriter {
    /// Create a new digital binary writer.
    ///
    /// Creates `digital_N.bin` files in `output_dir` for each channel.
    pub fn new(output_dir: &Path, channels: &[u8]) -> io::Result<Self> {
        fs::create_dir_all(output_dir)?;
        let mut writers = Vec::with_capacity(channels.len());
        for &ch in channels {
            let path = output_dir.join(format!("digital_{ch}.bin"));
            let file = File::create(&path)?;
            writers.push(BufWriter::new(file));
        }
        Ok(Self { writers })
    }

    /// Write a batch of packed digital samples.
    ///
    /// `per_channel_bytes` contains one byte vector per channel,
    /// with 8 samples per byte (LSB first).
    pub fn write_samples(&mut self, per_channel_bytes: &[Vec<u8>]) -> io::Result<()> {
        if per_channel_bytes.len() != self.writers.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "expected {} channels, got {}",
                    self.writers.len(),
                    per_channel_bytes.len()
                ),
            ));
        }

        for (writer, bytes) in self.writers.iter_mut().zip(per_channel_bytes.iter()) {
            writer.write_all(bytes)?;
        }
        Ok(())
    }

    /// Flush and finish writing.
    pub fn finish(mut self) -> io::Result<()> {
        for w in &mut self.writers {
            w.flush()?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Analog binary writer
// ---------------------------------------------------------------------------

/// Streaming binary writer for analog capture data.
///
/// Creates one `analog_N.bin` file per channel. Each file contains
/// raw f32 voltages in native byte order.
pub struct AnalogBinaryWriter {
    writers: Vec<BufWriter<File>>,
}

impl AnalogBinaryWriter {
    /// Create a new analog binary writer.
    ///
    /// Creates `analog_N.bin` files in `output_dir` for each channel.
    pub fn new(output_dir: &Path, channels: &[u8]) -> io::Result<Self> {
        fs::create_dir_all(output_dir)?;
        let mut writers = Vec::with_capacity(channels.len());
        for &ch in channels {
            let path = output_dir.join(format!("analog_{ch}.bin"));
            let file = File::create(&path)?;
            writers.push(BufWriter::new(file));
        }
        Ok(Self { writers })
    }

    /// Write a batch of analog voltage samples.
    ///
    /// `per_channel_voltages` contains one f64 voltage vector per channel.
    /// Values are downcast to f32 for storage (matching Saleae binary format).
    pub fn write_samples(&mut self, per_channel_voltages: &[Vec<f64>]) -> io::Result<()> {
        if per_channel_voltages.len() != self.writers.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "expected {} channels, got {}",
                    self.writers.len(),
                    per_channel_voltages.len()
                ),
            ));
        }

        for (writer, voltages) in self.writers.iter_mut().zip(per_channel_voltages.iter()) {
            for &v in voltages {
                let f32_val = v as f32;
                writer.write_all(&f32_val.to_ne_bytes())?;
            }
        }
        Ok(())
    }

    /// Flush and finish writing.
    pub fn finish(mut self) -> io::Result<()> {
        for w in &mut self.writers {
            w.flush()?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Convenience: create writers from ExportConfig
// ---------------------------------------------------------------------------

/// Result of creating export writers for a digital capture.
pub enum DigitalExporter {
    /// CSV export: single digital.csv file.
    Csv(DigitalCsvWriter),
    /// Binary export: one digital_N.bin per channel.
    Binary(DigitalBinaryWriter),
}

impl DigitalExporter {
    /// Create a digital exporter from configuration.
    ///
    /// Returns an error for `ExportFormat::Sal` -- use [`crate::sal::SalWriter`]
    /// directly for `.sal` archive output.
    pub fn new(config: &ExportConfig) -> io::Result<Self> {
        match config.format {
            ExportFormat::Csv => {
                let writer = DigitalCsvWriter::new(
                    &config.output_dir,
                    config.sample_rate,
                    &config.channels,
                )?;
                Ok(DigitalExporter::Csv(writer))
            }
            ExportFormat::Binary => {
                let writer = DigitalBinaryWriter::new(&config.output_dir, &config.channels)?;
                Ok(DigitalExporter::Binary(writer))
            }
            ExportFormat::Sal => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "use SalWriter for .sal format output",
            )),
        }
    }

    /// Finish writing and flush all files.
    pub fn finish(self) -> io::Result<()> {
        match self {
            DigitalExporter::Csv(w) => w.finish(),
            DigitalExporter::Binary(w) => w.finish(),
        }
    }
}

/// Result of creating export writers for an analog capture.
pub enum AnalogExporter {
    /// CSV export: single analog.csv file.
    Csv(AnalogCsvWriter),
    /// Binary export: one analog_N.bin per channel.
    Binary(AnalogBinaryWriter),
}

impl AnalogExporter {
    /// Create an analog exporter from configuration.
    ///
    /// Returns an error for `ExportFormat::Sal` -- use [`crate::sal::SalWriter`]
    /// directly for `.sal` archive output.
    pub fn new(config: &ExportConfig) -> io::Result<Self> {
        match config.format {
            ExportFormat::Csv => {
                let writer =
                    AnalogCsvWriter::new(&config.output_dir, config.sample_rate, &config.channels)?;
                Ok(AnalogExporter::Csv(writer))
            }
            ExportFormat::Binary => {
                let writer = AnalogBinaryWriter::new(&config.output_dir, &config.channels)?;
                Ok(AnalogExporter::Binary(writer))
            }
            ExportFormat::Sal => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "use SalWriter for .sal format output",
            )),
        }
    }

    /// Finish writing and flush all files.
    pub fn finish(self) -> io::Result<()> {
        match self {
            AnalogExporter::Csv(w) => w.finish(),
            AnalogExporter::Binary(w) => w.finish(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read as _;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("saleae_export_test_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    // -----------------------------------------------------------------------
    // Digital CSV tests
    // -----------------------------------------------------------------------

    #[test]
    fn digital_csv_header_and_samples() {
        let dir = temp_dir("dig_csv_basic");
        let channels = vec![0, 1, 3];

        let mut writer = DigitalCsvWriter::new(&dir, 1_000_000, &channels).unwrap();

        // Write 2 samples: ch0=[true, false], ch1=[false, true], ch3=[true, true]
        let samples = vec![vec![true, false], vec![false, true], vec![true, true]];
        let written = writer.write_samples(&samples).unwrap();
        assert_eq!(written, 2);

        writer.finish().unwrap();

        // Read and verify.
        let content = fs::read_to_string(dir.join("digital.csv")).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        // Header
        assert_eq!(lines[0], "timestamp,Channel 0,Channel 1,Channel 3");

        // Row 0: sample index 0 at 1 MHz = 0.000000000 s
        assert_eq!(lines[1], "0.000000000,1,0,1");

        // Row 1: sample index 1 at 1 MHz = 0.000001000 s
        assert_eq!(lines[2], "0.000001000,0,1,1");

        assert_eq!(lines.len(), 3); // header + 2 data rows

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn digital_csv_streaming_batches() {
        let dir = temp_dir("dig_csv_stream");
        let channels = vec![0];

        let mut writer = DigitalCsvWriter::new(&dir, 1_000_000_000, &channels).unwrap();

        // Write two batches.
        writer.write_samples(&[vec![true]]).unwrap();
        writer.write_samples(&[vec![false]]).unwrap();

        writer.finish().unwrap();

        let content = fs::read_to_string(dir.join("digital.csv")).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(lines[0], "timestamp,Channel 0");
        assert_eq!(lines[1], "0.000000000,1"); // sample 0 at 1 GHz = 0 ns
        assert_eq!(lines[2], "0.000000001,0"); // sample 1 at 1 GHz = 1 ns
        assert_eq!(lines.len(), 3);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn digital_csv_packed_samples() {
        let dir = temp_dir("dig_csv_packed");
        let channels = vec![0, 1];

        let mut writer = DigitalCsvWriter::new(&dir, 1_000_000, &channels).unwrap();

        // 0xFF = all high (8 samples), 0x00 = all low (8 samples)
        let ch0_bytes = vec![0xFF];
        let ch1_bytes = vec![0x00];
        let written = writer
            .write_packed_samples(&[ch0_bytes, ch1_bytes])
            .unwrap();
        assert_eq!(written, 8); // 1 byte = 8 samples

        writer.finish().unwrap();

        let content = fs::read_to_string(dir.join("digital.csv")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 9); // header + 8 data rows

        // All rows should have ch0=1, ch1=0
        for line in &lines[1..] {
            let parts: Vec<&str> = line.split(',').collect();
            assert_eq!(parts[1], "1");
            assert_eq!(parts[2], "0");
        }

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn digital_csv_mismatched_lengths_returns_error() {
        let dir = temp_dir("dig_csv_mismatch");
        let mut writer = DigitalCsvWriter::new(&dir, 1_000_000, &[0, 1]).unwrap();

        let result = writer.write_samples(&[vec![true, false], vec![true]]);
        assert!(result.is_err());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn digital_csv_empty_batch() {
        let dir = temp_dir("dig_csv_empty");
        let mut writer = DigitalCsvWriter::new(&dir, 1_000_000, &[0]).unwrap();

        // Empty per-channel data.
        let written = writer.write_samples(&[vec![]]).unwrap();
        assert_eq!(written, 0);

        // Wrong channel count returns 0 (not an error, just a no-op).
        let written = writer.write_samples(&[]).unwrap();
        assert_eq!(written, 0);

        writer.finish().unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    // -----------------------------------------------------------------------
    // Analog CSV tests
    // -----------------------------------------------------------------------

    #[test]
    fn analog_csv_header_and_samples() {
        let dir = temp_dir("ana_csv_basic");
        let channels = vec![0, 2];

        let mut writer = AnalogCsvWriter::new(&dir, 1_000_000, &channels).unwrap();

        let voltages = vec![vec![1.234567, -0.5], vec![7.654321, 0.0]];
        let written = writer.write_samples(&voltages).unwrap();
        assert_eq!(written, 2);

        writer.finish().unwrap();

        let content = fs::read_to_string(dir.join("analog.csv")).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(lines[0], "timestamp,Channel 0,Channel 2");
        assert_eq!(lines[1], "0.000000000,1.234567,7.654321");
        assert_eq!(lines[2], "0.000001000,-0.500000,0.000000");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn analog_csv_streaming_batches() {
        let dir = temp_dir("ana_csv_stream");
        let channels = vec![0];

        let mut writer = AnalogCsvWriter::new(&dir, 50_000_000, &channels).unwrap();

        writer.write_samples(&[vec![1.0]]).unwrap();
        writer.write_samples(&[vec![2.0]]).unwrap();
        writer.write_samples(&[vec![3.0]]).unwrap();

        writer.finish().unwrap();

        let content = fs::read_to_string(dir.join("analog.csv")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 4); // header + 3

        // At 50 MHz, sample period = 20 ns
        assert_eq!(lines[1], "0.000000000,1.000000");
        assert_eq!(lines[2], "0.000000020,2.000000");
        assert_eq!(lines[3], "0.000000040,3.000000");

        fs::remove_dir_all(&dir).unwrap();
    }

    // -----------------------------------------------------------------------
    // Digital binary tests
    // -----------------------------------------------------------------------

    #[test]
    fn digital_binary_writes_per_channel_files() {
        let dir = temp_dir("dig_bin_basic");
        let channels = vec![0, 3];

        let mut writer = DigitalBinaryWriter::new(&dir, &channels).unwrap();

        let ch0_data = vec![0xAA, 0x55];
        let ch3_data = vec![0xFF, 0x00];
        writer
            .write_samples(&[ch0_data.clone(), ch3_data.clone()])
            .unwrap();

        writer.finish().unwrap();

        // Verify file contents.
        let mut buf = Vec::new();
        File::open(dir.join("digital_0.bin"))
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf, ch0_data);

        buf.clear();
        File::open(dir.join("digital_3.bin"))
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf, ch3_data);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn digital_binary_streaming() {
        let dir = temp_dir("dig_bin_stream");
        let mut writer = DigitalBinaryWriter::new(&dir, &[0]).unwrap();

        writer.write_samples(&[vec![0xAA]]).unwrap();
        writer.write_samples(&[vec![0x55]]).unwrap();

        writer.finish().unwrap();

        let mut buf = Vec::new();
        File::open(dir.join("digital_0.bin"))
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf, vec![0xAA, 0x55]);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn digital_binary_wrong_channel_count() {
        let dir = temp_dir("dig_bin_mismatch");
        let mut writer = DigitalBinaryWriter::new(&dir, &[0, 1]).unwrap();

        let result = writer.write_samples(&[vec![0xAA]]); // 1 channel, expected 2
        assert!(result.is_err());

        fs::remove_dir_all(&dir).unwrap();
    }

    // -----------------------------------------------------------------------
    // Analog binary tests
    // -----------------------------------------------------------------------

    #[test]
    fn analog_binary_writes_f32_voltages() {
        let dir = temp_dir("ana_bin_basic");
        let channels = vec![0, 1];

        let mut writer = AnalogBinaryWriter::new(&dir, &channels).unwrap();

        let ch0_voltages = vec![1.5f64, -2.25];
        let ch1_voltages = vec![0.0, 4.567];
        writer
            .write_samples(&[ch0_voltages.clone(), ch1_voltages.clone()])
            .unwrap();

        writer.finish().unwrap();

        // Verify channel 0 file.
        let mut buf = Vec::new();
        File::open(dir.join("analog_0.bin"))
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf.len(), 8); // 2 samples * 4 bytes each

        let v0 = f32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let v1 = f32::from_ne_bytes([buf[4], buf[5], buf[6], buf[7]]);
        assert!((v0 - 1.5f32).abs() < 1e-6);
        assert!((v1 - (-2.25f32)).abs() < 1e-6);

        // Verify channel 1 file.
        buf.clear();
        File::open(dir.join("analog_1.bin"))
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf.len(), 8);

        let v0 = f32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let v1 = f32::from_ne_bytes([buf[4], buf[5], buf[6], buf[7]]);
        assert!((v0 - 0.0f32).abs() < 1e-6);
        assert!((v1 - 4.567f32).abs() < 0.001);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn analog_binary_streaming() {
        let dir = temp_dir("ana_bin_stream");
        let mut writer = AnalogBinaryWriter::new(&dir, &[0]).unwrap();

        writer.write_samples(&[vec![1.0]]).unwrap();
        writer.write_samples(&[vec![2.0]]).unwrap();
        writer.write_samples(&[vec![3.0]]).unwrap();

        writer.finish().unwrap();

        let mut buf = Vec::new();
        File::open(dir.join("analog_0.bin"))
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf.len(), 12); // 3 samples * 4 bytes

        let values: Vec<f32> = buf
            .chunks(4)
            .map(|chunk| f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        assert!((values[0] - 1.0).abs() < 1e-6);
        assert!((values[1] - 2.0).abs() < 1e-6);
        assert!((values[2] - 3.0).abs() < 1e-6);

        fs::remove_dir_all(&dir).unwrap();
    }

    // -----------------------------------------------------------------------
    // ExportConfig convenience tests
    // -----------------------------------------------------------------------

    #[test]
    fn digital_exporter_csv() {
        let dir = temp_dir("dig_export_csv");
        let config = ExportConfig {
            output_dir: dir.clone(),
            format: ExportFormat::Csv,
            sample_rate: 1_000_000,
            channels: vec![0, 1],
        };

        let exporter = DigitalExporter::new(&config).unwrap();
        match exporter {
            DigitalExporter::Csv(mut w) => {
                w.write_samples(&[vec![true], vec![false]]).unwrap();
                w.finish().unwrap();
            }
            _ => panic!("expected CSV exporter"),
        }

        assert!(dir.join("digital.csv").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn digital_exporter_binary() {
        let dir = temp_dir("dig_export_bin");
        let config = ExportConfig {
            output_dir: dir.clone(),
            format: ExportFormat::Binary,
            sample_rate: 1_000_000,
            channels: vec![0, 1],
        };

        let exporter = DigitalExporter::new(&config).unwrap();
        match exporter {
            DigitalExporter::Binary(mut w) => {
                w.write_samples(&[vec![0xFF], vec![0x00]]).unwrap();
                w.finish().unwrap();
            }
            _ => panic!("expected Binary exporter"),
        }

        assert!(dir.join("digital_0.bin").exists());
        assert!(dir.join("digital_1.bin").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn analog_exporter_csv() {
        let dir = temp_dir("ana_export_csv");
        let config = ExportConfig {
            output_dir: dir.clone(),
            format: ExportFormat::Csv,
            sample_rate: 1_000_000,
            channels: vec![0],
        };

        let exporter = AnalogExporter::new(&config).unwrap();
        match exporter {
            AnalogExporter::Csv(mut w) => {
                w.write_samples(&[vec![1.5]]).unwrap();
                w.finish().unwrap();
            }
            _ => panic!("expected CSV exporter"),
        }

        assert!(dir.join("analog.csv").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn analog_exporter_binary() {
        let dir = temp_dir("ana_export_bin");
        let config = ExportConfig {
            output_dir: dir.clone(),
            format: ExportFormat::Binary,
            sample_rate: 1_000_000,
            channels: vec![0, 2],
        };

        let exporter = AnalogExporter::new(&config).unwrap();
        match exporter {
            AnalogExporter::Binary(mut w) => {
                w.write_samples(&[vec![1.0], vec![2.0]]).unwrap();
                w.finish().unwrap();
            }
            _ => panic!("expected Binary exporter"),
        }

        assert!(dir.join("analog_0.bin").exists());
        assert!(dir.join("analog_2.bin").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn creates_output_directory_if_missing() {
        let dir = temp_dir("create_dir_test").join("nested").join("deep");
        assert!(!dir.exists());

        let mut writer = DigitalCsvWriter::new(&dir, 1_000_000, &[0]).unwrap();
        writer.write_samples(&[vec![true]]).unwrap();
        writer.finish().unwrap();

        assert!(dir.join("digital.csv").exists());
        fs::remove_dir_all(dir.parent().unwrap().parent().unwrap()).unwrap();
    }

    #[test]
    fn timestamp_at_high_sample_rate() {
        let dir = temp_dir("high_rate");
        let mut writer = AnalogCsvWriter::new(&dir, 500_000_000, &[0]).unwrap();

        // At 500 MHz, sample 0 = 0 ns, sample 1 = 2 ns
        writer.write_samples(&[vec![0.0, 0.0]]).unwrap();
        writer.finish().unwrap();

        let content = fs::read_to_string(dir.join("analog.csv")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines[1], "0.000000000,0.000000");
        assert_eq!(lines[2], "0.000000002,0.000000");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn large_batch_digital_binary() {
        let dir = temp_dir("large_batch");
        let mut writer = DigitalBinaryWriter::new(&dir, &[0]).unwrap();

        // Write 1 MB of data.
        let data = vec![0xAA; 1024 * 1024];
        writer.write_samples(&[data.clone()]).unwrap();
        writer.finish().unwrap();

        let metadata = fs::metadata(dir.join("digital_0.bin")).unwrap();
        assert_eq!(metadata.len(), 1024 * 1024);

        fs::remove_dir_all(&dir).unwrap();
    }
}
