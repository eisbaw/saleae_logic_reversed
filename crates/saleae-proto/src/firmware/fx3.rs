//! Cypress FX3 firmware image parser.
//!
//! Parses the standard Cypress FX3 boot image format (AN76405).
//! The Saleae Logic Pro 8 and Logic Pro 16 firmware images use this format.
//!
//! ## Image Layout
//!
//! ```text
//! +--------+--------+----------+-----------+
//! | 'C'    | 'Y'    | image_ctl| image_type|  4-byte header
//! +--------+--------+----------+-----------+
//! | dLength (u32le)            |              Section N header
//! | dAddress (u32le)           |
//! | dData[dLength * 4]         |              Section N payload
//! | ...                        |
//! | dLength = 0 (u32le)        |              Terminator
//! | entry_addr (u32le)         |
//! | checksum (u32le)           |
//! +----------------------------+
//! ```
//!
//! All multi-byte fields are little-endian.

use crate::error::{ProtoError, ProtoResult};

/// FX3 firmware image magic bytes: "CY" (0x43, 0x59).
pub const FX3_MAGIC: [u8; 2] = [0x43, 0x59];

/// Normal firmware image type.
pub const IMAGE_TYPE_NORMAL: u8 = 0xB0;

/// Maximum number of sections we expect in a firmware image.
/// This is a safety limit to prevent parsing runaway data.
const MAX_SECTIONS: usize = 64;

/// Maximum section data length in 32-bit words.
/// FX3 has 512KB SRAM, so ~128K words is an absolute upper bound.
const MAX_SECTION_WORDS: u32 = 128 * 1024;

/// Maximum firmware upload chunk size for USB control transfers.
pub const FX3_MAX_CHUNK_SIZE: usize = 4096;

/// USB control transfer constants for FX3 firmware upload.
pub mod usb {
    /// bmRequestType: Host-to-Device, Vendor, Device.
    pub const BM_REQUEST_TYPE: u8 = 0x40;

    /// bRequest: Cypress FW_UPLOAD / RW_INTERNAL.
    pub const B_REQUEST: u8 = 0xA0;
}

/// A parsed FX3 firmware image header.
///
/// ## image_ctl bit layout (AN76405 Table 3)
///
/// | Bit(s) | Field         | Description                              |
/// |--------|---------------|------------------------------------------|
/// | 0      | checksum      | 1 = verify checksum during boot          |
/// | 1-2    | spi_width     | SPI bus width: 0=1bit, 1=2bit, 2=4bit    |
/// | 3      | fast_boot     | 1 = reduced verify during SPI boot       |
/// | 4      | no_execute    | 1 = do not auto-execute after load        |
/// | 5-7    | reserved      | Must be 0                                 |
///
/// Saleae firmware uses `image_ctl = 0x1C`:
///   - checksum = 0 (disabled)
///   - spi_width = 2 (4-bit SPI)
///   - fast_boot = 1
///   - no_execute = 1
///
/// Note: the Saleae host application (`ParseFirmwareImage`) does NOT verify
/// the checksum regardless of this bit. Our parser is more conservative and
/// validates when the bit is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fx3Header {
    /// Image control byte.
    pub image_ctl: u8,
    /// Image type byte. Expected: 0xB0 (normal firmware).
    pub image_type: u8,
}

impl Fx3Header {
    /// Whether checksum verification is enabled (bit 0 of image_ctl).
    pub const fn checksum_enabled(&self) -> bool {
        self.image_ctl & 0x01 != 0
    }

    /// SPI bus width setting (bits 1-2 of image_ctl).
    /// Returns 1, 2, or 4 for 1-bit, 2-bit, or 4-bit SPI.
    pub const fn spi_width(&self) -> u8 {
        match (self.image_ctl >> 1) & 0x03 {
            0 => 1,
            1 => 2,
            _ => 4,
        }
    }

    /// Whether fast boot is enabled (bit 3 of image_ctl).
    pub const fn fast_boot(&self) -> bool {
        self.image_ctl & 0x08 != 0
    }
}

