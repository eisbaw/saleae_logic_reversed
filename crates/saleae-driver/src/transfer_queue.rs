//! USB transfer queue sizing.
//!
//! Computes the transfer buffer size and queue depth for USB bulk transfers
//! during capture. The sizing formula matches the original Saleae software's
//! `ComputeTransferSize` (vtable slot 11) and `AdjustTransferSize` functions.
//!
//! See `re/notes/transfer-queue-sizing.md` for the full RE analysis and
//! `re/notes/streaming-thread-decompilation.md` for the decompiled streaming
//! thread architecture.
//!
//! # Formula
//!
//! ```text
//! ideal_size = ceil(bandwidth_bytes_per_sec / 240)
//! block_size = 4096 (USB 2.0) or 16384 (USB 3.0)
//! transfer_size = round_up(ideal_size, block_size)
//! transfer_size = min(transfer_size, MAX_TRANSFER_SIZE aligned to block_size)
//! transfer_size = max(transfer_size, block_size)
//! ```
//!
//! The divisor 240 means each transfer carries ~1/240th of a second of data.
//! At 240 USB completions per second, this keeps the pipe saturated while
//! maintaining reasonable latency.
//!
//! # Streaming architecture (from decompilation)
//!
//! The original uses a two-thread model:
//! - Thread 1 ("BulkReadStream_Read"): processes completed transfers via
//!   `libusb_handle_events_timeout_completed`, delivers data to callbacks.
//! - Thread 2 ("ReadStream_QueueBuffersThread"): allocates buffers from a
//!   MemoryPool and submits transfers via `libusb_submit_transfer`.
//!
//! Our nusb-based approach uses `nusb::transfer::Queue` which handles
//! the submit/complete cycle internally. The key parameters (transfer_size,
//! queue_depth, timeout) are identical.
//!
//! # Buffer management (MemoryPool equivalence)
//!
//! The original uses `Saleae::MemoryPool` (memory_pool.cpp) -- a thread-safe
//! free-list pool that allocates fixed-size byte buffers (`operator new`,
//! not mmap) and recycles them via `shared_ptr` custom deleters. The pool
//! blocks submission when exhausted (`HasAvailable()` returns false).
//!
//! nusb eliminates the need for a separate pool: `RequestBuffer::reuse`
//! recycles the completed transfer's `Vec<u8>` allocation directly. The
//! caller controls queue depth by tracking `queue.pending()`. See
//! `doc/memory-pool.md` for the full decompiled architecture.
//!
//! # Original constants (confirmed by decompilation)
//!
//! - `timeout_ms = 1000` (per-event-loop-iteration, not per-transfer)
//! - `max_transfer_bytes = 5_000_000_000` (5 GB, effectively unlimited)
//! - `queue_depth = min(128, max_transfer_bytes / transfer_size)` = always 128
//! - Per-transfer libusb timeout = 0 (no timeout, handled at event loop level)
//! - Condition variable poll interval = 200ms

use saleae_proto::device::UsbSpeed;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Number of outstanding USB bulk transfers to queue.
///
/// From `ReadStream_QueueBuffersThread`: `queue_depth = min(128, max_bytes / transfer_size)`.
/// With max_bytes = 5 GB and typical transfer sizes of 28 KB - 1.6 MB,
/// the depth is effectively always 128.
pub const QUEUE_DEPTH: usize = 128;

/// Default maximum transfer buffer size in bytes (0x190000 = 1,638,400).
///
/// This is the hard-coded cap in `AdjustTransferSize`. Transfers larger
/// than this are clamped down, then block-aligned.
pub const MAX_TRANSFER_SIZE: usize = 1_638_400;

/// USB 2.0 High Speed block alignment size.
///
/// Transfer sizes are rounded up to multiples of this value when the
/// device is connected at USB 2.0 speed. Matches USB 2.0 max packet
/// size multiples for efficient DMA.
pub const USB2_BLOCK_SIZE: usize = 4096;

