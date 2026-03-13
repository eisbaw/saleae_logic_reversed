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

/// Sample data bytes per overflow block (256 samples * 2 bytes for analog).
pub const OVERFLOW_BLOCK_SAMPLE_SIZE: usize = 512;

/// Stats bytes per overflow block.
pub const OVERFLOW_BLOCK_STATS_SIZE: usize = 124;

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
        let timestamp_48 =
            u64::from_le_bytes([data[4], data[5], data[6], data[7], data[8], data[9], 0, 0]);

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
// Overflow metadata (16 bytes when stats flags bit 9 is set)
// ---------------------------------------------------------------------------

/// 16-byte overflow metadata that follows the mandatory stats header when
/// the `overflow_meta` flag (bit 9) is set.
///
/// Based on Ghidra decompilation of `ParseMsoWaveformData` (ELF VA 0x2d41510):
/// the parser stores `overflow_entry_size = 8` and saves the stream position,
/// then skips 16 bytes. With entry_size=8 and total=16, this is 2 entries.
///
/// The downstream consumer `PrintErrors` (ELF VA 0x1c1fcb0) checks boolean
/// error flags for: Master FIFO overflow, Digital FIFO overflow, Analog FIFO
/// overflow, analog FSM error, digital FSM error, bitslip 1 error, bitslip 2
/// error. The `cur_overflow != prev_overflow` comparison triggers error
/// logging.
///
/// # Confidence
///
/// The 16-byte size and 2x8-byte entry structure are **High confidence**
/// (direct decompilation). The interpretation as prev/cur overflow flags
/// is **Hypothesis** -- the parser skips these bytes without field-level
/// parsing. The flag bit positions within each u64 are inferred from
/// PrintErrors but not confirmed.
///
/// # Wire format
///
/// ```text
/// Offset  Size  Description
/// 0x00    8     Previous overflow flags (u64, LE) -- hypothesis
/// 0x08    8     Current overflow flags (u64, LE) -- hypothesis
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverflowMetadata {
    /// Previous overflow flags / counter (u64 LE).
    ///
    /// Hypothesized to be a bitfield of FIFO overflow and FSM error flags.
    pub prev_overflow: u64,
    /// Current overflow flags / counter (u64 LE).
    ///
    /// When `cur_overflow != prev_overflow`, the host detects overflow.
    pub cur_overflow: u64,
}

impl OverflowMetadata {
    /// Parse 16 bytes of overflow metadata.
    pub fn parse(data: &[u8]) -> ProtoResult<Self> {
        if data.len() < OVERFLOW_METADATA_SIZE {
            return Err(ProtoError::BufferTooShort {
                need: OVERFLOW_METADATA_SIZE,
                have: data.len(),
            });
        }

        let prev_overflow = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);
        let cur_overflow = u64::from_le_bytes([
            data[8], data[9], data[10], data[11], data[12], data[13], data[14], data[15],
        ]);

        Ok(Self {
            prev_overflow,
            cur_overflow,
        })
    }

    /// Encode as 16 bytes (prev LE + cur LE).
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[..8].copy_from_slice(&self.prev_overflow.to_le_bytes());
        out[8..16].copy_from_slice(&self.cur_overflow.to_le_bytes());
        out
    }

    /// Whether overflow was detected (cur differs from prev).
    pub fn overflow_detected(&self) -> bool {
        self.cur_overflow != self.prev_overflow
    }
}

// ---------------------------------------------------------------------------
// Overflow error flags (hypothesized bit positions within u64)
// ---------------------------------------------------------------------------

