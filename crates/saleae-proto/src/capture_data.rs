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
//! confidence** (disassembly at VA 0x2d41510). Stats flag bit names are
//! derived from the log format string in `HandleWaveformData` (VA 0x2d44b80),
//! giving **High confidence** for bits 8 (end), 9 (start), 10 (software),
//! 14 (prev_overflow), 15 (cur_overflow). Bit 11 is extracted but unused
//! in known callers. Bit 13 purpose is unclear (Low confidence).
//! See [`stats_flags`] module for details.

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
///
/// # config_id semantics
///
/// The `config_id` is a **monotonically incrementing counter** assigned by
/// `MsoOscilloscopeInstrument::SetConfiguration()` (ELF VA 0x2d477d0).
/// Each call to `SetConfiguration` increments a counter at object offset
/// 0x1f0 by 1 and returns the new value as `ConfigurationId`.
///
/// On MSO-class FPGAs, the host writes the config_id to FPGA registers
/// `SCOPE_CONFIG_ID_0..3` (addresses 0x0044-0x0050) via
/// `SetConfigurationIds` (ELF VA 0x2d64230). The FPGA then embeds this
/// value in bits [31:16] of every chunk header in the EP2 data stream.
///
/// On Logic2FpgaDevice (Spartan-6, used by Logic Pro 8), the 8-bit
/// register interface does **not** include config_id registers.
/// `SetCaptureParameters` writes registers 0x06-0x14 and 0x1A-0x1B but
/// no config_id. The config_id in chunk headers from these devices is
/// expected to be 0 (or a fixed value from the FPGA bitstream).
///
/// The host compares each packet's config_id against a stored "current"
/// value. On mismatch it logs a warning ("Received start digital data for
/// old configuration id") but does **not** drop the packet.
///
/// **Confidence: High** -- decompiled `SetConfiguration` (counter at
/// offset 0x1f0), `SetConfigurationIds` (FPGA register writes), and
/// `DigitalCreatorNode::HandleData` (comparison + warning log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkHeader {
    /// Channel number (0-3).
    pub channel: u8,
    /// Software-generated data flag.
    pub software: bool,
    /// When true, start packet fields (timestamp + time) follow.
    pub is_start: bool,
    /// Configuration ID for stream synchronization.
    ///
    /// Monotonically incrementing counter set by the host at capture
    /// start. On MSO FPGAs, written to SCOPE_CONFIG_ID registers
    /// (0x0044-0x0050). On Spartan-6 (Logic Pro 8), this field is
    /// expected to be 0 since no config_id registers exist in the
    /// 8-bit register space.
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
/// The host rejects data that arrives without a preceding start packet
/// ("Received digital data without ever receiving a start packet").
///
/// # Emission timing
///
/// Start packets are emitted **once per channel at the beginning of each
/// capture** (or configuration change). Each enabled channel gets its own
/// start packet with `is_start=1` in the chunk header.
///
/// If a second start packet arrives for a channel that already has one,
/// the host checks time ordering ("Jumped back in time - New start time
/// starts before old end time") and resets the per-channel state:
/// time_reference, timestamp/time, config_id, and sample counter.
///
/// The per-channel state layout (stride 0x30 bytes, base offset 0x4d8
/// in `DigitalCreatorNode`):
///
/// ```text
/// +0x00  f64  time_reference (initialized to 1.25e9)
/// +0x08  16B  start timestamp + time (copied from start packet)
/// +0x18  u32  config_id (from chunk header)
/// +0x20  u64  sample_counter (reset to 0)
/// +0x28  bool has_start_packet
/// ```
///
/// **Confidence: High** -- decompiled `DigitalCreatorNode::HandleData`
/// (ELF VA 0x2d51710), log strings, per-channel state layout.
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
// Stats header flag bit constants
// ---------------------------------------------------------------------------

