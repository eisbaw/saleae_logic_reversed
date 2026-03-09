//! Capture data parsing for EP2 IN bulk transfer stream.
//!
//! The Saleae FPGA sends interleaved sample data on EP2 IN (0x82).
//! Each data chunk begins with a 4-byte header. When the header's
//! `is_start` bit is set, 12 additional bytes follow (timestamp + time)
//! forming a 16-byte "start packet". Otherwise, sample data follows
//! the header directly.
//!
//! When stats collection is enabled, each chunk's sample data is followed
//! by a 20+ byte stats trailer containing overflow counters and timing.
//!
//! # Wire format references
//!
//! - `re/notes/start-packet-format.md` -- chunk header disassembly
//! - `re/notes/withstats-format.md` -- stats trailer disassembly
//! - `doc/protocol/digital-capture.md` -- protocol overview
//!
//! # Confidence
//!
//! The header bitfield layout is **High confidence** (direct disassembly
//! of shift/mask operations at VA 0x2d41ced). The stats format is **High
//! confidence** (disassembly at VA 0x2d41510). The exact interpretation
//! of some stats flag bits is still hypothesis-level.

use crate::error::{ProtoError, ProtoResult};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of the chunk header on the wire (bytes).
pub const CHUNK_HEADER_SIZE: usize = 4;

/// Size of the start packet extension after the header (timestamp + time).
pub const START_PACKET_EXTENSION_SIZE: usize = 12;

/// Total start packet size on the wire (header + timestamp + time).
pub const START_PACKET_TOTAL_SIZE: usize = CHUNK_HEADER_SIZE + START_PACKET_EXTENSION_SIZE;

/// Mandatory stats header size (bytes).
///
/// Fields: packet_id(4) + timestamp_48(6) + flags(2) + host_time(8) = 20.
pub const STATS_HEADER_SIZE: usize = 20;

/// Overflow metadata size when present (bytes).
pub const OVERFLOW_METADATA_SIZE: usize = 16;

/// Size of one overflow data block: 512 bytes sample + 124 bytes stats.
pub const OVERFLOW_BLOCK_SIZE: usize = 636;

/// Samples per packet alignment when stats are enabled.
pub const SAMPLES_PER_PACKET_MULTIPLE_WITH_STATS: usize = 256;

/// Maximum number of digital channels supported.
pub const MAX_DIGITAL_CHANNELS: usize = 4;

// ---------------------------------------------------------------------------
// Chunk header
// ---------------------------------------------------------------------------

/// Parsed 4-byte chunk header from the interleaved EP2 IN data stream.
///
/// Every data chunk starts with this header. The layout is a 32-bit
/// little-endian word with the following bitfield:
///
/// ```text
/// [7:0]   channel     -- channel number (0-3)
/// [8]     software    -- software-generated data flag
/// [9]     is_start    -- start packet discriminator
/// [15:10] reserved
/// [31:16] config_id   -- configuration ID for stream sync
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkHeader {
    /// Channel number (0-3).
    pub channel: u8,
    /// Software-generated data flag.
    pub software: bool,
    /// When true, start packet fields (timestamp + time) follow.
    pub is_start: bool,
    /// Configuration ID for stream synchronization.
    pub config_id: u16,
}

impl ChunkHeader {
    /// Parse a chunk header from a 4-byte little-endian word.
    pub fn parse(data: &[u8]) -> ProtoResult<Self> {
        if data.len() < CHUNK_HEADER_SIZE {
            return Err(ProtoError::BufferTooShort {
                need: CHUNK_HEADER_SIZE,
                have: data.len(),
            });
        }

        let word = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);

        Ok(Self {
            channel: (word & 0xFF) as u8,
            software: (word >> 8) & 1 != 0,
            is_start: word & 0x200 != 0,
            config_id: (word >> 16) as u16,
        })
    }

    /// Encode this header as a 4-byte little-endian word.
    pub fn to_bytes(&self) -> [u8; 4] {
        let mut word: u32 = self.channel as u32;
        if self.software {
            word |= 1 << 8;
        }
        if self.is_start {
            word |= 1 << 9;
        }
        word |= (self.config_id as u32) << 16;
        word.to_le_bytes()
    }
}

// ---------------------------------------------------------------------------
// Start packet fields
// ---------------------------------------------------------------------------