/// USB 3.0 SuperSpeed block alignment size.
///
/// Transfer sizes are rounded up to multiples of this value when the
/// device is connected at USB 3.0 speed. Matches USB 3.0 burst size
/// multiples (bMaxBurst=15, 16 * 1024 = 16384).
pub const USB3_BLOCK_SIZE: usize = 16384;

/// Target number of USB transfer completions per second.
///
/// The bandwidth is divided by this value to get the ideal transfer size.
/// Confirmed by the "expected update rate: {}/s" log in the original
/// Professional StartSampling.
const TRANSFERS_PER_SECOND: u64 = 240;

// ---------------------------------------------------------------------------
// Block size helper
// ---------------------------------------------------------------------------

/// Return the USB block alignment size for the given speed.
///
/// USB 3.0 (Super, SuperPlus) uses 16384-byte blocks.
/// Everything else (Low, Full, High) uses 4096-byte blocks.
pub fn block_size_for_speed(usb_speed: UsbSpeed) -> usize {
    match usb_speed {
        UsbSpeed::Super | UsbSpeed::SuperPlus => USB3_BLOCK_SIZE,
        _ => USB2_BLOCK_SIZE,
    }
}

// ---------------------------------------------------------------------------
// Core computation
// ---------------------------------------------------------------------------

/// Compute the USB bulk transfer buffer size.
///
/// Implements the Saleae `ComputeTransferSize` + `AdjustTransferSize` logic:
///
/// 1. Divide bandwidth by 240 to get the ideal transfer size.
/// 2. Round up to the nearest block boundary.
/// 3. Cap at `MAX_TRANSFER_SIZE` (aligned down to block boundary).
/// 4. Clamp to a minimum of one block (never zero).
///
/// # Arguments
///
/// - `bandwidth_bytes_per_sec`: Total USB data rate in bytes/sec.
/// - `usb_speed`: Negotiated USB connection speed.
///
/// # Returns
///
/// Transfer buffer size in bytes, always a multiple of the block size.
pub fn compute_transfer_size(bandwidth_bytes_per_sec: u64, usb_speed: UsbSpeed) -> usize {
    let block = block_size_for_speed(usb_speed) as u64;

    // Step 1: ideal transfer size = ceil(bandwidth / 240).
    let ideal = bandwidth_bytes_per_sec.div_ceil(TRANSFERS_PER_SECOND);

    // Step 2: round up to block boundary.
    let rounded = if ideal == 0 {
        block
    } else {
        ideal.div_ceil(block) * block
    };

    // Step 3: apply maximum cap (aligned down to block boundary).
    let max_aligned = (MAX_TRANSFER_SIZE as u64 / block) * block;
    let capped = rounded.min(max_aligned);

    // Step 4: enforce minimum of one block.
    capped.max(block) as usize
}

/// Compute the total USB bandwidth in bytes per second.
///
/// Each digital channel contributes 1 bit per sample. Each analog channel
/// contributes `bytes_per_sample` bytes per sample. The total is:
///
/// ```text
/// bandwidth = (digital_channels * sample_rate) / 8
///           + analog_channels * sample_rate * bytes_per_sample
/// ```
///
/// This matches the Saleae `CalculateBandwidth` function
/// (ELF VA 0x1c4bbb0), which sums `channel * rate` for digital channels
/// (1 bit each) and `channel * rate * bits_per_sample` for analog,
/// then divides the total bits by 8.
///
/// # Arguments
///
/// - `sample_rate`: Sample rate in Hz (shared by digital and analog).
/// - `digital_channels`: Number of active digital channels.
/// - `analog_channels`: Number of active analog channels.
/// - `bytes_per_sample`: Bytes per analog sample (e.g., 2 for 12-bit packed as int16).
pub fn compute_bandwidth(
    sample_rate: u64,
    digital_channels: u8,
    analog_channels: u8,
    bytes_per_sample: u8,
) -> u64 {
    let digital_bits = sample_rate as u128 * digital_channels as u128;
    let analog_bits = sample_rate as u128 * analog_channels as u128 * bytes_per_sample as u128 * 8;
    let total_bits = digital_bits + analog_bits;
    // Convert to bytes, rounding up.
    let bytes = total_bits.div_ceil(8);
    // Clamp to u64 range (will not overflow in practice).
    bytes.min(u64::MAX as u128) as u64
}