/// Stats header flags bitfield (u16) bit positions and accessors.
///
/// Derived from decompilation of `ParseMsoWaveformData` (ELF VA 0x2d41510)
/// and its caller `HandleWaveformData` (ELF VA 0x2d44b80).
///
/// The high byte of the flags u16 is unpacked into 8 individual boolean
/// fields by the binary. The low byte is a channel/status value.
///
/// ## Bit layout
///
/// ```text
/// Bit   Parser output    Log field name     Confidence
/// ----  ---------------  -----------------  ----------
///  [7:0]  channel_status   (low byte)        High
///  8      param_1[7]       "end"             High (log string literal)
///  9      param_1[6]       "start"           High (log string literal)
/// 10      param_1[5]       "software"        High (log string literal)
/// 11      param_1[4]       (unused)          Medium (extracted but never read)
/// 12      param_1[3]       (none in log)     High (controls overflow block parsing)
/// 13      param_1[2]       (none in log)     Low (inverted and stored, purpose unclear)
/// 14      param_1[0]       "prev_overflow"   High (log string literal)
/// 15      param_1[1]       "cur_overflow"    High (log string literal)
/// ```
///
/// ## Evidence
///
/// The log format string in `HandleWaveformData` (line ~0x287 of
/// `oscilloscope_instrument.cpp`) is:
/// ```text
/// "id={}, start={}, end={}, sample_count={}, config_id={}, ts={},
///  software={}, cur_overflow={}, prev_overflow={}, samples={}"
/// ```
/// Each `{}` maps to a specific flag bit via typed pointer arguments
/// to the logging function `FUN_02e4cef0`.
///
/// ## Relationship to existing names
///
/// - "start" (bit 9) was previously labeled `overflow_meta`. The parser
///   reads 16 bytes of overflow metadata when this bit is set, but the
///   Saleae developers named the flag "start" (start of waveform segment).
///   Both names describe the same behavior; we adopt the developer's name.
///
/// - "software" (bit 10) was previously labeled `debug_log`. The caller
///   uses this flag to gate detailed logging AND to enter the software
///   trigger path. The developer's log names it "software".
///
/// - "end" (bit 8) was previously unidentified.
///
/// - "cur_overflow" / "prev_overflow" (bits 15/14) were previously
///   unidentified. These are single-bit booleans, not counters.
pub mod stats_flags {
    /// Bit 8: end-of-waveform segment marker.
    ///
    /// When set, indicates this chunk is the last in the current waveform
    /// segment. In `HandleWaveformData`, stored as a per-channel status flag
    /// and used to gate end-of-waveform processing.
    ///
    /// **Confidence: High** (log string literal "end").
    pub const END: u16 = 1 << 8;

    /// Bit 9: start-of-waveform segment marker.
    ///
    /// When set, indicates this chunk begins a new waveform segment.
    /// The parser reads 16 bytes of overflow metadata (prev/cur overflow
    /// state) when this flag is set. In `HandleWaveformData`, used to
    /// initialize per-channel waveform tracking state.
    ///
    /// **Confidence: High** (log string literal "start").
    pub const START: u16 = 1 << 9;

    /// Bit 10: software-triggered waveform flag.
    ///
    /// When set, indicates this waveform was generated by a software
    /// trigger (as opposed to a hardware/freerunning trigger). The caller
    /// uses this combined with `START` to enter the software trigger
    /// handling path, and also to gate detailed per-waveform logging.
    ///
    /// **Confidence: High** (log string literal "software").
    pub const SOFTWARE: u16 = 1 << 10;

    /// Bit 11: reserved / unknown.
    ///
    /// Extracted by the parser (`param_1[4] = bVar4 >> 3 & 1`) but never
    /// accessed by `HandleWaveformData`. Possibly reserved for future use
    /// or used by other callers not yet decompiled.
    ///
    /// **Confidence: Medium** (extracted but unused in known callers).
    pub const RESERVED_11: u16 = 1 << 11;

    /// Bit 12: overflow data blocks present.
    ///
    /// When set (along with `START`/bit 9), the remaining data after the
    /// 16-byte overflow metadata is structured as N blocks of 636 bytes
    /// each (512 sample bytes + 124 stats bytes). Without this flag, the
    /// remaining data is plain sample bytes.
    ///
    /// In `HandleWaveformData`, also checked in the software trigger path
    /// where it gates the "empty software trigger" condition.
    ///
    /// **Confidence: High** (controls parser branch for overflow blocks).
    pub const OVERFLOW_DATA: u16 = 1 << 12;

    /// Bit 13: unknown purpose.
    ///
    /// In `HandleWaveformData`, this bit is inverted (`!bit13`) and stored
    /// in a waveform chunk descriptor struct that gets pushed to a
    /// processing queue. The inversion suggests it may mean "has more
    /// chunks" (0 = last chunk, 1 = continuation), but this is speculative.
    ///
    /// **Confidence: Low** (inverted and stored, no clear semantic from
    /// decompilation alone).
    pub const UNKNOWN_13: u16 = 1 << 13;