/// Start packet extension fields that follow the chunk header when
/// `is_start` is set.
///
/// The start packet establishes the beginning of a channel's data stream.
/// The host rejects data that arrives without a preceding start packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartPacketFields {
    /// 32-bit device-side timestamp (FPGA clock).
    pub timestamp: u32,
    /// 64-bit host/graph time.
    pub time: u64,
}

impl StartPacketFields {
    /// Parse start packet fields from 12 bytes after the chunk header.
    pub fn parse(data: &[u8]) -> ProtoResult<Self> {
        if data.len() < START_PACKET_EXTENSION_SIZE {
            return Err(ProtoError::BufferTooShort {
                need: START_PACKET_EXTENSION_SIZE,
                have: data.len(),
            });
        }

        let timestamp = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let time = u64::from_le_bytes([
            data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11],
        ]);

        Ok(Self { timestamp, time })
    }

    /// Encode these fields as 12 bytes (timestamp LE + time LE).
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut out = [0u8; 12];
        out[..4].copy_from_slice(&self.timestamp.to_le_bytes());
        out[4..12].copy_from_slice(&self.time.to_le_bytes());
        out
    }
}

// ---------------------------------------------------------------------------
// Stats header
// ---------------------------------------------------------------------------

/// Mandatory 20-byte stats header appended after sample data in each chunk
/// when stats collection is enabled.
///
/// Wire format:
/// ```text
/// Offset  Size  Description
/// 0x00    4     Packet counter (sequential ID), LE
/// 0x04    6     48-bit FPGA timestamp (packed in u64, LE)
/// 0x0a    2     Flags bitfield (u16, LE)
/// 0x0c    8     Host/graph time (u64, LE)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsHeader {
    /// Sequential packet counter.
    pub packet_id: u32,
    /// 48-bit FPGA timestamp (upper 16 bits are zero).
    pub timestamp_48: u64,
    /// Raw flags bitfield.
    pub flags: u16,
    /// Host/graph time.
    pub host_time: u64,
}

impl StatsHeader {
    /// Parse the mandatory 20-byte stats header.
    pub fn parse(data: &[u8]) -> ProtoResult<Self> {
        if data.len() < STATS_HEADER_SIZE {
            return Err(ProtoError::BufferTooShort {
                need: STATS_HEADER_SIZE,
                have: data.len(),
            });
        }

        let packet_id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);

        // 48-bit timestamp: 6 bytes LE, zero-extended to u64
        let timestamp_48 = u64::from_le_bytes([
            data[4], data[5], data[6], data[7], data[8], data[9], 0, 0,
        ]);

        let flags = u16::from_le_bytes([data[10], data[11]]);

        let host_time = u64::from_le_bytes([
            data[12], data[13], data[14], data[15], data[16], data[17], data[18], data[19],
        ]);

        Ok(Self {
            packet_id,
            timestamp_48,
            flags,
            host_time,
        })
    }

    /// Whether the overflow metadata extension is present (bit 9).
    pub fn has_overflow_meta(&self) -> bool {
        self.flags & (1 << 9) != 0
    }

    /// Whether overflow data blocks are present (bits 9 AND 12).
    pub fn has_overflow_data(&self) -> bool {
        self.has_overflow_meta() && (self.flags & (1 << 12) != 0)
    }

    /// Whether the debug log flag is set (bit 10).
    pub fn debug_log(&self) -> bool {
        self.flags & (1 << 10) != 0
    }

    /// Channel / status byte from the low byte of flags.
    pub fn channel_status(&self) -> u8 {
        (self.flags & 0xFF) as u8
    }
}

// ---------------------------------------------------------------------------
// Parsed data chunk
// ---------------------------------------------------------------------------

/// A parsed data chunk from the EP2 IN stream.
///
/// Each chunk has a header, optional start packet fields, sample data,
/// and optional stats trailer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataChunk {
    /// Chunk header (always present).
    pub header: ChunkHeader,
    /// Start packet fields (present when `header.is_start` is true).
    pub start: Option<StartPacketFields>,
    /// Raw sample data bytes.
    ///
    /// For digital channels: 8 samples per byte, 1 bit per sample.
    /// For analog channels: 2 bytes per sample (int16 LE).
    pub sample_data: Vec<u8>,
    /// Stats trailer (present when stats are enabled).
    pub stats: Option<StatsHeader>,
}