/// Hypothesized overflow error flag bit positions within the overflow
/// metadata u64 values.
///
/// Derived from `PrintErrors` (ELF VA 0x1c1fcb0) which checks a boolean
/// array at offsets [0], [1], [2], [3], [4], [9], [10]. The boolean array
/// is likely derived from the overflow flags bitfield.
///
/// # Confidence: Hypothesis
///
/// The error names and their existence are **High confidence** (string
/// literals in the binary). The mapping to specific bit positions is
/// **Hypothesis** -- PrintErrors takes a boolean array, not a bitfield
/// directly, so the unpacking logic is elsewhere.
pub mod overflow_flags {
    /// Master FIFO overflow (PrintErrors param[0]).
    pub const MASTER_FIFO: u64 = 1 << 0;
    /// Digital FIFO overflow (PrintErrors param[1]).
    pub const DIGITAL_FIFO: u64 = 1 << 1;
    /// Analog FIFO overflow (PrintErrors param[2]).
    pub const ANALOG_FIFO: u64 = 1 << 2;
    /// Analog FSM error (PrintErrors param[3]).
    pub const ANALOG_FSM: u64 = 1 << 3;
    /// Digital FSM error (PrintErrors param[4]).
    pub const DIGITAL_FSM: u64 = 1 << 4;
    /// Bitslip 1 error (PrintErrors param[9]).
    pub const BITSLIP_1: u64 = 1 << 9;
    /// Bitslip 2 error (PrintErrors param[10]).
    pub const BITSLIP_2: u64 = 1 << 10;
}

// ---------------------------------------------------------------------------
// Overflow block stats (124 bytes per overflow data block)
// ---------------------------------------------------------------------------

/// Per-block stats within an overflow data block (124 bytes).
///
/// Each overflow block is 636 bytes total: 512 bytes of sample data
/// followed by 124 bytes of stats. The first 20 bytes of the stats
/// have the same layout as the mandatory `StatsHeader`. The remaining
/// 104 bytes contain additional per-block metadata.
///
/// # Confidence
///
/// The 124-byte size is **High confidence** (decompilation shows
/// `n_blocks * 0x7c` and `skip(stream, n_blocks * 124)`). The internal
/// layout is **Hypothesis** -- the parser skips the stats without
/// field-level parsing. We parse the first 20 bytes as a `StatsHeader`
/// and preserve the remaining 104 bytes as raw data for future analysis.
///
/// # Wire format (hypothesized)
///
/// ```text
/// Offset  Size  Description
/// 0x00    20    StatsHeader (same format as mandatory stats header)
/// 0x14    104   Additional per-block metadata (unknown layout)
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverflowBlockStats {
    /// Stats header (first 20 bytes, same format as mandatory stats).
    pub header: StatsHeader,
    /// Raw remaining bytes (104 bytes of unknown per-block metadata).
    ///
    /// Preserved verbatim for future analysis. The internal structure
    /// is not yet determined from decompilation.
    pub extra: [u8; 104],
}

impl OverflowBlockStats {
    /// Parse 124 bytes of per-block overflow stats.
    pub fn parse(data: &[u8]) -> ProtoResult<Self> {
        if data.len() < OVERFLOW_BLOCK_STATS_SIZE {
            return Err(ProtoError::BufferTooShort {
                need: OVERFLOW_BLOCK_STATS_SIZE,
                have: data.len(),
            });
        }

        let header = StatsHeader::parse(data)?;

        let mut extra = [0u8; 104];
        extra.copy_from_slice(&data[STATS_HEADER_SIZE..OVERFLOW_BLOCK_STATS_SIZE]);

        Ok(Self { header, extra })
    }
}

// ---------------------------------------------------------------------------
// Parsed overflow block (512 sample bytes + 124 stats bytes)
// ---------------------------------------------------------------------------

/// One overflow data block from the EP2 stream.
///
/// When both `overflow_meta` (bit 9) and `overflow_data` (bit 12) flags
/// are set in the stats header, the remaining bytes after the 16-byte
/// overflow metadata are organized as N blocks of 636 bytes each.
///
/// Each block contains 256 analog samples (512 bytes at 2 bytes/sample)
/// followed by 124 bytes of per-block stats.
///
/// # Wire format
///
/// ```text
/// | 512 bytes sample data | 124 bytes stats |  = 636 bytes per block
/// ```
///
/// The number of blocks is `remaining_bytes / 636`. If `remaining_bytes`
/// is not evenly divisible by 636, the parser throws
/// `"Invalid MSO waveform received, invalid data length"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverflowBlock {
    /// Raw sample data (512 bytes = 256 analog samples at 2 bytes each).
    pub sample_data: Vec<u8>,
    /// Per-block stats (124 bytes).
    pub stats: OverflowBlockStats,
}