// ---------------------------------------------------------------------------
// TransferConfig
// ---------------------------------------------------------------------------

/// USB transfer queue configuration.
///
/// Groups the computed transfer size, queue depth, and block alignment
/// needed to set up the USB bulk read stream. Corresponds to the
/// parameters packed by each device's `StartSampling` into the
/// `BulkReadStream` configuration struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferConfig {
    /// Bytes per USB bulk transfer buffer.
    pub transfer_size: usize,
    /// Number of outstanding transfers to keep queued.
    pub queue_depth: usize,
    /// USB block alignment size (4096 or 16384).
    pub block_size: usize,
}

impl TransferConfig {
    /// Compute transfer configuration from capture parameters.
    ///
    /// This is the convenience entry point that combines bandwidth
    /// calculation and transfer sizing in one call, matching the
    /// Saleae `StartSampling` flow: `CalculateBandwidth` then
    /// `ComputeTransferSize`.
    ///
    /// If `SALEAE_USB_TRANSFER_LIMIT` is set, the transfer size
    /// is capped to that value (block-aligned).
    pub fn from_capture(
        sample_rate: u64,
        digital_channels: u8,
        analog_channels: u8,
        bytes_per_sample: u8,
        usb_speed: UsbSpeed,
    ) -> Self {
        let bandwidth = compute_bandwidth(
            sample_rate,
            digital_channels,
            analog_channels,
            bytes_per_sample,
        );
        let block = block_size_for_speed(usb_speed);
        let mut transfer_size = compute_transfer_size(bandwidth, usb_speed);

        // Apply environment variable override if present.
        if let Some(env_limit) = Self::from_env_override() {
            // Align the env limit down to block boundary.
            let aligned_limit = (env_limit / block) * block;
            let aligned_limit = if aligned_limit == 0 {
                block
            } else {
                aligned_limit
            };
            if aligned_limit < transfer_size {
                transfer_size = aligned_limit;
            }
        }

        Self {
            transfer_size,
            queue_depth: QUEUE_DEPTH,
            block_size: block,
        }
    }