/// Parse a raw EP2 IN data buffer into a data chunk.
///
/// This parses a single chunk from the beginning of the buffer.
/// The caller must know the expected sample data size (typically
/// `samples_per_chunk * bytes_per_sample_row`) to correctly separate
/// sample data from the stats trailer.
///
/// # Arguments
///
/// - `data`: Raw bytes from EP2 IN bulk transfer.
/// - `expected_sample_bytes`: Expected size of the sample data portion.
///   If 0, all bytes after the header (and start packet) are treated as
///   sample data (no stats stripping).
/// - `has_stats`: Whether stats collection is enabled.
///
/// # Returns
///
/// The parsed `DataChunk` and the number of bytes consumed.
pub fn parse_data_chunk(
    data: &[u8],
    expected_sample_bytes: usize,
    has_stats: bool,
) -> ProtoResult<(DataChunk, usize)> {
    if data.len() < CHUNK_HEADER_SIZE {
        return Err(ProtoError::BufferTooShort {
            need: CHUNK_HEADER_SIZE,
            have: data.len(),
        });
    }

    let header = ChunkHeader::parse(data)?;
    let mut cursor = CHUNK_HEADER_SIZE;

    // Parse start packet if present.
    let start = if header.is_start {
        if data.len() < cursor + START_PACKET_EXTENSION_SIZE {
            return Err(ProtoError::BufferTooShort {
                need: cursor + START_PACKET_EXTENSION_SIZE,
                have: data.len(),
            });
        }
        let fields = StartPacketFields::parse(&data[cursor..])?;
        cursor += START_PACKET_EXTENSION_SIZE;
        Some(fields)
    } else {
        None
    };

    // Determine sample data boundaries.
    let _remaining = data.len() - cursor;

    let (sample_end, stats) = if has_stats && expected_sample_bytes > 0 {
        // Stats are appended after `expected_sample_bytes` of sample data.
        let sample_end = cursor + expected_sample_bytes;
        if sample_end > data.len() {
            // Not enough data for expected samples -- take what we have.
            (data.len(), None)
        } else {
            let stats_data = &data[sample_end..];
            let stats = if stats_data.len() >= STATS_HEADER_SIZE {
                Some(StatsHeader::parse(stats_data)?)
            } else {
                None
            };
            (sample_end, stats)
        }
    } else if expected_sample_bytes > 0 {
        let sample_end = (cursor + expected_sample_bytes).min(data.len());
        (sample_end, None)
    } else {
        // No expected size -- take all remaining as sample data.
        (data.len(), None)
    };

    let sample_data = data[cursor..sample_end].to_vec();

    // Total consumed: for stats case, consume stats too.
    let consumed = if stats.is_some() {
        let stats_end = sample_end + STATS_HEADER_SIZE;
        // If overflow meta present, skip that too.
        let stats_ref = stats.as_ref().unwrap();
        if stats_ref.has_overflow_meta() {
            (stats_end + OVERFLOW_METADATA_SIZE).min(data.len())
        } else {
            stats_end.min(data.len())
        }
    } else {
        sample_end
    };

    Ok((
        DataChunk {
            header,
            start,
            sample_data,
            stats,
        },
        consumed,
    ))
}

// ---------------------------------------------------------------------------
// Digital sample helpers
// ---------------------------------------------------------------------------

/// Extract individual digital samples from packed bytes.
///
/// Digital samples are packed 8 per byte, LSB first.
/// Bit 0 of byte 0 is the first sample.
///
/// # Arguments
///
/// - `data`: Packed digital sample bytes.
///
/// # Returns
///
/// A vector of boolean values (true = high, false = low).
pub fn unpack_digital_samples(data: &[u8]) -> Vec<bool> {
    let mut samples = Vec::with_capacity(data.len() * 8);
    for &byte in data {
        for bit in 0..8 {
            samples.push((byte >> bit) & 1 != 0);
        }
    }
    samples
}

/// Pack boolean digital samples into bytes (8 per byte, LSB first).
///
/// This is the inverse of [`unpack_digital_samples`].
pub fn pack_digital_samples(samples: &[bool]) -> Vec<u8> {
    let byte_count = (samples.len() + 7) / 8;
    let mut bytes = vec![0u8; byte_count];
    for (i, &sample) in samples.iter().enumerate() {
        if sample {
            bytes[i / 8] |= 1 << (i % 8);
        }
    }
    bytes
}