    /// Bit 14: previous overflow state.
    ///
    /// Single-bit boolean indicating the overflow state from the previous
    /// waveform chunk. Used together with `CUR_OVERFLOW` (bit 15) for
    /// overflow change detection.
    ///
    /// **Confidence: High** (log string literal "prev_overflow").
    pub const PREV_OVERFLOW: u16 = 1 << 14;

    /// Bit 15: current overflow state.
    ///
    /// Single-bit boolean indicating the current overflow state.
    /// In `HandleWaveformData`, when this is set in the software trigger
    /// path, triggers the "Received an empty software trigger" condition.
    ///
    /// **Confidence: High** (log string literal "cur_overflow").
    pub const CUR_OVERFLOW: u16 = 1 << 15;
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
/// 0x0a    2     Flags bitfield (u16, LE) -- see [`stats_flags`]
/// 0x0c    8     Host/graph time (u64, LE)
/// ```
///
/// The flags bitfield layout is documented in the [`stats_flags`] module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsHeader {
    /// Sequential packet counter.
    pub packet_id: u32,
    /// 48-bit FPGA timestamp (upper 16 bits are zero).
    pub timestamp_48: u64,
    /// Raw flags bitfield. See [`stats_flags`] for bit definitions.
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

    // -- Flag bit accessors (developer-named, from log format string) --

    /// Whether this chunk marks the end of a waveform segment (bit 8).
    ///
    /// **Confidence: High** (log string literal "end").
    pub fn is_end(&self) -> bool {
        self.flags & stats_flags::END != 0
    }

    /// Whether this chunk starts a new waveform segment (bit 9).
    ///
    /// When set, 16 bytes of overflow metadata (prev/cur state) follow
    /// the mandatory stats header.
    ///
    /// **Confidence: High** (log string literal "start").
    pub fn is_start(&self) -> bool {
        self.flags & stats_flags::START != 0
    }

    /// Whether this is a software-triggered waveform (bit 10).
    ///
    /// **Confidence: High** (log string literal "software").
    pub fn is_software(&self) -> bool {
        self.flags & stats_flags::SOFTWARE != 0
    }

    /// Whether overflow data blocks are present (bits 9 AND 12).
    ///
    /// Requires both `is_start` (bit 9) and the overflow data flag
    /// (bit 12). When true, data after the overflow metadata is
    /// structured as 636-byte blocks.
    pub fn has_overflow_data(&self) -> bool {
        self.is_start() && (self.flags & stats_flags::OVERFLOW_DATA != 0)
    }

    /// Whether the previous overflow state flag is set (bit 14).
    ///
    /// **Confidence: High** (log string literal "prev_overflow").
    pub fn prev_overflow(&self) -> bool {
        self.flags & stats_flags::PREV_OVERFLOW != 0
    }

    /// Whether the current overflow state flag is set (bit 15).
    ///
    /// **Confidence: High** (log string literal "cur_overflow").
    pub fn cur_overflow(&self) -> bool {
        self.flags & stats_flags::CUR_OVERFLOW != 0
    }

    /// Whether overflow was flagged (cur_overflow differs from prev_overflow).
    pub fn overflow_changed(&self) -> bool {
        self.cur_overflow() != self.prev_overflow()
    }

    /// Channel / status byte from the low byte of flags.
    pub fn channel_status(&self) -> u8 {
        (self.flags & 0xFF) as u8
    }

    // -- Backwards-compatible aliases --

    /// Alias for [`is_start`](Self::is_start). Bit 9 was previously
    /// labeled "overflow_meta"; the developer's name is "start".
    pub fn has_overflow_meta(&self) -> bool {
        self.is_start()
    }

    /// Alias for [`is_software`](Self::is_software). Bit 10 was
    /// previously labeled "debug_log"; the developer's name is "software".
    pub fn debug_log(&self) -> bool {
        self.is_software()
    }
}

// ---------------------------------------------------------------------------
// Overflow metadata (16 bytes when stats flags START / bit 9 is set)
// ---------------------------------------------------------------------------

/// 16-byte overflow metadata that follows the mandatory stats header when
/// the `start` flag (bit 9, [`stats_flags::START`]) is set.
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

/// Number of entries in the min/max pyramid per block.
///
/// The pyramid has 5 levels: 16 + 8 + 4 + 2 + 1 = 31 entries.
pub const MINMAX_PYRAMID_ENTRIES: usize = 31;