// ---------------------------------------------------------------------------
// Parsed data chunk
// ---------------------------------------------------------------------------

/// A parsed data chunk from the EP2 IN stream.
///
/// Each chunk has a header, optional start packet fields, sample data,
/// optional stats trailer, and optional overflow data.
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
    /// Overflow metadata (present when stats flag bit 9 is set).
    pub overflow_meta: Option<OverflowMetadata>,
    /// Overflow data blocks (present when stats flag bits 9 AND 12 are set).
    pub overflow_blocks: Vec<OverflowBlock>,
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

    // Parse overflow data after stats header.
    let (overflow_meta, overflow_blocks, consumed) = if let Some(ref stats_ref) = stats {
        let mut pos = sample_end + STATS_HEADER_SIZE;

        // Parse overflow metadata (16 bytes) when bit 9 is set.
        let overflow_meta = if stats_ref.has_overflow_meta() && pos + OVERFLOW_METADATA_SIZE <= data.len() {
            let meta = OverflowMetadata::parse(&data[pos..])?;
            pos += OVERFLOW_METADATA_SIZE;
            Some(meta)
        } else {
            None
        };

        // Parse overflow data blocks when bits 9 AND 12 are set.
        let overflow_blocks = if stats_ref.has_overflow_data() && overflow_meta.is_some() {
            let remaining = data.len().saturating_sub(pos);
            if remaining > 0 && remaining % OVERFLOW_BLOCK_SIZE != 0 {
                return Err(ProtoError::InvalidData {
                    reason: format!(
                        "overflow data length {} not divisible by block size {}",
                        remaining, OVERFLOW_BLOCK_SIZE,
                    ),
                });
            }
            let n_blocks = remaining / OVERFLOW_BLOCK_SIZE;
            let mut blocks = Vec::with_capacity(n_blocks);
            for _ in 0..n_blocks {
                let block_sample = data[pos..pos + OVERFLOW_BLOCK_SAMPLE_SIZE].to_vec();
                pos += OVERFLOW_BLOCK_SAMPLE_SIZE;
                let block_stats = OverflowBlockStats::parse(&data[pos..])?;
                pos += OVERFLOW_BLOCK_STATS_SIZE;
                blocks.push(OverflowBlock {
                    sample_data: block_sample,
                    stats: block_stats,
                });
            }
            blocks
        } else {
            Vec::new()
        };

        (overflow_meta, overflow_blocks, pos.min(data.len()))
    } else {
        (None, Vec::new(), sample_end)
    };

    Ok((
        DataChunk {
            header,
            start,
            sample_data,
            stats,
            overflow_meta,
            overflow_blocks,
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
    let byte_count = samples.len().div_ceil(8);
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
    let output_bytes_per_channel = total_samples.div_ceil(8);

    let mut channels: Vec<Vec<bool>> = vec![Vec::with_capacity(total_samples); num_channels];

    for &byte in interleaved {
        // Each byte has `samples_per_byte` groups of `num_channels` bits.
        for sample in 0..samples_per_byte {
            let offset = sample * num_channels;
            for (ch, ch_vec) in channels.iter_mut().enumerate() {
                let bit = (byte >> (offset + ch)) & 1 != 0;
                ch_vec.push(bit);
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
// Analog sample helpers
// ---------------------------------------------------------------------------

/// Disaggregate interleaved multi-channel analog data into per-channel
/// sample streams.
///
/// Analog samples are 16-bit signed integers (int16, little-endian), 2
/// bytes each. With N analog channels enabled, every `2*N` bytes in the
/// interleaved stream contain one sample per channel, in channel order
/// (channel 0 first, then channel 1, etc.).
///
/// # Arguments
///
/// - `data`: The interleaved byte stream from EP2 IN.
/// - `num_channels`: Number of enabled analog channels (1-8 for Pro 8).
///
/// # Returns
///
/// A vector of per-channel sample vectors. `result[ch]` contains all
/// samples for channel `ch`.
///
/// # Panics
///
/// Panics if `num_channels` is 0.
///
/// # Note
///
/// Trailing bytes that do not form a complete sample row (2 * num_channels
/// bytes) are silently discarded.
pub fn disaggregate_analog(data: &[u8], num_channels: usize) -> Vec<Vec<i16>> {
    assert!(num_channels > 0, "num_channels must be > 0");

    let row_size = 2 * num_channels;
    let num_rows = data.len() / row_size;

    let mut channels: Vec<Vec<i16>> = vec![Vec::with_capacity(num_rows); num_channels];

    for row_idx in 0..num_rows {
        let row_start = row_idx * row_size;
        for (ch, ch_vec) in channels.iter_mut().enumerate() {
            let offset = row_start + ch * 2;
            let sample = i16::from_le_bytes([data[offset], data[offset + 1]]);
            ch_vec.push(sample);
        }
    }

    channels
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
        assert!(matches!(
            err,
            ProtoError::BufferTooShort { need: 4, have: 3 }
        ));
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
            ProtoError::BufferTooShort { need: 12, have: 11 }
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
            ProtoError::BufferTooShort { need: 20, have: 19 }
        ));
    }

    // -------------------------------------------------------------------
    // OverflowMetadata tests
    // -------------------------------------------------------------------

    #[test]
    fn overflow_metadata_parse() {
        let mut data = [0u8; 16];
        data[..8].copy_from_slice(&42u64.to_le_bytes()); // prev
        data[8..16].copy_from_slice(&99u64.to_le_bytes()); // cur

        let meta = OverflowMetadata::parse(&data).unwrap();
        assert_eq!(meta.prev_overflow, 42);
        assert_eq!(meta.cur_overflow, 99);
        assert!(meta.overflow_detected());
    }

    #[test]
    fn overflow_metadata_no_change() {
        let mut data = [0u8; 16];
        data[..8].copy_from_slice(&7u64.to_le_bytes());
        data[8..16].copy_from_slice(&7u64.to_le_bytes());

        let meta = OverflowMetadata::parse(&data).unwrap();
        assert!(!meta.overflow_detected());
    }

    #[test]
    fn overflow_metadata_roundtrip() {
        let original = OverflowMetadata {
            prev_overflow: 0xDEAD_BEEF_CAFE_BABE,
            cur_overflow: 0x0123_4567_89AB_CDEF,
        };
        let bytes = original.to_bytes();
        let parsed = OverflowMetadata::parse(&bytes).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn overflow_metadata_too_short() {
        let data = [0u8; 15];
        let err = OverflowMetadata::parse(&data).unwrap_err();
        assert!(matches!(
            err,
            ProtoError::BufferTooShort { need: 16, have: 15 }
        ));
    }

    // -------------------------------------------------------------------
    // OverflowBlockStats tests
    // -------------------------------------------------------------------

    #[test]
    fn overflow_block_stats_parse() {
        let mut data = [0u8; 124];
        // Set packet_id in the embedded stats header.
        data[0..4].copy_from_slice(&55u32.to_le_bytes());
        // Set some recognizable pattern in the extra bytes.
        data[20] = 0xAA;
        data[123] = 0xBB;

        let block_stats = OverflowBlockStats::parse(&data).unwrap();
        assert_eq!(block_stats.header.packet_id, 55);
        assert_eq!(block_stats.extra[0], 0xAA);
        assert_eq!(block_stats.extra[103], 0xBB);
    }

    #[test]
    fn overflow_block_stats_too_short() {
        let data = [0u8; 123];
        let err = OverflowBlockStats::parse(&data).unwrap_err();
        assert!(matches!(
            err,
            ProtoError::BufferTooShort {
                need: 124,
                have: 123
            }
        ));
    }

    // -------------------------------------------------------------------
    // parse_data_chunk with overflow tests
    // -------------------------------------------------------------------

    #[test]
    fn parse_data_chunk_with_overflow_meta() {
        let header = ChunkHeader {
            channel: 0,
            software: false,
            is_start: false,
            config_id: 1,
        };

        let sample_bytes = vec![0x11; 32];

        // Stats with overflow_meta flag (bit 9) set.
        let mut stats_bytes = [0u8; STATS_HEADER_SIZE];
        stats_bytes[0..4].copy_from_slice(&7u32.to_le_bytes());
        stats_bytes[10..12].copy_from_slice(&0x0200u16.to_le_bytes()); // bit 9

        // Overflow metadata: 16 bytes.
        let mut meta_bytes = [0u8; 16];
        meta_bytes[..8].copy_from_slice(&10u64.to_le_bytes());
        meta_bytes[8..].copy_from_slice(&20u64.to_le_bytes());

        let mut data = Vec::new();
        data.extend_from_slice(&header.to_bytes());
        data.extend_from_slice(&sample_bytes);
        data.extend_from_slice(&stats_bytes);
        data.extend_from_slice(&meta_bytes);

        let (chunk, consumed) = parse_data_chunk(&data, 32, true).unwrap();
        assert_eq!(consumed, 4 + 32 + 20 + 16);
        assert_eq!(chunk.sample_data.len(), 32);
        assert!(chunk.stats.is_some());
        assert!(chunk.stats.as_ref().unwrap().has_overflow_meta());
        assert!(!chunk.stats.as_ref().unwrap().has_overflow_data());

        let meta = chunk.overflow_meta.unwrap();
        assert_eq!(meta.prev_overflow, 10);
        assert_eq!(meta.cur_overflow, 20);
        assert!(meta.overflow_detected());
        assert!(chunk.overflow_blocks.is_empty());
    }

    #[test]
    fn parse_data_chunk_with_overflow_blocks() {
        let header = ChunkHeader {
            channel: 0,
            software: false,
            is_start: false,
            config_id: 1,
        };

        let sample_bytes = vec![0x11; 32];

        // Stats with overflow_meta (bit 9) AND overflow_data (bit 12) set.
        let mut stats_bytes = [0u8; STATS_HEADER_SIZE];
        stats_bytes[0..4].copy_from_slice(&1u32.to_le_bytes());
        stats_bytes[10..12].copy_from_slice(&0x1200u16.to_le_bytes()); // bits 9+12

        // Overflow metadata: 16 bytes.
        let meta_bytes = [0u8; 16];

        // One overflow block: 512 sample + 124 stats = 636 bytes.
        let mut block_sample = vec![0xCC; 512];
        block_sample[0] = 0xDD; // recognizable
        let mut block_stats = [0u8; 124];
        block_stats[0..4].copy_from_slice(&99u32.to_le_bytes()); // packet_id

        let mut data = Vec::new();
        data.extend_from_slice(&header.to_bytes());
        data.extend_from_slice(&sample_bytes);
        data.extend_from_slice(&stats_bytes);
        data.extend_from_slice(&meta_bytes);
        data.extend_from_slice(&block_sample);
        data.extend_from_slice(&block_stats);

        let (chunk, consumed) = parse_data_chunk(&data, 32, true).unwrap();
        assert_eq!(consumed, data.len());
        assert_eq!(chunk.overflow_blocks.len(), 1);
        assert_eq!(chunk.overflow_blocks[0].sample_data[0], 0xDD);
        assert_eq!(chunk.overflow_blocks[0].stats.header.packet_id, 99);
    }

    #[test]
    fn parse_data_chunk_overflow_invalid_length() {
        let header = ChunkHeader {
            channel: 0,
            software: false,
            is_start: false,
            config_id: 1,
        };

        let sample_bytes = vec![0x11; 32];

        // Stats with bits 9+12 set.
        let mut stats_bytes = [0u8; STATS_HEADER_SIZE];
        stats_bytes[10..12].copy_from_slice(&0x1200u16.to_le_bytes());

        let meta_bytes = [0u8; 16];

        // Invalid: 100 bytes is not divisible by 636.
        let bad_overflow = vec![0u8; 100];

        let mut data = Vec::new();
        data.extend_from_slice(&header.to_bytes());
        data.extend_from_slice(&sample_bytes);
        data.extend_from_slice(&stats_bytes);
        data.extend_from_slice(&meta_bytes);
        data.extend_from_slice(&bad_overflow);

        let err = parse_data_chunk(&data, 32, true).unwrap_err();
        assert!(matches!(err, ProtoError::InvalidData { .. }));
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
        for (i, &s) in samples.iter().enumerate() {
            if i < 8 {
                assert!(s, "sample {i} should be high");
            } else {
                assert!(!s, "sample {i} should be low");
            }
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
        assert!(!ch0_samples[0]); // period0 ch0
        assert!(ch0_samples[1]); // period1 ch0
        assert!(!ch0_samples[2]); // period2 ch0
        assert!(ch0_samples[3]); // period3 ch0

        assert!(!ch1_samples[0]); // period0 ch1
        assert!(!ch1_samples[1]); // period1 ch1
        assert!(ch1_samples[2]); // period2 ch1
        assert!(ch1_samples[3]); // period3 ch1
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
        assert!(ch0[0]); // period0
        assert!(!ch0[1]); // period1

        assert!(!ch1[0]);
        assert!(ch1[1]);

        assert!(ch2[0]);
        assert!(!ch2[1]);

        assert!(!ch3[0]);
        assert!(ch3[1]);
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
        assert!(ch0[0]);
        assert!(ch0[1]);
        assert!(!ch0[2]);
        assert!(!ch0[3]);
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

    // -------------------------------------------------------------------
    // Analog disaggregation tests
    // -------------------------------------------------------------------

    #[test]
    fn disaggregate_analog_single_channel() {
        // 3 samples for 1 channel: [100, -200, 2047]
        let mut data = Vec::new();
        data.extend_from_slice(&100i16.to_le_bytes());
        data.extend_from_slice(&(-200i16).to_le_bytes());
        data.extend_from_slice(&2047i16.to_le_bytes());

        let channels = disaggregate_analog(&data, 1);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0], vec![100, -200, 2047]);
    }

    #[test]
    fn disaggregate_analog_two_channels() {
        // 2 sample rows, 2 channels: row0=[100, -100], row1=[200, -200]
        let mut data = Vec::new();
        // Row 0
        data.extend_from_slice(&100i16.to_le_bytes());
        data.extend_from_slice(&(-100i16).to_le_bytes());
        // Row 1
        data.extend_from_slice(&200i16.to_le_bytes());
        data.extend_from_slice(&(-200i16).to_le_bytes());

        let channels = disaggregate_analog(&data, 2);
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0], vec![100, 200]);
        assert_eq!(channels[1], vec![-100, -200]);
    }

    #[test]
    fn disaggregate_analog_eight_channels() {
        // 1 sample row, 8 channels: [0, 1, 2, 3, 4, 5, 6, 7]
        let mut data = Vec::new();
        for ch in 0..8i16 {
            data.extend_from_slice(&ch.to_le_bytes());
        }

        let channels = disaggregate_analog(&data, 8);
        assert_eq!(channels.len(), 8);
        for (ch, ch_data) in channels.iter().enumerate() {
            assert_eq!(*ch_data, vec![ch as i16]);
        }
    }

    #[test]
    fn disaggregate_analog_boundary_values() {
        // Test ADC code boundaries: -2048 (min) and 2047 (max) for 12-bit
        let mut data = Vec::new();
        data.extend_from_slice(&(-2048i16).to_le_bytes());
        data.extend_from_slice(&2047i16.to_le_bytes());

        let channels = disaggregate_analog(&data, 2);
        assert_eq!(channels[0], vec![-2048]);
        assert_eq!(channels[1], vec![2047]);
    }

    #[test]
    fn disaggregate_analog_trailing_bytes_discarded() {
        // 2 channels = 4 bytes per row. 5 bytes -> 1 row, 1 byte discarded.
        let mut data = Vec::new();
        data.extend_from_slice(&42i16.to_le_bytes());
        data.extend_from_slice(&(-42i16).to_le_bytes());
        data.push(0xFF); // trailing byte

        let channels = disaggregate_analog(&data, 2);
        assert_eq!(channels[0], vec![42]);
        assert_eq!(channels[1], vec![-42]);
    }

    #[test]
    fn disaggregate_analog_empty_input() {
        let channels = disaggregate_analog(&[], 2);
        assert_eq!(channels.len(), 2);
        assert!(channels[0].is_empty());
        assert!(channels[1].is_empty());
    }

    #[test]
    #[should_panic(expected = "num_channels must be > 0")]
    fn disaggregate_analog_zero_channels_panics() {
        disaggregate_analog(&[0, 0], 0);
    }
}