// ---------------------------------------------------------------------------
// Disaggregator
// ---------------------------------------------------------------------------

/// Disaggregate interleaved multi-channel digital data into per-channel
/// byte streams.
///
/// The FPGA outputs interleaved data where each byte contains one sample
/// per enabled channel, packed from LSB upward. For example, with 4
/// channels enabled, each byte contains:
///
/// ```text
/// bit 0 = channel 0 sample
/// bit 1 = channel 1 sample
/// bit 2 = channel 2 sample
/// bit 3 = channel 3 sample
/// bits 4-7 = next set of samples for channels 0-3
/// ```
///
/// Wait -- this is actually uncertain. The exact bit-to-channel mapping
/// depends on the FPGA implementation. Two common approaches:
///
/// 1. **Bit-interleaved within bytes**: Each byte contains samples from
///    multiple channels, alternating bits.
/// 2. **Byte-interleaved rows**: Each sample row has one byte per digital
///    channel group.
///
/// Based on the RE analysis (`channel_count == 2 || channel_count == 4`
/// and `rawdata.size() * 8 == encoding_state.num_samples`), the most
/// likely format is byte-interleaved rows where:
/// - Each row has `row_width` bytes
/// - The digital portion of each row is 1 byte (for 2 or 4 channels)
/// - Each digital byte has 8 samples packed for that sample period's
///   worth of channels
///
/// Since the exact format is uncertain, this disaggregator implements
/// the simplest byte-level approach: given N channels and interleaved
/// bytes where each byte has bits [0..N-1] for channels 0..N-1 at each
/// sample position, extract per-channel streams.
///
/// # Arguments
///
/// - `interleaved`: The interleaved byte stream from the disaggregator.
/// - `num_channels`: Number of enabled digital channels (must be 2 or 4).
///
/// # Returns
///
/// A vector of per-channel byte streams. Each output byte has 8 samples
/// for one channel.
///
/// # PLACEHOLDER
///
/// The exact interleaving format is not confirmed by USB capture.
/// This implementation assumes bit-interleaved within each byte:
/// for 4 channels, each input byte has 2 sample periods (4 bits each).
/// This will need revision once hardware captures are available.
pub fn disaggregate_digital(interleaved: &[u8], num_channels: usize) -> Vec<Vec<u8>> {
    assert!(
        num_channels == 2 || num_channels == 4,
        "num_channels must be 2 or 4, got {num_channels}"
    );

    // Samples per input byte: 8 bits / num_channels bits per sample period
    let samples_per_byte = 8 / num_channels;
    let total_samples = interleaved.len() * samples_per_byte;
    let output_bytes_per_channel = (total_samples + 7) / 8;

    let mut channels: Vec<Vec<bool>> = vec![Vec::with_capacity(total_samples); num_channels];

    for &byte in interleaved {
        // Each byte has `samples_per_byte` groups of `num_channels` bits.
        for sample in 0..samples_per_byte {
            let offset = sample * num_channels;
            for ch in 0..num_channels {
                let bit = (byte >> (offset + ch)) & 1 != 0;
                channels[ch].push(bit);
            }
        }
    }

    // Pack each channel's samples into bytes (8 per byte, LSB first).
    channels
        .into_iter()
        .map(|ch_samples| {
            let mut bytes = vec![0u8; output_bytes_per_channel];
            for (i, &sample) in ch_samples.iter().enumerate() {
                if sample {
                    bytes[i / 8] |= 1 << (i % 8);
                }
            }
            bytes
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // ChunkHeader tests
    // -------------------------------------------------------------------

    #[test]
    fn chunk_header_parse_basic() {
        // channel=2, software=false, is_start=false, config_id=0x0001
        let word: u32 = 0x0001_0002; // config_id=1, channel=2
        let data = word.to_le_bytes();
        let header = ChunkHeader::parse(&data).unwrap();
        assert_eq!(header.channel, 2);
        assert!(!header.software);
        assert!(!header.is_start);
        assert_eq!(header.config_id, 1);
    }

    #[test]
    fn chunk_header_parse_with_start_bit() {
        // channel=0, software=false, is_start=true, config_id=0x0042
        let word: u32 = (0x0042u32 << 16) | (1 << 9);
        let data = word.to_le_bytes();
        let header = ChunkHeader::parse(&data).unwrap();
        assert_eq!(header.channel, 0);
        assert!(!header.software);
        assert!(header.is_start);
        assert_eq!(header.config_id, 0x0042);
    }

    #[test]
    fn chunk_header_parse_with_software_flag() {
        // channel=3, software=true, is_start=false, config_id=0xABCD
        let word: u32 = (0xABCDu32 << 16) | (1 << 8) | 3;
        let data = word.to_le_bytes();
        let header = ChunkHeader::parse(&data).unwrap();
        assert_eq!(header.channel, 3);
        assert!(header.software);
        assert!(!header.is_start);
        assert_eq!(header.config_id, 0xABCD);
    }

    #[test]
    fn chunk_header_parse_all_flags() {
        // channel=1, software=true, is_start=true, config_id=0xFFFF
        let word: u32 = (0xFFFFu32 << 16) | (1 << 9) | (1 << 8) | 1;
        let data = word.to_le_bytes();
        let header = ChunkHeader::parse(&data).unwrap();
        assert_eq!(header.channel, 1);
        assert!(header.software);
        assert!(header.is_start);
        assert_eq!(header.config_id, 0xFFFF);
    }

    #[test]
    fn chunk_header_roundtrip() {
        let original = ChunkHeader {
            channel: 3,
            software: true,
            is_start: true,
            config_id: 0x1234,
        };
        let bytes = original.to_bytes();
        let parsed = ChunkHeader::parse(&bytes).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn chunk_header_parse_too_short() {
        let data = [0u8; 3];
        let err = ChunkHeader::parse(&data).unwrap_err();
        assert!(matches!(err, ProtoError::BufferTooShort { need: 4, have: 3 }));
    }

    // -------------------------------------------------------------------
    // StartPacketFields tests
    // -------------------------------------------------------------------

    #[test]
    fn start_packet_parse() {
        let timestamp: u32 = 0xDEAD_BEEF;
        let time: u64 = 0x0123_4567_89AB_CDEF;

        let mut data = [0u8; 12];
        data[..4].copy_from_slice(&timestamp.to_le_bytes());
        data[4..12].copy_from_slice(&time.to_le_bytes());

        let fields = StartPacketFields::parse(&data).unwrap();
        assert_eq!(fields.timestamp, timestamp);
        assert_eq!(fields.time, time);
    }

    #[test]
    fn start_packet_roundtrip() {
        let original = StartPacketFields {
            timestamp: 42,
            time: 1_250_000_000,
        };
        let bytes = original.to_bytes();
        let parsed = StartPacketFields::parse(&bytes).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn start_packet_too_short() {
        let data = [0u8; 11];
        let err = StartPacketFields::parse(&data).unwrap_err();
        assert!(matches!(
            err,
            ProtoError::BufferTooShort {
                need: 12,
                have: 11
            }
        ));
    }

    // -------------------------------------------------------------------
    // StatsHeader tests
    // -------------------------------------------------------------------

    #[test]
    fn stats_header_parse() {
        let mut data = [0u8; 20];

        // packet_id = 42
        data[0..4].copy_from_slice(&42u32.to_le_bytes());
        // timestamp_48 = 0x0000_FFEE_DDCC_BBAA (6 bytes)
        let ts: u64 = 0x0000_FFEE_DDCC_BBAA;
        let ts_bytes = ts.to_le_bytes();
        data[4..10].copy_from_slice(&ts_bytes[..6]);
        // flags = 0x0601 (overflow_meta=bit9, debug_log=bit10, channel_status=0x01)
        data[10..12].copy_from_slice(&0x0601u16.to_le_bytes());
        // host_time = 999
        data[12..20].copy_from_slice(&999u64.to_le_bytes());

        let stats = StatsHeader::parse(&data).unwrap();
        assert_eq!(stats.packet_id, 42);
        assert_eq!(stats.timestamp_48, ts);
        assert_eq!(stats.flags, 0x0601);
        assert_eq!(stats.host_time, 999);
        assert!(stats.has_overflow_meta()); // bit 9
        assert!(stats.debug_log()); // bit 10
        assert!(!stats.has_overflow_data()); // bit 12 not set
        assert_eq!(stats.channel_status(), 0x01);
    }

    #[test]
    fn stats_header_overflow_data_flag() {
        let mut data = [0u8; 20];
        // flags with bits 9 and 12 set = 0x1200
        data[10..12].copy_from_slice(&0x1200u16.to_le_bytes());

        let stats = StatsHeader::parse(&data).unwrap();
        assert!(stats.has_overflow_meta());
        assert!(stats.has_overflow_data());
    }

    #[test]
    fn stats_header_no_flags() {
        let data = [0u8; 20];
        let stats = StatsHeader::parse(&data).unwrap();
        assert!(!stats.has_overflow_meta());
        assert!(!stats.has_overflow_data());
        assert!(!stats.debug_log());
        assert_eq!(stats.channel_status(), 0);
    }

    #[test]
    fn stats_header_too_short() {
        let data = [0u8; 19];
        let err = StatsHeader::parse(&data).unwrap_err();
        assert!(matches!(
            err,
            ProtoError::BufferTooShort {
                need: 20,
                have: 19
            }
        ));
    }

    // -------------------------------------------------------------------
    // parse_data_chunk tests
    // -------------------------------------------------------------------

    #[test]
    fn parse_data_chunk_start_packet() {
        // Build a start packet: header (4) + timestamp (4) + time (8) = 16 bytes
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

        let mut data = Vec::new();
        data.extend_from_slice(&header.to_bytes());
        data.extend_from_slice(&start.to_bytes());

        let (chunk, consumed) = parse_data_chunk(&data, 0, false).unwrap();
        assert_eq!(consumed, START_PACKET_TOTAL_SIZE);
        assert_eq!(chunk.header, header);
        assert_eq!(chunk.start, Some(start));
        assert!(chunk.sample_data.is_empty());
        assert!(chunk.stats.is_none());
    }

    #[test]
    fn parse_data_chunk_with_sample_data() {
        let header = ChunkHeader {
            channel: 1,
            software: false,
            is_start: false,
            config_id: 5,
        };

        let sample_bytes = vec![0xAA, 0xBB, 0xCC, 0xDD];
        let mut data = Vec::new();
        data.extend_from_slice(&header.to_bytes());
        data.extend_from_slice(&sample_bytes);

        let (chunk, consumed) = parse_data_chunk(&data, 4, false).unwrap();
        assert_eq!(consumed, 8); // 4 header + 4 samples
        assert_eq!(chunk.header, header);
        assert!(chunk.start.is_none());
        assert_eq!(chunk.sample_data, sample_bytes);
        assert!(chunk.stats.is_none());
    }

    #[test]
    fn parse_data_chunk_with_stats() {
        let header = ChunkHeader {
            channel: 0,
            software: false,
            is_start: false,
            config_id: 10,
        };

        let sample_bytes = vec![0x11; 32];
        let mut stats_bytes = [0u8; STATS_HEADER_SIZE];
        stats_bytes[0..4].copy_from_slice(&7u32.to_le_bytes()); // packet_id

        let mut data = Vec::new();
        data.extend_from_slice(&header.to_bytes());
        data.extend_from_slice(&sample_bytes);
        data.extend_from_slice(&stats_bytes);

        let (chunk, consumed) = parse_data_chunk(&data, 32, true).unwrap();
        assert_eq!(consumed, 4 + 32 + STATS_HEADER_SIZE);
        assert_eq!(chunk.sample_data.len(), 32);
        assert!(chunk.stats.is_some());
        assert_eq!(chunk.stats.unwrap().packet_id, 7);
    }

    #[test]
    fn parse_data_chunk_too_short() {
        let data = [0u8; 3];
        let err = parse_data_chunk(&data, 0, false).unwrap_err();
        assert!(matches!(err, ProtoError::BufferTooShort { .. }));
    }

    #[test]
    fn parse_data_chunk_start_packet_too_short() {
        // Header says is_start but not enough data for start fields.
        let header = ChunkHeader {
            channel: 0,
            software: false,
            is_start: true,
            config_id: 1,
        };
        let data = header.to_bytes(); // only 4 bytes, need 16
        let err = parse_data_chunk(&data, 0, false).unwrap_err();
        assert!(matches!(err, ProtoError::BufferTooShort { .. }));
    }

    // -------------------------------------------------------------------
    // Digital sample packing/unpacking tests
    // -------------------------------------------------------------------

    #[test]
    fn unpack_digital_single_byte() {
        // 0b10110001 = bit0=1, bit1=0, bit2=0, bit3=0, bit4=1, bit5=1, bit6=0, bit7=1
        let data = [0b1011_0001u8];
        let samples = unpack_digital_samples(&data);
        assert_eq!(samples.len(), 8);
        assert_eq!(
            samples,
            vec![true, false, false, false, true, true, false, true]
        );
    }

    #[test]
    fn unpack_digital_all_high() {
        let data = [0xFF];
        let samples = unpack_digital_samples(&data);
        assert!(samples.iter().all(|&s| s));
    }

    #[test]
    fn unpack_digital_all_low() {
        let data = [0x00];
        let samples = unpack_digital_samples(&data);
        assert!(samples.iter().all(|&s| !s));
    }

    #[test]
    fn pack_unpack_roundtrip() {
        let original = vec![true, false, true, true, false, false, true, false];
        let packed = pack_digital_samples(&original);
        let unpacked = unpack_digital_samples(&packed);
        assert_eq!(unpacked, original);
    }

    #[test]
    fn pack_digital_partial_byte() {
        // 5 samples -- should produce 1 byte with 3 zero-padded bits
        let samples = vec![true, true, false, true, false];
        let packed = pack_digital_samples(&samples);
        assert_eq!(packed.len(), 1);
        // bits: 1,1,0,1,0,0,0,0 = 0b0000_1011 = 0x0B
        assert_eq!(packed[0], 0x0B);
    }

    #[test]
    fn unpack_digital_multiple_bytes() {
        let data = [0xFF, 0x00]; // 8 high, 8 low
        let samples = unpack_digital_samples(&data);
        assert_eq!(samples.len(), 16);
        for i in 0..8 {
            assert!(samples[i], "sample {i} should be high");
        }
        for i in 8..16 {
            assert!(!samples[i], "sample {i} should be low");
        }
    }

    // -------------------------------------------------------------------
    // Disaggregator tests
    // -------------------------------------------------------------------

    #[test]
    fn disaggregate_2_channels() {
        // With 2 channels, each byte has 4 sample periods.
        // Each sample period uses 2 bits: bit0=ch0, bit1=ch1.
        //
        // Input byte: bits [1:0]=period0, [3:2]=period1, [5:4]=period2, [7:6]=period3
        //
        // Example: 0b11_10_01_00
        //   period0: ch0=0, ch1=0
        //   period1: ch0=1, ch1=0
        //   period2: ch0=0, ch1=1
        //   period3: ch0=1, ch1=1
        let input = vec![0b11_10_01_00u8];
        let channels = disaggregate_digital(&input, 2);

        assert_eq!(channels.len(), 2);

        let ch0_samples = unpack_digital_samples(&channels[0]);
        let ch1_samples = unpack_digital_samples(&channels[1]);

        // We only have 4 samples per channel (padded to 8 in the output byte)
        assert_eq!(ch0_samples[0], false); // period0 ch0
        assert_eq!(ch0_samples[1], true); // period1 ch0
        assert_eq!(ch0_samples[2], false); // period2 ch0
        assert_eq!(ch0_samples[3], true); // period3 ch0

        assert_eq!(ch1_samples[0], false); // period0 ch1
        assert_eq!(ch1_samples[1], false); // period1 ch1
        assert_eq!(ch1_samples[2], true); // period2 ch1
        assert_eq!(ch1_samples[3], true); // period3 ch1
    }

    #[test]
    fn disaggregate_4_channels() {
        // With 4 channels, each byte has 2 sample periods.
        // Each period uses 4 bits: bit0=ch0, bit1=ch1, bit2=ch2, bit3=ch3.
        //
        // Input byte: bits [3:0]=period0, [7:4]=period1
        //
        // Example: 0b1010_0101
        //   period0 (bits 3:0 = 0101): ch0=1, ch1=0, ch2=1, ch3=0
        //   period1 (bits 7:4 = 1010): ch0=0, ch1=1, ch2=0, ch3=1
        let input = vec![0b1010_0101u8];
        let channels = disaggregate_digital(&input, 4);

        assert_eq!(channels.len(), 4);

        let ch0 = unpack_digital_samples(&channels[0]);
        let ch1 = unpack_digital_samples(&channels[1]);
        let ch2 = unpack_digital_samples(&channels[2]);
        let ch3 = unpack_digital_samples(&channels[3]);

        // 2 samples per channel
        assert_eq!(ch0[0], true); // period0
        assert_eq!(ch0[1], false); // period1

        assert_eq!(ch1[0], false);
        assert_eq!(ch1[1], true);

        assert_eq!(ch2[0], true);
        assert_eq!(ch2[1], false);

        assert_eq!(ch3[0], false);
        assert_eq!(ch3[1], true);
    }

    #[test]
    fn disaggregate_all_zeros() {
        let input = vec![0x00; 4];
        let channels = disaggregate_digital(&input, 4);
        for ch in &channels {
            assert!(unpack_digital_samples(ch).iter().all(|&s| !s));
        }
    }

    #[test]
    fn disaggregate_all_ones() {
        let input = vec![0xFF; 4];
        let channels = disaggregate_digital(&input, 4);
        for ch in &channels {
            // Each channel should have all high samples
            let samples = unpack_digital_samples(ch);
            for (i, &s) in samples.iter().enumerate() {
                if i < 4 * 2 {
                    // 4 input bytes * 2 samples/byte for 4 channels = 8 samples
                    assert!(s, "channel sample {i} should be high");
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "num_channels must be 2 or 4")]
    fn disaggregate_invalid_channel_count() {
        disaggregate_digital(&[0x00], 3);
    }

    #[test]
    fn disaggregate_multiple_bytes_4ch() {
        // Two input bytes with 4 channels -> 4 samples per channel.
        // Byte 0: 0b0101_0101 -> periods: ch0=1,ch1=0,ch2=1,ch3=0 | ch0=1,ch1=0,ch2=1,ch3=0
        // Byte 1: 0b1010_1010 -> periods: ch0=0,ch1=1,ch2=0,ch3=1 | ch0=0,ch1=1,ch2=0,ch3=1
        let input = vec![0b0101_0101, 0b1010_1010];
        let channels = disaggregate_digital(&input, 4);

        let ch0 = unpack_digital_samples(&channels[0]);
        // ch0 samples: 1, 1, 0, 0 (from byte0-period0, byte0-period1, byte1-period0, byte1-period1)
        assert_eq!(ch0[0], true);
        assert_eq!(ch0[1], true);
        assert_eq!(ch0[2], false);
        assert_eq!(ch0[3], false);
    }

    // -------------------------------------------------------------------
    // Integration-style tests
    // -------------------------------------------------------------------

    #[test]
    fn full_start_packet_parse() {
        // Construct a complete start packet as would appear on the wire.
        let header = ChunkHeader {
            channel: 2,
            software: false,
            is_start: true,
            config_id: 0x0042,
        };
        let start = StartPacketFields {
            timestamp: 1000,
            time: 1_250_000_000,
        };

        let mut wire = Vec::new();
        wire.extend_from_slice(&header.to_bytes());
        wire.extend_from_slice(&start.to_bytes());
        assert_eq!(wire.len(), START_PACKET_TOTAL_SIZE);

        let (chunk, consumed) = parse_data_chunk(&wire, 0, false).unwrap();
        assert_eq!(consumed, START_PACKET_TOTAL_SIZE);
        assert_eq!(chunk.header.channel, 2);
        assert!(chunk.header.is_start);
        assert_eq!(chunk.start.unwrap().timestamp, 1000);
        assert_eq!(chunk.start.unwrap().time, 1_250_000_000);
    }

    #[test]
    fn full_data_packet_with_stats() {
        let header = ChunkHeader {
            channel: 0,
            software: false,
            is_start: false,
            config_id: 1,
        };

        // 256 bytes of sample data (256 samples at 1 byte/sample for digital).
        let samples = vec![0xAA; 256];

        // Stats header.
        let mut stats_bytes = [0u8; STATS_HEADER_SIZE];
        stats_bytes[0..4].copy_from_slice(&42u32.to_le_bytes()); // packet_id=42

        let mut wire = Vec::new();
        wire.extend_from_slice(&header.to_bytes());
        wire.extend_from_slice(&samples);
        wire.extend_from_slice(&stats_bytes);

        let (chunk, consumed) = parse_data_chunk(&wire, 256, true).unwrap();
        assert_eq!(consumed, 4 + 256 + STATS_HEADER_SIZE);
        assert_eq!(chunk.sample_data.len(), 256);
        assert!(chunk.stats.is_some());
        assert_eq!(chunk.stats.unwrap().packet_id, 42);
    }
}