    /// Check for the `SALEAE_USB_TRANSFER_LIMIT` environment variable.
    ///
    /// Returns `Some(limit)` if the variable is set to a non-zero decimal
    /// integer, `None` otherwise. This mirrors the `AdjustTransferSize`
    /// function which calls `getenv("SALEAE_USB_TRANSFER_LIMIT")` and
    /// parses the value via `strtoll`.
    pub fn from_env_override() -> Option<usize> {
        let val = std::env::var("SALEAE_USB_TRANSFER_LIMIT").ok()?;
        let parsed: i64 = match val.parse() {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    value = %val,
                    error = %e,
                    "SALEAE_USB_TRANSFER_LIMIT is set but not a valid integer, ignoring"
                );
                return None;
            }
        };
        if parsed > 0 {
            Some(parsed as usize)
        } else {
            None
        }
    }

    /// Total memory needed for all transfer buffers.
    pub fn total_buffer_bytes(&self) -> usize {
        self.transfer_size * self.queue_depth
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // compute_transfer_size
    // -----------------------------------------------------------------------

    #[test]
    fn usb3_500mhz_8_digital() {
        // 8 digital channels at 500 MHz:
        // bandwidth = 8 * 500_000_000 / 8 = 500_000_000 bytes/sec
        // ideal = ceil(500_000_000 / 240) = 2_083_334
        // block = 16384 (USB 3.0)
        // rounded = ceil(2_083_334 / 16384) * 16384 = 128 * 16384 = 2_097_152
        // max_aligned = (1_638_400 / 16384) * 16384 = 100 * 16384 = 1_638_400
        //   (0x190000 is exactly 100 * 16384, already 16K aligned)
        // capped = min(2_097_152, 1_638_400) = 1_638_400
        let bw = compute_bandwidth(500_000_000, 8, 0, 0);
        assert_eq!(bw, 500_000_000);

        let size = compute_transfer_size(bw, UsbSpeed::Super);
        assert_eq!(size, MAX_TRANSFER_SIZE); // 100 * 16384, capped by MAX
        assert_eq!(size % USB3_BLOCK_SIZE, 0);
    }

    #[test]
    fn usb2_12mhz_4_digital() {
        // Logic 4, 4 digital @ 12 MHz, USB 2.0:
        // bandwidth = 4 * 12_000_000 / 8 = 6_000_000 bytes/sec
        // ideal = ceil(6_000_000 / 240) = 25_000
        // block = 4096 (USB 2.0)
        // rounded = ceil(25_000 / 4096) * 4096 = 7 * 4096 = 28_672
        let bw = compute_bandwidth(12_000_000, 4, 0, 0);
        assert_eq!(bw, 6_000_000);

        let size = compute_transfer_size(bw, UsbSpeed::High);
        assert_eq!(size, 28_672); // 7 * 4096
        assert_eq!(size % USB2_BLOCK_SIZE, 0);
    }

    #[test]
    fn usb2_25mhz_8_digital() {
        // Logic 8, 8 digital @ 25 MHz, USB 2.0:
        // bandwidth = 8 * 25_000_000 / 8 = 25_000_000 bytes/sec
        // ideal = ceil(25_000_000 / 240) = 104_167
        // block = 4096
        // rounded = ceil(104_167 / 4096) * 4096 = 26 * 4096 = 106_496
        let bw = compute_bandwidth(25_000_000, 8, 0, 0);
        assert_eq!(bw, 25_000_000);

        let size = compute_transfer_size(bw, UsbSpeed::High);
        assert_eq!(size, 106_496);
        assert_eq!(size % USB2_BLOCK_SIZE, 0);
    }

    #[test]
    fn minimum_clamping_very_low_bandwidth() {
        // Extremely low bandwidth: 1 byte/sec.
        // ideal = ceil(1 / 240) = 1
        // rounded up to block_size = 4096
        let size = compute_transfer_size(1, UsbSpeed::High);
        assert_eq!(size, USB2_BLOCK_SIZE);

        let size = compute_transfer_size(1, UsbSpeed::Super);
        assert_eq!(size, USB3_BLOCK_SIZE);
    }

    #[test]
    fn minimum_clamping_zero_bandwidth() {
        // Zero bandwidth should still return minimum block size.
        let size = compute_transfer_size(0, UsbSpeed::High);
        assert_eq!(size, USB2_BLOCK_SIZE);

        let size = compute_transfer_size(0, UsbSpeed::Super);
        assert_eq!(size, USB3_BLOCK_SIZE);
    }

    #[test]
    fn maximum_clamping_huge_bandwidth() {
        // Enormous bandwidth that would exceed MAX_TRANSFER_SIZE.
        let bw: u64 = 10_000_000_000; // 10 GB/sec
        let size = compute_transfer_size(bw, UsbSpeed::Super);
        // Should be capped to MAX_TRANSFER_SIZE aligned to 16384:
        // (1_638_400 / 16384) * 16384 = 100 * 16384 = 1_638_400
        // (0x190000 is exactly 16K aligned)
        assert_eq!(size, MAX_TRANSFER_SIZE);
        assert_eq!(size % USB3_BLOCK_SIZE, 0);

        let size = compute_transfer_size(bw, UsbSpeed::High);
        // USB 2.0: (1_638_400 / 4096) * 4096 = 400 * 4096 = 1_638_400
        assert_eq!(size, MAX_TRANSFER_SIZE);
        assert_eq!(size % USB2_BLOCK_SIZE, 0);
    }

    #[test]
    fn transfer_size_always_block_aligned() {
        // Test a range of bandwidths to verify block alignment.
        for bw in [
            100,
            1_000,
            50_000,
            500_000,
            5_000_000,
            50_000_000,
            500_000_000,
        ] {
            let size_usb2 = compute_transfer_size(bw, UsbSpeed::High);
            assert_eq!(
                size_usb2 % USB2_BLOCK_SIZE,
                0,
                "USB2 not aligned for bw={bw}: size={size_usb2}"
            );

            let size_usb3 = compute_transfer_size(bw, UsbSpeed::Super);
            assert_eq!(
                size_usb3 % USB3_BLOCK_SIZE,
                0,
                "USB3 not aligned for bw={bw}: size={size_usb3}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // compute_bandwidth
    // -----------------------------------------------------------------------

    #[test]
    fn bandwidth_digital_only() {
        // 8 digital channels at 100 MHz: 8 * 100M bits = 800M bits = 100M bytes.
        let bw = compute_bandwidth(100_000_000, 8, 0, 0);
        assert_eq!(bw, 100_000_000);
    }

    #[test]
    fn bandwidth_analog_only() {
        // 4 analog channels at 50 MHz, 2 bytes/sample:
        // 4 * 50M * 2 = 400M bytes/sec
        let bw = compute_bandwidth(50_000_000, 0, 4, 2);
        assert_eq!(bw, 400_000_000);
    }

    #[test]
    fn bandwidth_mixed() {
        // 8 digital at 500 MHz + 8 analog at 500 MHz, 2 bytes/sample:
        // digital: 8 * 500M bits = 4G bits
        // analog: 8 * 500M * 2 * 8 bits = 64G bits
        // total: 68G bits = 8.5G bytes
        let bw = compute_bandwidth(500_000_000, 8, 8, 2);
        assert_eq!(bw, 8_500_000_000);
    }

    #[test]
    fn bandwidth_pro16_full_config() {
        // 8 digital @ 500 MHz + 8 analog @ 50 MHz, 2 bytes/sample (12-bit as int16).
        // Note: uses single sample_rate, so this is a simplified model.
        // Digital: 8 * 500M / 8 = 500M bytes
        // This function takes a single sample rate, so we can't model
        // different rates for digital vs analog. In practice, the caller
        // would compute bandwidth for each rate separately.
        let bw = compute_bandwidth(500_000_000, 8, 0, 2);
        assert_eq!(bw, 500_000_000);
    }

    #[test]
    fn bandwidth_zero() {
        assert_eq!(compute_bandwidth(0, 8, 4, 2), 0);
        assert_eq!(compute_bandwidth(100_000_000, 0, 0, 2), 0);
    }

    #[test]
    fn bandwidth_single_digital() {
        // 1 digital channel at 1 MHz = 1M bits = 125_000 bytes.
        let bw = compute_bandwidth(1_000_000, 1, 0, 0);
        assert_eq!(bw, 125_000);
    }

    // -----------------------------------------------------------------------
    // TransferConfig::from_capture
    // -----------------------------------------------------------------------

    #[test]
    fn from_capture_usb3_digital() {
        let config = TransferConfig::from_capture(500_000_000, 8, 0, 0, UsbSpeed::Super);
        assert_eq!(config.block_size, USB3_BLOCK_SIZE);
        assert_eq!(config.queue_depth, QUEUE_DEPTH);
        // bandwidth = 500M bytes/sec -> capped at MAX_TRANSFER_SIZE (already 16K aligned)
        assert_eq!(config.transfer_size, MAX_TRANSFER_SIZE);
    }

    #[test]
    fn from_capture_usb2_digital() {
        let config = TransferConfig::from_capture(12_000_000, 4, 0, 0, UsbSpeed::High);
        assert_eq!(config.block_size, USB2_BLOCK_SIZE);
        assert_eq!(config.queue_depth, QUEUE_DEPTH);
        assert_eq!(config.transfer_size, 28_672);
    }

    #[test]
    fn total_buffer_bytes() {
        let config = TransferConfig {
            transfer_size: 16384,
            queue_depth: 128,
            block_size: 16384,
        };
        assert_eq!(config.total_buffer_bytes(), 16384 * 128);
    }

    // -----------------------------------------------------------------------
    // Environment variable override
    // -----------------------------------------------------------------------

    #[test]
    fn env_override_not_set() {
        // Remove the env var if present, then check.
        std::env::remove_var("SALEAE_USB_TRANSFER_LIMIT");
        assert_eq!(TransferConfig::from_env_override(), None);
    }

    #[test]
    fn env_override_set_valid() {
        std::env::set_var("SALEAE_USB_TRANSFER_LIMIT", "65536");
        let val = TransferConfig::from_env_override();
        assert_eq!(val, Some(65536));
        std::env::remove_var("SALEAE_USB_TRANSFER_LIMIT");
    }

    #[test]
    fn env_override_set_zero() {
        std::env::set_var("SALEAE_USB_TRANSFER_LIMIT", "0");
        assert_eq!(TransferConfig::from_env_override(), None);
        std::env::remove_var("SALEAE_USB_TRANSFER_LIMIT");
    }

    #[test]
    fn env_override_set_negative() {
        std::env::set_var("SALEAE_USB_TRANSFER_LIMIT", "-1");
        assert_eq!(TransferConfig::from_env_override(), None);
        std::env::remove_var("SALEAE_USB_TRANSFER_LIMIT");
    }

    #[test]
    fn env_override_set_non_numeric() {
        std::env::set_var("SALEAE_USB_TRANSFER_LIMIT", "abc");
        assert_eq!(TransferConfig::from_env_override(), None);
        std::env::remove_var("SALEAE_USB_TRANSFER_LIMIT");
    }

    // -----------------------------------------------------------------------
    // Block size helper
    // -----------------------------------------------------------------------

    #[test]
    fn block_sizes() {
        assert_eq!(block_size_for_speed(UsbSpeed::Low), USB2_BLOCK_SIZE);
        assert_eq!(block_size_for_speed(UsbSpeed::Full), USB2_BLOCK_SIZE);
        assert_eq!(block_size_for_speed(UsbSpeed::High), USB2_BLOCK_SIZE);
        assert_eq!(block_size_for_speed(UsbSpeed::Super), USB3_BLOCK_SIZE);
        assert_eq!(block_size_for_speed(UsbSpeed::SuperPlus), USB3_BLOCK_SIZE);
    }

    // -----------------------------------------------------------------------
    // Exact match with RE examples
    // -----------------------------------------------------------------------

    #[test]
    fn re_example_logic_pro16_full() {
        // From re/notes/transfer-queue-sizing.md:
        // 8 digital @ 500 MHz + 8 analog @ 50 MHz, 12-bit (but single rate API).
        // We test the digital portion only since our API has a single rate.
        //
        // Digital only: 8 ch * 500 MHz / 8 = 500 MB/s
        // ideal = ceil(500_000_000 / 240) = 2_083_334
        // rounded = 128 * 16384 = 2_097_152
        // max_aligned = 100 * 16384 = 1_638_400 (0x190000 is 16K aligned)
        // final = 1_638_400
        let bw = compute_bandwidth(500_000_000, 8, 0, 0);
        let size = compute_transfer_size(bw, UsbSpeed::Super);
        assert_eq!(size, MAX_TRANSFER_SIZE);
    }

    #[test]
    fn re_example_logic8_8ch_25mhz() {
        // From re/notes/transfer-queue-sizing.md:
        // 8 digital @ 25 MHz, USB 2.0:
        // bandwidth = 25_000_000 bytes/sec
        // ideal = ceil(25_000_000 / 240) = 104_167
        // rounded = ceil(104_167 / 4096) * 4096 = 26 * 4096 = 106_496
        let bw = compute_bandwidth(25_000_000, 8, 0, 0);
        assert_eq!(bw, 25_000_000);

        let size = compute_transfer_size(bw, UsbSpeed::High);
        assert_eq!(size, 106_496);
    }

    #[test]
    fn re_example_logic4_4ch_12mhz() {
        // From re/notes/transfer-queue-sizing.md:
        // 4 digital @ 12 MHz, USB 2.0:
        // bandwidth = 6_000_000 bytes/sec
        // ideal = ceil(6_000_000 / 240) = 25_000
        // rounded = ceil(25_000 / 4096) * 4096 = 7 * 4096 = 28_672
        let bw = compute_bandwidth(12_000_000, 4, 0, 0);
        assert_eq!(bw, 6_000_000);

        let size = compute_transfer_size(bw, UsbSpeed::High);
        assert_eq!(size, 28_672);
    }
}
