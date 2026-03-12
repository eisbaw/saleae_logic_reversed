//! Firmware image parsing.
//!
//! ## Firmware Format Selection (RE findings)
//!
//! The Saleae binary (`GetFirmwareForDeviceType`) uses an internal DeviceType
//! enum (0-7) to select the firmware format:
//!
//! | Enum | Device         | Format | Notes                          |
//! |------|----------------|--------|--------------------------------|
//! | 0    | Logic Original | IHex   | FX2 8051 firmware              |
//! | 1    | Logic 16       | IHex   | FX2LP 8051 firmware            |
//! | 2    | (MSO?)         | --     | "firmware not available"       |
//! | 3    | (MSO Plus?)    | --     | "firmware not available"       |
//! | 4    | Logic 4        | IHex   | FX2-compatible (8051 core)     |
//! | 5    | Logic 8        | IHex   | FX2-compatible (8051 core)     |
//! | 6    | Logic Pro 8    | CY     | FX3 ARM firmware (AN76405)     |
//! | 7    | Logic Pro 16   | CY     | FX3 ARM firmware (AN76405)     |
//!
//! Key findings:
//! - Logic 4 and Logic 8 use **IHex** format (8051 addresses), despite
//!   sometimes being called "FX3 devices". They may use a different
//!   Cypress chip or an FX2-compatible upload path.
//! - Only Logic Pro 8 and Logic Pro 16 use the **CY** (Cypress boot image)
//!   format, which is the standard FX3 ARM firmware format.
//! - Both Pro 8 and Pro 16 firmware are identical except for 4 bytes
//!   (channel count and product ID differences) plus the checksum.
//! - Saleae's `ParseFirmwareImage` does NOT verify the checksum, even
//!   though the firmware has `image_ctl = 0x1C` (checksum bit 0 = 0).
//!
//! We only implement the CY/FX3 parser since the Pro devices are our
//! primary target.

pub mod fx3;