/// A single section within an FX3 firmware image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fx3Section {
    /// Target address in FX3 memory map.
    pub address: u32,
    /// Raw data bytes to load at this address.
    pub data: Vec<u8>,
}

impl Fx3Section {
    /// Length of data in 32-bit words.
    pub fn word_len(&self) -> u32 {
        (self.data.len() / 4) as u32
    }
}

/// A fully parsed FX3 firmware image.
#[derive(Debug, Clone)]
pub struct Fx3Image {
    /// Image header.
    pub header: Fx3Header,
    /// Code/data sections to load.
    pub sections: Vec<Fx3Section>,
    /// Program entry point address.
    pub entry_address: u32,
    /// Stored checksum (if checksum is enabled).
    pub checksum: u32,
}

impl Fx3Image {
    /// Parse an FX3 firmware image from a byte slice.
    ///
    /// Returns the parsed image and the number of bytes consumed.
    pub fn parse(data: &[u8]) -> ProtoResult<(Self, usize)> {
        if data.len() < 4 {
            return Err(ProtoError::BufferTooShort {
                need: 4,
                have: data.len(),
            });
        }

        // Validate magic.
        if data[0..2] != FX3_MAGIC {
            return Err(ProtoError::InvalidFx3Magic {
                got: u16::from_le_bytes([data[0], data[1]]),
            });
        }

        let header = Fx3Header {
            image_ctl: data[2],
            image_type: data[3],
        };

        if header.image_type != IMAGE_TYPE_NORMAL {
            return Err(ProtoError::UnexpectedFx3ImageType {
                got: header.image_type,
            });
        }

        let mut offset = 4;
        let mut sections = Vec::new();
        let mut checksum_accum: u32 = 0;

        for _ in 0..MAX_SECTIONS {
            if offset + 8 > data.len() {
                return Err(ProtoError::BufferTooShort {
                    need: offset + 8,
                    have: data.len(),
                });
            }

            let d_length = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            let d_address = u32::from_le_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);

            // Terminal section: dLength == 0.
            if d_length == 0 {
                let entry_address = d_address;

                // Read checksum.
                if offset + 12 > data.len() {
                    return Err(ProtoError::BufferTooShort {
                        need: offset + 12,
                        have: data.len(),
                    });
                }

                let checksum = u32::from_le_bytes([
                    data[offset + 8],
                    data[offset + 9],
                    data[offset + 10],
                    data[offset + 11],
                ]);

                let consumed = offset + 12;

                let image = Fx3Image {
                    header,
                    sections,
                    entry_address,
                    checksum,
                };

                // Verify checksum if enabled.
                if header.checksum_enabled() && checksum_accum != checksum {
                    return Err(ProtoError::ChecksumMismatch {
                        computed: checksum_accum,
                        stored: checksum,
                    });
                }

                return Ok((image, consumed));
            }

            // Validate section length.
            if d_length > MAX_SECTION_WORDS {
                return Err(ProtoError::InvalidFirmwareSection {
                    offset,
                    field: "dLength",
                    value: d_length,
                });
            }

            let data_bytes = (d_length as usize) * 4;
            let section_end = offset + 8 + data_bytes;

            if section_end > data.len() {
                return Err(ProtoError::BufferTooShort {
                    need: section_end,
                    have: data.len(),
                });
            }

            let section_data = data[offset + 8..section_end].to_vec();

            // Accumulate checksum over the section's 32-bit words.
            for chunk in section_data.chunks_exact(4) {
                let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                checksum_accum = checksum_accum.wrapping_add(word);
            }

            sections.push(Fx3Section {
                address: d_address,
                data: section_data,
            });

            offset = section_end;
        }