/// A packed min/max pair for a group of analog samples.
///
/// Stored on the wire as a u32: `(max_i16 << 16) | (min_i16 & 0xFFFF)`.
/// Packed by `FUN_02ed1b60` in the Saleae binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinMaxPair {
    /// Minimum sample value in the group (int16).
    pub min: i16,
    /// Maximum sample value in the group (int16).
    pub max: i16,
}

impl MinMaxPair {
    /// Unpack a min/max pair from a u32.
    pub fn from_u32(packed: u32) -> Self {
        Self {
            min: packed as i16,
            max: (packed >> 16) as i16,
        }
    }

    /// Pack into a u32.
    pub fn to_u32(self) -> u32 {
        ((self.max as u16 as u32) << 16) | (self.min as u16 as u32)
    }
}

/// Per-block stats within an overflow data block (124 bytes).
///
/// Each overflow block is 636 bytes total: 512 bytes of sample data (256
/// analog samples at 2 bytes each) followed by 124 bytes of stats.
///
/// The stats are a **multi-resolution min/max pyramid** for fast waveform
/// rendering at any zoom level. The 31 entries (4 bytes each = 124 bytes)
/// are organized into 5 levels:
///
/// ```text
/// Level  Samples/Group  Entries  Offset  Description
///   0         16           16      0     Min/max for each group of 16 samples
///   1         32            8     16     Min/max for each group of 32 samples
///   2         64            4     24     Min/max for each group of 64 samples
///   3        128            2     28     Min/max for each group of 128 samples
///   4        256            1     30     Min/max for all 256 samples in block
///                          ---
///                           31 entries = 124 bytes
/// ```
///
/// Each entry is a packed u32: `(max_i16 << 16) | (min_i16 & 0xFFFF)`.
///
/// The consumer (`PackedWaveformGetStats`, `ChunkIterator::GetStats`)
/// uses this pyramid for O(log N) min/max queries at any sample range,
/// starting from the coarsest level and recursing to finer levels at
/// boundaries.
///
/// # Confidence
///
/// **High** -- decoded from `Writer::PushBack` (5 loops with strides
/// 0x20, 0x40, 0x80, 0x100, 0x200) and `PackedWaveformGetStats`
/// (recursive level-based access). Assertion `written == expected`
/// validates output is exactly `n_blocks * 0x7c`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverflowBlockStats {
    /// Multi-resolution min/max pyramid (31 entries).
    ///
    /// Entries are organized by level:
    /// - `entries[0..16]`:  level 0 (16 samples per group)
    /// - `entries[16..24]`: level 1 (32 samples per group)
    /// - `entries[24..28]`: level 2 (64 samples per group)
    /// - `entries[28..30]`: level 3 (128 samples per group)
    /// - `entries[30]`:     level 4 (all 256 samples)
    pub entries: [MinMaxPair; MINMAX_PYRAMID_ENTRIES],
}

/// Entry offsets for each pyramid level.
///
/// Level N starts at `PYRAMID_LEVEL_OFFSETS[N]` in the entries array.
pub const PYRAMID_LEVEL_OFFSETS: [usize; 5] = [0, 16, 24, 28, 30];

/// Number of entries at each pyramid level.
pub const PYRAMID_LEVEL_COUNTS: [usize; 5] = [16, 8, 4, 2, 1];

/// Number of samples per group at each pyramid level.
pub const PYRAMID_LEVEL_SAMPLES: [usize; 5] = [16, 32, 64, 128, 256];

impl OverflowBlockStats {
    /// Parse 124 bytes of per-block overflow stats (min/max pyramid).
    pub fn parse(data: &[u8]) -> ProtoResult<Self> {
        if data.len() < OVERFLOW_BLOCK_STATS_SIZE {
            return Err(ProtoError::BufferTooShort {
                need: OVERFLOW_BLOCK_STATS_SIZE,
                have: data.len(),
            });
        }

        let mut entries = [MinMaxPair { min: 0, max: 0 }; MINMAX_PYRAMID_ENTRIES];
        for (i, entry) in entries.iter_mut().enumerate().take(MINMAX_PYRAMID_ENTRIES) {
            let offset = i * 4;
            let packed = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            *entry = MinMaxPair::from_u32(packed);
        }

        Ok(Self { entries })
    }

    /// Get the min/max pair for a specific level and group index.
    ///
    /// Returns `None` if level >= 5 or index >= entries at that level.
    pub fn get(&self, level: usize, index: usize) -> Option<MinMaxPair> {
        if level >= 5 || index >= PYRAMID_LEVEL_COUNTS[level] {
            return None;
        }
        Some(self.entries[PYRAMID_LEVEL_OFFSETS[level] + index])
    }