        Err(ProtoError::parse(format!(
            "firmware image has more than {MAX_SECTIONS} sections"
        )))
    }

    /// Total firmware payload size in bytes (sum of all section data).
    pub fn total_data_bytes(&self) -> usize {
        self.sections.iter().map(|s| s.data.len()).sum()
    }

    /// Iterator over (address, chunk) pairs for USB upload.
    ///
    /// Each chunk is at most `FX3_MAX_CHUNK_SIZE` bytes. Sections that are
    /// larger than this are split into multiple chunks with sequential
    /// addresses.
    pub fn upload_chunks(&self) -> impl Iterator<Item = (u32, &[u8])> {
        self.sections.iter().flat_map(|section| {
            section
                .data
                .chunks(FX3_MAX_CHUNK_SIZE)
                .enumerate()
                .map(move |(i, chunk)| {
                    let addr = section.address + (i * FX3_MAX_CHUNK_SIZE) as u32;
                    (addr, chunk)
                })
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid FX3 image with one section.
    fn make_test_image(section_data: &[u8], entry: u32) -> Vec<u8> {
        assert!(
            section_data.len() % 4 == 0,
            "section data must be word-aligned"
        );
        let word_count = (section_data.len() / 4) as u32;

        // Compute checksum.
        let mut checksum: u32 = 0;
        for chunk in section_data.chunks_exact(4) {
            let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            checksum = checksum.wrapping_add(word);
        }

        let mut buf = Vec::new();
        // Header.
        buf.extend_from_slice(&FX3_MAGIC);
        buf.push(0x1D); // image_ctl: checksum enabled (bit 0 set).
        buf.push(IMAGE_TYPE_NORMAL);
        // Section.
        buf.extend_from_slice(&word_count.to_le_bytes());
        buf.extend_from_slice(&0x40003000u32.to_le_bytes()); // address
        buf.extend_from_slice(section_data);
        // Terminator.
        buf.extend_from_slice(&0u32.to_le_bytes()); // dLength = 0
        buf.extend_from_slice(&entry.to_le_bytes());
        buf.extend_from_slice(&checksum.to_le_bytes());

        buf
    }

    #[test]
    fn parse_minimal_image() {
        let section_data = [0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00];
        let raw = make_test_image(&section_data, 0x4000_e020);

        let (image, consumed) = Fx3Image::parse(&raw).unwrap();
        assert_eq!(consumed, raw.len());
        assert_eq!(image.header.image_type, IMAGE_TYPE_NORMAL);
        assert!(image.header.checksum_enabled());
        assert_eq!(image.sections.len(), 1);
        assert_eq!(image.sections[0].address, 0x4000_3000);
        assert_eq!(image.sections[0].data, section_data);
        assert_eq!(image.entry_address, 0x4000_e020);
        assert_eq!(image.total_data_bytes(), 8);
    }

    #[test]
    fn parse_empty_section_data() {
        // Image with zero sections -- just a terminator.
        let mut buf = Vec::new();
        buf.extend_from_slice(&FX3_MAGIC);
        buf.push(0x00); // image_ctl: no checksum
        buf.push(IMAGE_TYPE_NORMAL);
        // Terminator immediately.
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0x4000_0000u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // checksum (unused)

        let (image, _consumed) = Fx3Image::parse(&buf).unwrap();
        assert_eq!(image.sections.len(), 0);
        assert_eq!(image.entry_address, 0x4000_0000);
    }

    #[test]
    fn bad_magic_rejected() {
        let buf = [0x00, 0x00, 0x00, IMAGE_TYPE_NORMAL];
        let err = Fx3Image::parse(&buf).unwrap_err();
        assert!(matches!(err, ProtoError::InvalidFx3Magic { .. }));
    }

    #[test]
    fn bad_image_type_rejected() {
        let buf = [0x43, 0x59, 0x00, 0xFF];
        let err = Fx3Image::parse(&buf).unwrap_err();
        assert!(matches!(
            err,
            ProtoError::UnexpectedFx3ImageType { got: 0xFF }
        ));
    }

    #[test]
    fn truncated_buffer_rejected() {
        let buf = [0x43, 0x59];
        let err = Fx3Image::parse(&buf).unwrap_err();
        assert!(matches!(err, ProtoError::BufferTooShort { .. }));
    }

    #[test]
    fn checksum_mismatch_detected() {
        let section_data = [0x01, 0x00, 0x00, 0x00];
        let mut raw = make_test_image(&section_data, 0x4000_e020);
        // Corrupt the stored checksum (last 4 bytes).
        let len = raw.len();
        raw[len - 1] = 0xFF;

        let err = Fx3Image::parse(&raw).unwrap_err();
        assert!(matches!(err, ProtoError::ChecksumMismatch { .. }));
    }

    #[test]
    fn upload_chunks_splits_large_section() {
        // Create a section larger than FX3_MAX_CHUNK_SIZE.
        let section_data = vec![0xABu8; FX3_MAX_CHUNK_SIZE * 2 + 128];
        // Pad to word alignment.
        let padded_len = (section_data.len() + 3) & !3;
        let mut padded = section_data;
        padded.resize(padded_len, 0);

        let raw = make_test_image(&padded, 0x4000_e020);
        let (image, _) = Fx3Image::parse(&raw).unwrap();

        let chunks: Vec<_> = image.upload_chunks().collect();
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].0, 0x4000_3000);
        assert_eq!(chunks[0].1.len(), FX3_MAX_CHUNK_SIZE);
        assert_eq!(chunks[1].0, 0x4000_3000 + FX3_MAX_CHUNK_SIZE as u32);
        assert_eq!(chunks[1].1.len(), FX3_MAX_CHUNK_SIZE);
        assert_eq!(chunks[2].0, 0x4000_3000 + (FX3_MAX_CHUNK_SIZE * 2) as u32);
        assert_eq!(chunks[2].1.len(), 128);
    }

    /// Parse a firmware image with image_ctl=0x1C (checksum bit NOT set).
    /// This matches the real Saleae FX3 firmware format: SPI 4-bit width,
    /// fast boot, but checksum verification disabled.
    #[test]
    fn parse_saleae_style_header() {
        let section_data = [0x10, 0x40, 0x2D, 0xE9]; // ARM instruction
        let word_count = 1u32;

        // Compute checksum (even though not checked, must be present).
        let checksum: u32 = u32::from_le_bytes(section_data);

        let mut buf = Vec::new();
        buf.extend_from_slice(&FX3_MAGIC);
        buf.push(0x1C); // image_ctl: Saleae-style (checksum disabled, SPI 4-bit, fast boot)
        buf.push(IMAGE_TYPE_NORMAL);
        // Section at the FX3 interrupt vector table address.
        buf.extend_from_slice(&word_count.to_le_bytes());
        buf.extend_from_slice(&0x0000_0100u32.to_le_bytes());
        buf.extend_from_slice(&section_data);
        // Terminator.
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0x4000_e020u32.to_le_bytes());
        buf.extend_from_slice(&checksum.to_le_bytes());

        let (image, consumed) = Fx3Image::parse(&buf).unwrap();
        assert_eq!(consumed, buf.len());
        assert_eq!(image.header.image_ctl, 0x1C);
        assert!(!image.header.checksum_enabled());
        assert_eq!(image.sections.len(), 1);
        assert_eq!(image.sections[0].address, 0x0000_0100);
        assert_eq!(image.entry_address, 0x4000_e020);
    }

    /// Parse a 4-section image matching the real Saleae Pro firmware layout:
    ///   Section 0: 0x00000100 (interrupt vectors)
    ///   Section 1: 0x40003000 (main code)
    ///   Section 2: 0x40013000 (code continued)
    ///   Section 3: 0x40030000 (data/descriptors)
    #[test]
    fn parse_four_section_image() {
        // Simulate the real layout with small sections.
        let sections: &[(u32, &[u8])] = &[
            (0x0000_0100, &[0x10, 0x40, 0x2D, 0xE9]),
            (0x4000_3000, &[0x30, 0x40, 0x2D, 0xE9]),
            (0x4001_3000, &[0x04, 0x10, 0x80, 0xE5]),
            (0x4003_0000, &[0x8E, 0x24, 0x4B, 0x35]),
        ];

        let mut checksum: u32 = 0;
        let mut buf = Vec::new();
        buf.extend_from_slice(&FX3_MAGIC);
        buf.push(0x1C);
        buf.push(IMAGE_TYPE_NORMAL);

        for (addr, data) in sections {
            let word_count = (data.len() / 4) as u32;
            buf.extend_from_slice(&word_count.to_le_bytes());
            buf.extend_from_slice(&addr.to_le_bytes());
            buf.extend_from_slice(data);
            for chunk in data.chunks_exact(4) {
                let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                checksum = checksum.wrapping_add(word);
            }
        }

        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0x4000_e020u32.to_le_bytes());
        buf.extend_from_slice(&checksum.to_le_bytes());

        let (image, consumed) = Fx3Image::parse(&buf).unwrap();
        assert_eq!(consumed, buf.len());
        assert_eq!(image.sections.len(), 4);
        assert_eq!(image.sections[0].address, 0x0000_0100);
        assert_eq!(image.sections[1].address, 0x4000_3000);
        assert_eq!(image.sections[2].address, 0x4001_3000);
        assert_eq!(image.sections[3].address, 0x4003_0000);
        assert_eq!(image.entry_address, 0x4000_e020);
    }

    /// Parse the real Saleae Logic Pro 8 firmware extracted from the binary.
    /// This test is skipped if the firmware file doesn't exist.
    #[test]
    fn parse_real_pro8_firmware() {
        let path = std::path::Path::new("re/captures/fx3-firmware-pro8.bin");
        if !path.exists() {
            // Try from workspace root.
            let ws_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../re/captures/fx3-firmware-pro8.bin");
            if !ws_path.exists() {
                eprintln!("skipping: firmware file not found");
                return;
            }
            let raw = std::fs::read(&ws_path).unwrap();
            validate_pro_firmware(&raw, "Pro8");
            return;
        }
        let raw = std::fs::read(path).unwrap();
        validate_pro_firmware(&raw, "Pro8");
    }

    /// Parse the real Saleae Logic Pro 16 firmware extracted from the binary.
    #[test]
    fn parse_real_pro16_firmware() {
        let ws_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../re/captures/fx3-firmware-pro16.bin");
        if !ws_path.exists() {
            eprintln!("skipping: firmware file not found");
            return;
        }
        let raw = std::fs::read(&ws_path).unwrap();
        validate_pro_firmware(&raw, "Pro16");
    }

    fn validate_pro_firmware(raw: &[u8], label: &str) {
        let (image, consumed) =
            Fx3Image::parse(raw).unwrap_or_else(|e| panic!("{label} firmware parse failed: {e}"));

        assert_eq!(consumed, raw.len(), "{label}: should consume entire file");
        assert_eq!(image.header.image_ctl, 0x1C, "{label}: image_ctl");
        assert_eq!(
            image.header.image_type, IMAGE_TYPE_NORMAL,
            "{label}: image_type"
        );
        assert!(
            !image.header.checksum_enabled(),
            "{label}: checksum should be disabled"
        );
        assert_eq!(image.header.spi_width(), 4, "{label}: SPI width");
        assert!(image.header.fast_boot(), "{label}: fast boot");

        // Real firmware has 4 sections.
        assert_eq!(image.sections.len(), 4, "{label}: section count");

        // Section addresses match the Saleae firmware layout.
        assert_eq!(image.sections[0].address, 0x0000_0100, "{label}: sec0 addr");
        assert_eq!(image.sections[1].address, 0x4000_3000, "{label}: sec1 addr");
        assert_eq!(image.sections[2].address, 0x4001_3000, "{label}: sec2 addr");
        assert_eq!(image.sections[3].address, 0x4003_0000, "{label}: sec3 addr");

        assert_eq!(image.entry_address, 0x4000_e020, "{label}: entry addr");

        // Total data should be ~119776 bytes.
        let total = image.total_data_bytes();
        assert!(total > 100_000, "{label}: total data {total} too small");
        assert!(total < 130_000, "{label}: total data {total} too large");

        // Upload chunks should work.
        let chunks: Vec<_> = image.upload_chunks().collect();
        assert!(
            chunks.len() > 10,
            "{label}: expected many chunks, got {}",
            chunks.len()
        );

        eprintln!(
            "{label}: parsed OK, {total} bytes in {} sections, {} upload chunks",
            image.sections.len(),
            chunks.len()
        );
    }

    // -- Edge case and adversarial tests --

    #[test]
    fn empty_input_rejected() {
        let err = Fx3Image::parse(&[]).unwrap_err();
        assert!(matches!(err, ProtoError::BufferTooShort { need: 4, have: 0 }));
    }

    #[test]
    fn one_byte_input_rejected() {
        let err = Fx3Image::parse(&[0x43]).unwrap_err();
        assert!(matches!(err, ProtoError::BufferTooShort { .. }));
    }

    #[test]
    fn header_only_no_sections_rejected() {
        // Valid header but no section data at all
        let buf = [0x43, 0x59, 0x00, IMAGE_TYPE_NORMAL];
        let err = Fx3Image::parse(&buf).unwrap_err();
        assert!(matches!(err, ProtoError::BufferTooShort { .. }));
    }

    #[test]
    fn section_length_overflow_rejected() {
        // dLength = MAX_SECTION_WORDS + 1, should be rejected
        let mut buf = Vec::new();
        buf.extend_from_slice(&FX3_MAGIC);
        buf.push(0x00);
        buf.push(IMAGE_TYPE_NORMAL);
        let bad_len = MAX_SECTION_WORDS + 1;
        buf.extend_from_slice(&bad_len.to_le_bytes());
        buf.extend_from_slice(&0x4000_3000u32.to_le_bytes());

        let err = Fx3Image::parse(&buf).unwrap_err();
        assert!(
            matches!(err, ProtoError::InvalidFirmwareSection { .. }),
            "expected InvalidFirmwareSection, got: {err}"
        );
    }

    #[test]
    fn section_data_truncated_rejected() {
        // Claim 2 words (8 bytes) but only provide 4 bytes of data
        let mut buf = Vec::new();
        buf.extend_from_slice(&FX3_MAGIC);
        buf.push(0x00);
        buf.push(IMAGE_TYPE_NORMAL);
        buf.extend_from_slice(&2u32.to_le_bytes()); // 2 words = 8 bytes
        buf.extend_from_slice(&0x4000_3000u32.to_le_bytes());
        buf.extend_from_slice(&[0xAA; 4]); // only 4 bytes, need 8

        let err = Fx3Image::parse(&buf).unwrap_err();
        assert!(matches!(err, ProtoError::BufferTooShort { .. }));
    }

    #[test]
    fn terminator_without_checksum_rejected() {
        // Terminator (dLength=0) but not enough bytes for entry + checksum
        let mut buf = Vec::new();
        buf.extend_from_slice(&FX3_MAGIC);
        buf.push(0x00);
        buf.push(IMAGE_TYPE_NORMAL);
        buf.extend_from_slice(&0u32.to_le_bytes()); // dLength = 0 (terminator)
        buf.extend_from_slice(&0x4000_0000u32.to_le_bytes()); // entry addr
        // Missing checksum bytes

        let err = Fx3Image::parse(&buf).unwrap_err();
        assert!(matches!(err, ProtoError::BufferTooShort { .. }));
    }

    #[test]
    fn max_sections_overflow_rejected() {
        // Build an image with MAX_SECTIONS+1 sections (all minimal)
        let mut buf = Vec::new();
        buf.extend_from_slice(&FX3_MAGIC);
        buf.push(0x00); // no checksum
        buf.push(IMAGE_TYPE_NORMAL);

        for _ in 0..=MAX_SECTIONS {
            buf.extend_from_slice(&1u32.to_le_bytes()); // 1 word
            buf.extend_from_slice(&0x4000_3000u32.to_le_bytes());
            buf.extend_from_slice(&[0x00; 4]); // 4 bytes of data
        }
        // Terminator
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0x4000_0000u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());

        let err = Fx3Image::parse(&buf).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("more than") && msg.contains("sections"),
            "expected section limit error, got: {msg}"
        );
    }

    #[test]
    fn trailing_garbage_after_valid_image_ignored() {
        // A valid image followed by extra bytes. parse() should return
        // consumed < data.len(), and the image should be valid.
        let section_data = [0x01, 0x00, 0x00, 0x00];
        let mut raw = make_test_image(&section_data, 0x4000_e020);
        raw.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // trailing garbage

        let (image, consumed) = Fx3Image::parse(&raw).unwrap();
        assert_eq!(consumed, raw.len() - 4, "should not consume trailing garbage");
        assert_eq!(image.sections.len(), 1);
    }

    #[test]
    fn checksum_disabled_ignores_bad_checksum() {
        // image_ctl bit 0 = 0 means checksum not verified
        let section_data = [0x01, 0x00, 0x00, 0x00];
        let mut buf = Vec::new();
        buf.extend_from_slice(&FX3_MAGIC);
        buf.push(0x00); // checksum disabled
        buf.push(IMAGE_TYPE_NORMAL);
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&0x4000_3000u32.to_le_bytes());
        buf.extend_from_slice(&section_data);
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0x4000_e020u32.to_le_bytes());
        buf.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // wrong checksum

        // Should succeed because checksum bit is not set
        let (image, _) = Fx3Image::parse(&buf).unwrap();
        assert_eq!(image.sections.len(), 1);
    }

    #[test]
    fn section_word_len_calculated_correctly() {
        let section = Fx3Section {
            address: 0,
            data: vec![0; 20], // 5 words
        };
        assert_eq!(section.word_len(), 5);
    }

    #[test]
    fn upload_chunks_empty_image() {
        let image = Fx3Image {
            header: Fx3Header {
                image_ctl: 0,
                image_type: IMAGE_TYPE_NORMAL,
            },
            sections: vec![],
            entry_address: 0,
            checksum: 0,
        };
        let chunks: Vec<_> = image.upload_chunks().collect();
        assert!(chunks.is_empty());
    }

    #[test]
    fn spi_width_values() {
        // Verify all SPI width decodings from image_ctl bits 1-2
        assert_eq!(Fx3Header { image_ctl: 0b000, image_type: IMAGE_TYPE_NORMAL }.spi_width(), 1);
        assert_eq!(Fx3Header { image_ctl: 0b010, image_type: IMAGE_TYPE_NORMAL }.spi_width(), 2);
        assert_eq!(Fx3Header { image_ctl: 0b100, image_type: IMAGE_TYPE_NORMAL }.spi_width(), 4);
        assert_eq!(Fx3Header { image_ctl: 0b110, image_type: IMAGE_TYPE_NORMAL }.spi_width(), 4);
    }

    #[test]
    fn multiple_sections_parsed() {
        let sec1 = [0x01u8, 0x00, 0x00, 0x00];
        let sec2 = [0x02u8, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00];

        let mut checksum: u32 = 0;
        for chunk in sec1.chunks_exact(4).chain(sec2.chunks_exact(4)) {
            let word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            checksum = checksum.wrapping_add(word);
        }

        let mut buf = Vec::new();
        // Header.
        buf.extend_from_slice(&FX3_MAGIC);
        buf.push(0x01); // checksum enabled
        buf.push(IMAGE_TYPE_NORMAL);
        // Section 1.
        buf.extend_from_slice(&1u32.to_le_bytes()); // 1 word
        buf.extend_from_slice(&0x0000_0100u32.to_le_bytes());
        buf.extend_from_slice(&sec1);
        // Section 2.
        buf.extend_from_slice(&2u32.to_le_bytes()); // 2 words
        buf.extend_from_slice(&0x4000_3000u32.to_le_bytes());
        buf.extend_from_slice(&sec2);
        // Terminator.
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0x4000_e020u32.to_le_bytes());
        buf.extend_from_slice(&checksum.to_le_bytes());

        let (image, _) = Fx3Image::parse(&buf).unwrap();
        assert_eq!(image.sections.len(), 2);
        assert_eq!(image.sections[0].address, 0x0000_0100);
        assert_eq!(image.sections[0].data, sec1);
        assert_eq!(image.sections[1].address, 0x4000_3000);
        assert_eq!(image.sections[1].data, sec2);
    }
}