    /// Get the overall min/max for the entire 256-sample block (level 4).
    pub fn overall_minmax(&self) -> MinMaxPair {
        self.entries[PYRAMID_LEVEL_OFFSETS[4]]
    }
}

// ---------------------------------------------------------------------------
// Parsed overflow block (512 sample bytes + 124 stats bytes)
// ---------------------------------------------------------------------------

/// One overflow data block from the EP2 stream.
///
/// When both `start` (bit 9) and `overflow_data` (bit 12) flags
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
    /// Overflow metadata (present when stats flag `start` / bit 9 is set).
    pub overflow_meta: Option<OverflowMetadata>,
    /// Overflow data blocks (present when stats flags `start` AND `overflow_data` / bits 9+12 are set).
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

        // Parse overflow metadata (16 bytes) when START (bit 9) is set.
        let overflow_meta = if stats_ref.is_start() && pos + OVERFLOW_METADATA_SIZE <= data.len() {
            let meta = OverflowMetadata::parse(&data[pos..])?;
            pos += OVERFLOW_METADATA_SIZE;
            Some(meta)
        } else {
            None
        };

        // Parse overflow data blocks when START (bit 9) AND OVERFLOW_DATA (bit 12) are set.
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
        // flags = 0x0601: start(bit9) + software(bit10) + channel_status=0x01
        data[10..12].copy_from_slice(&0x0601u16.to_le_bytes());
        // host_time = 999
        data[12..20].copy_from_slice(&999u64.to_le_bytes());

        let stats = StatsHeader::parse(&data).unwrap();
        assert_eq!(stats.packet_id, 42);
        assert_eq!(stats.timestamp_48, ts);
        assert_eq!(stats.flags, 0x0601);
        assert_eq!(stats.host_time, 999);
        assert!(stats.is_start()); // bit 9
        assert!(stats.is_software()); // bit 10
        assert!(!stats.has_overflow_data()); // bit 12 not set
        assert!(!stats.is_end()); // bit 8 not set
        assert!(!stats.prev_overflow()); // bit 14 not set
        assert!(!stats.cur_overflow()); // bit 15 not set
        assert_eq!(stats.channel_status(), 0x01);
        // Backwards-compatible aliases
        assert!(stats.has_overflow_meta()); // alias for is_start
        assert!(stats.debug_log()); // alias for is_software
    }

    #[test]
    fn stats_header_overflow_data_flag() {
        let mut data = [0u8; 20];
        // flags with bits 9 (start) and 12 (overflow_data) set = 0x1200
        data[10..12].copy_from_slice(&0x1200u16.to_le_bytes());

        let stats = StatsHeader::parse(&data).unwrap();
        assert!(stats.is_start());
        assert!(stats.has_overflow_data());
    }

    #[test]
    fn stats_header_no_flags() {
        let data = [0u8; 20];
        let stats = StatsHeader::parse(&data).unwrap();
        assert!(!stats.is_start());
        assert!(!stats.is_end());
        assert!(!stats.is_software());
        assert!(!stats.has_overflow_data());
        assert!(!stats.prev_overflow());
        assert!(!stats.cur_overflow());
        assert!(!stats.overflow_changed());
        assert_eq!(stats.channel_status(), 0);
        // Backwards-compatible aliases
        assert!(!stats.has_overflow_meta());
        assert!(!stats.debug_log());
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

    #[test]
    fn stats_header_end_flag() {
        let mut data = [0u8; 20];
        // bit 8 = END = 0x0100
        data[10..12].copy_from_slice(&0x0100u16.to_le_bytes());
        let stats = StatsHeader::parse(&data).unwrap();
        assert!(stats.is_end());
        assert!(!stats.is_start());
        assert!(!stats.is_software());
    }

    #[test]
    fn stats_header_overflow_state_flags() {
        let mut data = [0u8; 20];
        // bit 14 (prev_overflow) + bit 15 (cur_overflow) = 0xC000
        data[10..12].copy_from_slice(&0xC000u16.to_le_bytes());
        let stats = StatsHeader::parse(&data).unwrap();
        assert!(stats.prev_overflow());
        assert!(stats.cur_overflow());
        assert!(!stats.overflow_changed()); // both set, no change
    }

    #[test]
    fn stats_header_overflow_changed() {
        let mut data = [0u8; 20];
        // Only bit 15 (cur_overflow) set = 0x8000
        data[10..12].copy_from_slice(&0x8000u16.to_le_bytes());
        let stats = StatsHeader::parse(&data).unwrap();
        assert!(!stats.prev_overflow());
        assert!(stats.cur_overflow());
        assert!(stats.overflow_changed()); // cur != prev
    }

    #[test]
    fn stats_header_all_known_flags() {
        let mut data = [0u8; 20];
        // All known flags: END(8) + START(9) + SOFTWARE(10) + RESERVED_11(11)
        //                + OVERFLOW_DATA(12) + UNKNOWN_13(13)
        //                + PREV_OVERFLOW(14) + CUR_OVERFLOW(15)
        // = 0xFF00 + channel_status 0x42
        data[10..12].copy_from_slice(&0xFF42u16.to_le_bytes());
        let stats = StatsHeader::parse(&data).unwrap();
        assert!(stats.is_end());
        assert!(stats.is_start());
        assert!(stats.is_software());
        assert!(stats.has_overflow_data());
        assert!(stats.prev_overflow());
        assert!(stats.cur_overflow());
        assert!(!stats.overflow_changed()); // both overflow flags set
        assert_eq!(stats.channel_status(), 0x42);
        // Verify flag constants are correct
        assert_ne!(stats.flags & stats_flags::RESERVED_11, 0);
        assert_ne!(stats.flags & stats_flags::UNKNOWN_13, 0);
    }

    #[test]
    fn stats_flags_constants_are_distinct() {
        // Verify no flag constants overlap
        let all = [
            stats_flags::END,
            stats_flags::START,
            stats_flags::SOFTWARE,
            stats_flags::RESERVED_11,
            stats_flags::OVERFLOW_DATA,
            stats_flags::UNKNOWN_13,
            stats_flags::PREV_OVERFLOW,
            stats_flags::CUR_OVERFLOW,
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_eq!(all[i] & all[j], 0, "flag constants {} and {} overlap", i, j);
            }
        }
        // Verify they cover exactly the high byte
        let combined: u16 = all.iter().copied().fold(0u16, |acc, f| acc | f);
        assert_eq!(combined, 0xFF00);
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
        // Entry 0 (level 0, group 0): min=100, max=200
        let packed0 = MinMaxPair { min: 100, max: 200 }.to_u32();
        data[0..4].copy_from_slice(&packed0.to_le_bytes());
        // Entry 30 (level 4, overall min/max): min=-500, max=500
        let packed30 = MinMaxPair {
            min: -500,
            max: 500,
        }
        .to_u32();
        data[120..124].copy_from_slice(&packed30.to_le_bytes());

        let block_stats = OverflowBlockStats::parse(&data).unwrap();
        assert_eq!(block_stats.entries[0].min, 100);
        assert_eq!(block_stats.entries[0].max, 200);
        assert_eq!(block_stats.overall_minmax().min, -500);
        assert_eq!(block_stats.overall_minmax().max, 500);
        assert_eq!(
            block_stats.get(0, 0),
            Some(MinMaxPair { min: 100, max: 200 })
        );
        assert_eq!(
            block_stats.get(4, 0),
            Some(MinMaxPair {
                min: -500,
                max: 500
            })
        );
        assert_eq!(block_stats.get(5, 0), None); // invalid level
        assert_eq!(block_stats.get(0, 16), None); // invalid index
    }

    #[test]
    fn minmax_pair_roundtrip() {
        let pair = MinMaxPair {
            min: -32768,
            max: 32767,
        };
        assert_eq!(MinMaxPair::from_u32(pair.to_u32()), pair);

        let pair2 = MinMaxPair { min: 0, max: 0 };
        assert_eq!(MinMaxPair::from_u32(pair2.to_u32()), pair2);

        let pair3 = MinMaxPair { min: -1, max: 1 };
        assert_eq!(MinMaxPair::from_u32(pair3.to_u32()), pair3);
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
        assert!(chunk.stats.as_ref().unwrap().is_start());
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
        // Entry 0: min=42, max=99 packed as u32
        let packed = MinMaxPair { min: 42, max: 99 }.to_u32();
        block_stats[0..4].copy_from_slice(&packed.to_le_bytes());

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
        assert_eq!(chunk.overflow_blocks[0].stats.entries[0].min, 42);
        assert_eq!(chunk.overflow_blocks[0].stats.entries[0].max, 99);
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
