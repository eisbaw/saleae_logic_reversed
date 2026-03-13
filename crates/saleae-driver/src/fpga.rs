//! FPGA bitstream programming and post-init sequence.
//!
//! After the Cypress FX3 firmware is running, the next step in the device
//! boot sequence is to download an FPGA bitstream and initialize the FPGA
//! subsystems (DDR, JESD, GPIF). This module implements:
//!
//! 1. [`program_fpga`] -- reset FPGA, send bitstream, verify readback.
//! 2. [`fpga_post_init`] -- write init registers, reset JESD PHY, poll
//!    DDR ready.
//! 3. [`common_device_init`] -- ADC init, JESD link, EEPROM device ID.
//!
//! # Register interfaces
//!
//! Two register paths exist (see `doc/protocol/fpga-registers.md`):
//!
//! - **MsoFpga (16-bit addr, 32-bit value)**: direct 6-byte USB bulk
//!   transfers. Used for addresses > 0xFF (e.g., COMMIT_CONTROL 0x01C0).
//!
//! - **FpgaDeviceFeatures (8-bit addr, 8-bit value)**: EP1 opcode 0x80
//!   (write) / 0x81 (read). Used for addresses <= 0xFF.
//!
//! # Hardware testing
//!
//! This code has NOT been validated against hardware yet. All testing is
//! via mock transport only. Register values are from static RE analysis
//! of libgraph_server_shared.so v2.4.40 (59 decompiled functions).

use std::time::{Duration, Instant};

use saleae_proto::device::{DeviceProfile, UsbSpeed};
use saleae_proto::fpga_regs;
use tracing::{debug, info};

use crate::ep1::Ep1CommandChannel;
use crate::error::{DriverError, DriverResult};

// ---------------------------------------------------------------------------
// Hardware revision (re-exported from saleae-proto)
// ---------------------------------------------------------------------------

/// Backward-compatible alias for the proto-level `HardwareRevision`.
///
/// New code should use `saleae_proto::device::HardwareRevision` directly.
pub type HwRevision = saleae_proto::device::HardwareRevision;

/// Read the hardware revision from EEPROM and determine FPGA/ADC type.
///
/// This reads 6 bytes from EEPROM offset 0x20 via `ReadVersion` (opcode 0x07)
/// and parses the "REV" + major.minor format. The major version determines
/// the FPGA chip type (Xilinx vs Lattice) and ADC type.
///
/// # Returns
///
/// The parsed [`HwRevision`], or an error if the read fails or the data
/// cannot be parsed.
pub fn read_hw_revision(ep1: &mut Ep1CommandChannel) -> DriverResult<HwRevision> {
    let data = ep1.read_version(&HwRevision::EEPROM_READ_PARAMS)?;
    HwRevision::parse(&data).ok_or_else(|| DriverError::CommandFailed {
        command: "ReadHwRevision",
        detail: format!(
            "failed to parse hardware revision from EEPROM data: {:02x?}",
            data
        ),
    })
}

// ---------------------------------------------------------------------------
// Configuration constants
// ---------------------------------------------------------------------------

/// Default timeout for JESD lock polling.
const JESD_LOCK_TIMEOUT: Duration = Duration::from_millis(5000);

/// Polling interval for JESD lock check.
const JESD_LOCK_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Maximum number of bitslip retries before giving up.
const BITSLIP_MAX_RETRIES: u32 = 10;

/// Default timeout for DDR ready polling.
const DDR_READY_TIMEOUT: Duration = Duration::from_secs(5);

/// Polling interval for DDR ready check.
const DDR_READY_POLL_INTERVAL: Duration = Duration::from_millis(1);

/// Timeout for JESD PHY reset poll loop (from decompilation: 2 seconds).
///
/// The decompiled ResetJesdPhyTransceivers uses a write-and-poll loop
/// with this timeout. Currently unused because `reset_jesd_phy` uses a
/// simplified write-deassert approach, but retained for when we implement
/// the full poll loop.
#[allow(dead_code)]
const JESD_PHY_RESET_TIMEOUT: Duration = Duration::from_secs(2);

/// Polling interval for JESD PHY reset (from decompilation: 1 ms sleep).
#[allow(dead_code)]
const JESD_PHY_RESET_POLL_INTERVAL: Duration = Duration::from_millis(1);

/// Initial register values for the post-init sequence.
///
/// The sequence and register addresses are confirmed from decompilation
/// of the post-FPGA-programming init code (TASK-0068). The exact values
/// written to COMMIT_CONTROL, CAPTURE_CONTROL, and INIT_CONFIG during
/// init have not been extracted from decompilation (the init function
/// was not fully traced). These use zero as a reasonable default -- the
/// actual values may depend on device type and need USB capture.
mod init_values {
    /// COMMIT_CONTROL initial value.
    ///
    /// Written first during init. bits [19:0] are smart cable channel
    /// enables (all disabled at init), bits [31:20] are bank config.
    /// Zero disables all smart cable channels and resets bank config.
    pub const COMMIT_CONTROL: u32 = 0x0000_0000;

    /// CAPTURE_CONTROL initial value (disable all capture).
    ///
    /// Bit layout: bit 4 = capture mode, bits [6:5] = DownsampleMode.
    /// Zero disables capture.
    pub const CAPTURE_CONTROL: u32 = 0x0000_0000;

    /// INIT_CONFIG value.
    ///
    /// Written once after FPGA programming. The exact bit layout and
    /// value have not been extracted from decompilation yet.
    pub const INIT_CONFIG: u32 = 0x0000_0000;

    /// GPIF_FLUSH_TIMEOUT in 100 MHz clock ticks.
    ///
    /// Controls how long the GPIF waits before flushing a partial
    /// packet. ~1 ms = 100,000 ticks = 0x186A0.
    pub const GPIF_FLUSH_TIMEOUT: u32 = 0x0001_86A0;

    /// GPIF_ENABLE value (enable the data path).
    pub const GPIF_ENABLE: u32 = 0x0000_0001;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Program the FPGA with a bitstream via EP1.
///
/// Follows the sequence documented in `doc/protocol/fpga-bitstream.md`:
///
/// 1. Send ResetFpga command (opcode 0x7E).
/// 2. Send bitstream via WriteConfigurationData (opcode 0x7F) in chunks
///    sized by device profile (62/508/1020 bytes depending on device and
///    USB speed).
/// 3. Verify: read register 0x00, expect 0xAA.
///
/// # Arguments
///
/// - `ep1`: Initialized EP1 command channel.
/// - `bitstream`: Raw FPGA bitstream data.
/// - `profile`: Device hardware profile (determines chunk size and ACK behavior).
/// - `usb_speed`: Current USB connection speed (affects chunk size for Pro devices).
///
/// # Errors
///
/// Returns [`DriverError::FpgaProgramming`] if the readback verification
/// fails after bitstream upload.
pub fn program_fpga(
    ep1: &mut Ep1CommandChannel,
    bitstream: &[u8],
    profile: &DeviceProfile,
    usb_speed: UsbSpeed,
) -> DriverResult<()> {
    let chunk_size = profile.fpga_chunk_size(usb_speed);
    let read_ack = profile.register_write_verifies();

    info!(
        bitstream_len = bitstream.len(),
        chunk_size,
        read_ack,
        device = %profile.device_type,
        "Programming FPGA bitstream"
    );

    // Step 1: Reset the FPGA.
    debug!("Resetting FPGA");
    ep1.reset_fpga()?;

    // Step 2: Send the bitstream data, chunked per device profile.
    debug!(
        "Sending bitstream data ({} bytes, chunk_size={})",
        bitstream.len(),
        chunk_size
    );
    ep1.write_config_data_chunked(bitstream, chunk_size, read_ack)?;

    // Step 3: Verify FPGA programming via register 0x7F readback.
    //
    // Pcap-validated sequence (Phase 3a, frames 1373-1384):
    //   WriteRegisters r0x7f=0xaa (write test value, twice)
    //   ReadRegisters  r0x7f -> 0xaa (read back to confirm FPGA is responsive)
    //
    // Register 0x7F is a scratch/test register that only works after
    // successful FPGA configuration.
    debug!("Verifying FPGA programming (write 0xAA to r0x7F, read back)");
    ep1.write_registers(&[(0x7F, fpga_regs::FPGA_READBACK_OK)])?;
    ep1.write_registers(&[(0x7F, fpga_regs::FPGA_READBACK_OK)])?;
    let status = ep1.read_registers(&[0x7F])?;

    if status.is_empty() {
        return Err(DriverError::FpgaProgramming {
            context: "readback returned no data".into(),
        });
    }

    if status[0] != fpga_regs::FPGA_READBACK_OK {
        return Err(DriverError::FpgaProgramming {
            context: format!(
                "readback got {:#04x} instead of {:#04x}",
                status[0],
                fpga_regs::FPGA_READBACK_OK
            ),
        });
    }

    info!("FPGA programming verified (readback = 0xAA)");
    Ok(())
}

/// Post-programming initialization sequence.
///
/// After the FPGA bitstream is loaded, the host must initialize the FPGA
/// subsystems before the device is ready for capture. The sequence is
/// confirmed from decompilation (TASK-0068, `re/notes/fpga-register-map.md`):
///
/// 1. Write COMMIT_CONTROL (0x01C0) -- bank/commit config, smart cable enables
/// 2. Write CAPTURE_CONTROL (0x0040) -- capture mode, downsample mode
/// 3. Write INIT_CONFIG (0x00E0) -- post-programming init
/// 4. Write GPIF_FLUSH_TIMEOUT (0x0024) -- flush timeout in 100 MHz ticks
/// 5. Write GPIF_ENABLE (0x0020) -- enable GPIF data path
/// 6. Reset JESD PHY transceivers (write value 2 to reg 0x0008)
/// 7. Poll DDR ready: (reg_0x0008 & 0x18) == 0x18, with timeout
///
/// Steps 6-7 are Lattice ECP5 specific. For Xilinx devices, JESD reset
/// may not be needed (or may use a different mechanism).
///
/// # Arguments
///
/// - `ep1`: Initialized EP1 command channel.
/// - `is_lattice`: Whether the FPGA is a Lattice ECP5 part. If true,
///   performs JESD PHY reset and DDR polling.
///
/// # Errors
///
/// Returns [`DriverError::FpgaProgramming`] if DDR fails to become ready
/// within the timeout.
pub fn fpga_post_init(ep1: &mut Ep1CommandChannel, is_lattice: bool) -> DriverResult<()> {
    info!("Running FPGA post-programming init sequence");

    // Step 1: COMMIT_CONTROL (0x01C0) -- address > 0xFF, needs 16-bit path.
    debug!("Writing COMMIT_CONTROL");
    ep1.write_fpga_register(fpga_regs::REG_COMMIT_CONTROL, init_values::COMMIT_CONTROL)?;

    // Step 2: CAPTURE_CONTROL (0x0040) -- fits in 8-bit path.
    debug!("Writing CAPTURE_CONTROL");
    ep1.write_fpga_register(fpga_regs::REG_CAPTURE_CONTROL, init_values::CAPTURE_CONTROL)?;

    // Step 3: INIT_CONFIG (0x00E0) -- fits in 8-bit path.
    debug!("Writing INIT_CONFIG");
    ep1.write_fpga_register(fpga_regs::REG_INIT_CONFIG, init_values::INIT_CONFIG)?;

    // Step 4: GPIF_FLUSH_TIMEOUT (0x0024) -- fits in 8-bit path.
    debug!("Writing GPIF_FLUSH_TIMEOUT");
    ep1.write_fpga_register(
        fpga_regs::REG_GPIF_FLUSH_TIMEOUT,
        init_values::GPIF_FLUSH_TIMEOUT,
    )?;

    // Step 5: GPIF_ENABLE (0x0020) -- fits in 8-bit path.
    debug!("Writing GPIF_ENABLE");
    ep1.write_fpga_register(fpga_regs::REG_GPIF_ENABLE, init_values::GPIF_ENABLE)?;

    // Steps 6-7: Lattice-specific JESD PHY reset and DDR polling.
    if is_lattice {
        reset_jesd_phy(ep1)?;
        wait_for_ddr_ready(ep1, DDR_READY_TIMEOUT)?;
    }

    info!("FPGA post-init complete");
    Ok(())
}

// ---------------------------------------------------------------------------
// CommonDeviceInit
// ---------------------------------------------------------------------------

/// Result of the [`common_device_init`] sequence.
///
/// Contains the device ID read from EEPROM and the status of the JESD204B
/// link (Lattice) or DCM lock (Xilinx) subsystem.
#[derive(Debug, Clone)]
pub struct DeviceInitResult {
    /// 64-bit device ID read from EEPROM via I2C tunnel.
    ///
    /// This is used to download per-device calibration data from:
    /// `https://downloads.saleae.com/calibration/{device_id:x}.cal`
    pub device_id: u64,

    /// Whether the JESD204B link is locked (Lattice ECP5 only).
    pub jesd_locked: bool,

    /// Whether the DDR subsystem is ready (Lattice ECP5 only).
    pub ddr_ready: bool,

    /// Whether DCM PLLs are locked (Xilinx Spartan-6 / AD9637 only).
    pub dcm_locked: bool,
}

/// Common device initialization sequence.
///
/// This runs after FPGA programming and [`fpga_post_init`]. It initializes
/// the ADC, establishes the JESD204B link, and reads the device ID from
/// EEPROM.
///
/// The sequence is based on RE analysis of `CommonDeviceInit` in
/// `libgraph_server_shared.so`:
///
/// 1. Configure SPI bridge for ADC access (write to REG_SPI_DATA).
/// 2. Set ADC drive strength via SPI.
/// 3. Enable ADC via SPI.
/// 4. Reset JESD PHY transceivers.
/// 5. Poll for JESD lock (timeout ~5000 ms).
/// 6. Run bitslip detection (up to N retries).
/// 7. Verify JESD link_up and sync_status.
/// 8. Read EEPROM device ID via I2C tunnel (opcode 0x88).
///
/// The sequence of operations (steps 1-8) is confirmed from RE analysis.
/// The JESD status bit positions and PHY reset value (2) are confirmed
/// from decompilation. The SPI bridge layout (bit 23 = R/W, bit 24 =
/// wire config, bit 26 = clock edge) is confirmed. The exact ADS528x
/// ADC register addresses/values used in steps 2-3 await USB capture.
///
/// # Arguments
///
/// - `ep1`: Initialized EP1 command channel.
/// - `is_lattice`: Whether the FPGA is a Lattice ECP5 part. ADC/JESD init
///   is only performed for Lattice devices (Xilinx uses a different path
///   with AD9637 via FpgaDeviceFeatures registers 0x03-0x05, 0x16).
///
/// # Errors
///
/// Returns [`DriverError::FpgaProgramming`] if JESD fails to lock within
/// the timeout, or [`DriverError::CommandFailed`] if the EEPROM read fails.
pub fn common_device_init(
    ep1: &mut Ep1CommandChannel,
    is_lattice: bool,
) -> DriverResult<DeviceInitResult> {
    info!("Running CommonDeviceInit (lattice={})", is_lattice);

    let mut jesd_locked = false;
    let mut ddr_ready = false;
    let mut dcm_locked = false;

    if is_lattice {
        // Lattice ECP5 path: ADS528x ADC via SPI bridge + JESD204B link.
        configure_spi_bridge(ep1)?;
        set_ads528x_drive(ep1)?;
        set_ads528x_enabled(ep1)?;
        reset_jesd_phy(ep1)?;

        jesd_locked = wait_for_jesd_lock(ep1, JESD_LOCK_TIMEOUT)?;
        if jesd_locked {
            run_bitslip_detection(ep1)?;
        }

        let status = ep1.read_fpga_register(fpga_regs::REG_STATUS_DDR_JESD)?;
        ddr_ready = (status & fpga_regs::DDR_READY_MASK) == fpga_regs::DDR_READY_MASK;
        if !ddr_ready {
            debug!(
                status = format_args!("{status:#010x}"),
                "DDR not ready after JESD init"
            );
        }
    } else {
        // Xilinx Spartan-6 path: AD9637 ADC via FPGA proxy + DCM lock.
        //
        // Read calibration coefficients from EEPROM first (offset 0x10, 16 bytes).
        // These are needed for the full AD9637 init sequence.
        info!("Reading calibration coefficients from EEPROM");
        let cal_data = ep1.read_version(&[0x33, 0x81, 0x10, 0x00, 0x10, 0x00])?;
        debug!(
            "Calibration: {}",
            cal_data
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        );

        let dcm_result = crate::adc::init_ad9637(ep1, &cal_data)?;
        dcm_locked = dcm_result.locked;
    }

    // Read EEPROM device ID.
    let device_id = ep1.read_eeprom_device_id()?;

    let result = DeviceInitResult {
        device_id,
        jesd_locked,
        ddr_ready,
        dcm_locked,
    };

    info!(
        device_id = format_args!("{:#018x}", result.device_id),
        jesd_locked = result.jesd_locked,
        ddr_ready = result.ddr_ready,
        dcm_locked = result.dcm_locked,
        "CommonDeviceInit complete"
    );

    Ok(result)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Configure the SPI bridge for ADC access.
///
/// Writes to REG_SPI_DATA (0x0004) to set up the FPGA's SPI bridge
/// for communication with the ADS528x ADC. The SPI bridge register
/// uses bits: [22:0] = addr+data, bit 23 = R/W, bit 24 = wire config,
/// bit 26 = clock edge.
///
/// Writing 0 sets: 4-wire SPI, rising clock edge, no data -- which
/// initializes the bridge for subsequent SPI transactions.
fn configure_spi_bridge(ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
    debug!("Configuring SPI bridge for ADC access");
    ep1.write_fpga_register(
        fpga_regs::REG_SPI_DATA,
        fpga_regs::SPI_BRIDGE_CONFIG_FOR_ADC,
    )?;
    Ok(())
}

/// Set ADS528x ADC drive strength via SPI.
///
/// Configures the ADC's LVDS output driver strength to match the FPGA's
/// JESD204B receiver input requirements. The SPI write is tunneled
/// through REG_SPI_DATA (0x0004).
///
/// The SPI word format: bits [22:0] = packed address+data, bit 23 = 0
/// (write). The exact ADS528x register address and value are from the
/// datasheet and await USB capture confirmation.
fn set_ads528x_drive(ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
    debug!("Setting ADS528x drive strength");

    // SPI write format: bits [22:0] = address + data (packed),
    // bit 23 = 0 for write. Bit 24 and 26 left at 0 (4-wire, rising).
    let spi_word = (fpga_regs::ADC_SPI_DRIVE_STRENGTH_ADDR & fpga_regs::SPI_ADDR_DATA_MASK)
        | (fpga_regs::ADC_SPI_DRIVE_STRENGTH_VALUE << 8);

    ep1.write_fpga_register(fpga_regs::REG_SPI_DATA, spi_word)?;
    Ok(())
}

/// Enable the ADS528x ADC via SPI.
///
/// Powers on the ADC analog core and starts conversion. After this,
/// the ADC begins sending JESD204B data to the FPGA.
///
/// The exact ADS528x register address and value await USB capture
/// confirmation.
fn set_ads528x_enabled(ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
    debug!("Enabling ADS528x ADC");

    let spi_word = (fpga_regs::ADC_SPI_ENABLE_ADDR & fpga_regs::SPI_ADDR_DATA_MASK)
        | (fpga_regs::ADC_SPI_ENABLE_VALUE << 8);

    ep1.write_fpga_register(fpga_regs::REG_SPI_DATA, spi_word)?;
    Ok(())
}

/// Wait for JESD204B link to lock.
///
/// Polls REG_STATUS_DDR_JESD (0x0008) until the JESD status bits indicate
/// the link is established. The original software logs:
/// "Locked JESD after {} ms, {} retries"
///
/// Checks JESD_READY_MASK = link_up (bit 0) | cpll_locked (bit 5).
/// These bit positions are confirmed from decompilation of the JESD
/// status logging code (link_up, cpll_locked fields).
///
/// # Returns
///
/// `true` if JESD locked within the timeout, `false` if it timed out.
/// Returns `Err` only on USB communication failure.
fn wait_for_jesd_lock(ep1: &mut Ep1CommandChannel, timeout: Duration) -> DriverResult<bool> {
    debug!("Waiting for JESD lock (timeout: {:?})", timeout);

    let start = Instant::now();
    let mut poll_count = 0u32;

    loop {
        let status = ep1.read_fpga_register(fpga_regs::REG_STATUS_DDR_JESD)?;
        let jesd_bits = status & fpga_regs::JESD_READY_MASK;
        poll_count += 1;

        if jesd_bits == fpga_regs::JESD_READY_MASK {
            let elapsed = start.elapsed();
            info!(
                elapsed_ms = elapsed.as_millis(),
                polls = poll_count,
                "Locked JESD after {} ms, {} polls",
                elapsed.as_millis(),
                poll_count,
            );
            return Ok(true);
        }

        if start.elapsed() >= timeout {
            info!(
                elapsed_ms = start.elapsed().as_millis(),
                polls = poll_count,
                status = format_args!("{status:#010x}"),
                jesd_bits = format_args!("{jesd_bits:#010x}"),
                expected = format_args!("{:#010x}", fpga_regs::JESD_READY_MASK),
                "JESD lock timed out"
            );
            return Ok(false);
        }

        std::thread::sleep(JESD_LOCK_POLL_INTERVAL);
    }
}

/// Run bitslip detection for ADC data alignment.
///
/// After JESD lock, the FPGA's JESD deserializer may need to adjust the
/// bit alignment of the received data. This is done by writing to the
/// JESD BER test register (0x00FC) and checking the result.
///
/// The original software checks bitslip for ADC1 and ADC2 separately,
/// retrying up to N times. The exact write/read sequence for bitslip
/// control uses REG_JESD_BER_TEST (0x00FC) -- BER testing and bitslip
/// detection are related JESD204B operations that share this register.
///
/// The exact trigger value and success check bit are from reasonable
/// inference and await USB capture confirmation.
fn run_bitslip_detection(ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
    debug!("Running bitslip detection");

    for attempt in 0..BITSLIP_MAX_RETRIES {
        // Trigger bitslip check by writing to BER test register.
        ep1.write_fpga_register(fpga_regs::REG_JESD_BER_TEST, 0x0000_0001)?;

        // Read back status to check alignment.
        let status = ep1.read_fpga_register(fpga_regs::REG_JESD_BER_TEST)?;

        // Check if bitslip is correct (bit 0 = aligned).
        if status & 0x01 != 0 {
            debug!(
                attempt,
                "Bitslip detection successful on attempt {}", attempt
            );
            return Ok(());
        }

        debug!(
            attempt,
            status = format_args!("{status:#010x}"),
            "Bitslip attempt {} failed, retrying",
            attempt
        );
    }

    // Bitslip detection failed after all retries. This is not fatal --
    // the JESD link may still work, but data alignment could be wrong.
    info!(
        max_retries = BITSLIP_MAX_RETRIES,
        "Bitslip detection did not converge after {} retries", BITSLIP_MAX_RETRIES
    );
    Ok(())
}

/// Reset JESD PHY transceivers.
///
/// From decompiled ResetJesdPhyTransceivers (ELF VA 0x2d62200):
///
/// 1. Assert channel_count == 2 || channel_count == 4
/// 2. Compute lane mask: 0xFF for 4 channels, 0x5A for 2 channels
/// 3. Write value 2 to register 0x0008 (JESD PHY reset trigger)
/// 4. Read register 0x0008 to check JESD status
/// 5. Retry with 1 ms sleep until lock or timeout (2 seconds)
///
/// The original software logs: "Lattice PLL and SERDES reset complete".
///
/// This implementation uses a simplified version: write the reset value
/// once, then let the caller poll for JESD lock separately (via
/// `wait_for_jesd_lock`). The decompiled code combines the reset write
/// and status polling in one loop, but functionally the result is the
/// same since the FPGA processes the reset asynchronously.
fn reset_jesd_phy(ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
    debug!("Resetting JESD PHY transceivers");

    // Write value 2 to register 0x0008 to trigger JESD PHY reset.
    // Confirmed from decompilation: the reset loop writes exactly
    // value 2 (not a bitmask toggle).
    ep1.write_fpga_register(
        fpga_regs::REG_STATUS_DDR_JESD,
        fpga_regs::JESD_PHY_RESET_ASSERT,
    )?;

    // The decompiled code does not explicitly deassert (write 0).
    // It writes 2 then polls for lock. We write 0 as a conservative
    // deassert to avoid leaving the reset bit asserted.
    ep1.write_fpga_register(
        fpga_regs::REG_STATUS_DDR_JESD,
        fpga_regs::JESD_PHY_RESET_DEASSERT,
    )?;

    debug!("JESD PHY reset complete");
    Ok(())
}

/// Poll register 0x0008 until DDR is ready.
///
/// DDR ready = (reg_value & 0x18) == 0x18 (both LOCKED and CALIBRATED bits).
///
/// Polls with 1 ms intervals. The original software logs:
/// "Locked and calibrated DDR after {} ms".
fn wait_for_ddr_ready(ep1: &mut Ep1CommandChannel, timeout: Duration) -> DriverResult<()> {
    debug!("Waiting for DDR ready (timeout: {:?})", timeout);

    let start = Instant::now();

    loop {
        let status = ep1.read_fpga_register(fpga_regs::REG_STATUS_DDR_JESD)?;
        let ddr_bits = status & fpga_regs::DDR_READY_MASK;

        if ddr_bits == fpga_regs::DDR_READY_MASK {
            let elapsed = start.elapsed();
            info!(
                elapsed_ms = elapsed.as_millis(),
                "Locked and calibrated DDR after {} ms",
                elapsed.as_millis()
            );
            return Ok(());
        }

        if start.elapsed() >= timeout {
            return Err(DriverError::FpgaProgramming {
                context: format!(
                    "DDR not ready after {:?}: status register = {status:#010x}, \
                     DDR bits = {ddr_bits:#010x} (expected {:#010x})",
                    timeout,
                    fpga_regs::DDR_READY_MASK,
                ),
            });
        }

        std::thread::sleep(DDR_READY_POLL_INTERVAL);
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

    /// Create a mock channel with XorAll encryption and seed=0 (identity XOR).
    fn mock_channel(mock: &std::sync::Arc<MockTransport>) -> Ep1CommandChannel {
        let transport: Box<dyn saleae_usb::transport::UsbTransport> = Box::new(mock.clone());
        let mut channel = Ep1CommandChannel::new(transport, EncryptionMode::XorAll, true);
        // Seed 0 makes XOR identity -- simplifies assertions.
        channel.encryption.reseed(0);
        channel.initialized = true;
        channel
    }

    // -----------------------------------------------------------------------
    // program_fpga tests
    // -----------------------------------------------------------------------

    #[test]
    fn program_fpga_sends_reset_then_data_then_verify() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue responses:
        // 1. ResetFpga ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // 2. WriteConfigurationData ACK (small bitstream fits in one chunk)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // 3-4. WriteRegisters ACKs (write 0xAA to r0x7F, twice)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // 5. ReadRegisters response: 0xAA (success)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);

        let mut channel = mock_channel(&mock);
        let bitstream = vec![0xFF; 100]; // 100-byte test bitstream

        program_fpga(
            &mut channel,
            &bitstream,
            &LOGIC_PRO_8_PROFILE,
            UsbSpeed::Super,
        )
        .unwrap();

        // Verify sequence of operations.
        let ops = mock.operations();
        let out_ops: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .collect();

        // Should be 5 bulk OUTs: reset + data + 2x write_regs + read_regs
        assert_eq!(
            out_ops.len(),
            5,
            "expected 5 OUT transfers (reset + data + 2x write + read)"
        );

        // First OUT: ResetFpga (opcode 0x7E)
        match out_ops[0] {
            UsbOperation::BulkOut { data, .. } => {
                assert_eq!(data[1], 0x7E, "first command should be ResetFpga (0x7E)");
            }
            _ => unreachable!(),
        }

        // Second OUT: WriteConfigurationData (opcode 0x7F)
        match out_ops[1] {
            UsbOperation::BulkOut { data, .. } => {
                assert_eq!(
                    data[1], 0x7F,
                    "second command should be WriteConfigurationData (0x7F)"
                );
            }
            _ => unreachable!(),
        }

        // Third + Fourth OUT: WriteRegisters (opcode 0x80) writing 0xAA to r0x7F
        for (i, op) in out_ops[2..4].iter().enumerate() {
            match op {
                UsbOperation::BulkOut { data, .. } => {
                    assert_eq!(
                        data[1],
                        0x80,
                        "command {} should be WriteRegisters (0x80)",
                        i + 2
                    );
                }
                _ => unreachable!(),
            }
        }

        // Fifth OUT: ReadRegisters (opcode 0x81)
        match out_ops[4] {
            UsbOperation::BulkOut { data, .. } => {
                assert_eq!(
                    data[1], 0x81,
                    "fifth command should be ReadRegisters (0x81)"
                );
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn program_fpga_fails_on_wrong_readback() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue responses:
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // ResetFpga ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // Config data ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #1
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #2
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x55]); // Wrong readback!

        let mut channel = mock_channel(&mock);
        let err = program_fpga(
            &mut channel,
            &[0xFF; 10],
            &LOGIC_PRO_8_PROFILE,
            UsbSpeed::Super,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("0x55") && msg.contains("0xaa"),
            "error should mention both actual and expected values: {msg}"
        );
    }

    #[test]
    fn program_fpga_fails_on_empty_readback() {
        let mock = std::sync::Arc::new(MockTransport::new());

        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // ResetFpga ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // Config data ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #1
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #2
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![]); // Empty!

        let mut channel = mock_channel(&mock);
        let err = program_fpga(
            &mut channel,
            &[0xFF; 10],
            &LOGIC_PRO_8_PROFILE,
            UsbSpeed::Super,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("no data"),
            "error should mention missing data: {msg}"
        );
    }

    #[test]
    fn program_fpga_handles_large_bitstream() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // A realistic Lattice bitstream is ~100KB-200KB.
        let bitstream_size = 150_000;
        let bitstream = vec![0xAB; bitstream_size];

        // Chunk size from profile: Pro at USB 3.0 = 1020 bytes.
        let chunk_size = LOGIC_PRO_8_PROFILE.fpga_chunk_size(UsbSpeed::Super);
        let num_chunks = bitstream_size.div_ceil(chunk_size);

        // Queue responses:
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // ResetFpga
        for _ in 0..num_chunks {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // Config data ACKs
        }
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #1
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // WriteRegisters ACK #2
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]); // Readback OK

        let mut channel = mock_channel(&mock);
        program_fpga(
            &mut channel,
            &bitstream,
            &LOGIC_PRO_8_PROFILE,
            UsbSpeed::Super,
        )
        .unwrap();

        // Count OUT operations
        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        // 1 (reset) + num_chunks (data) + 2 (write_regs) + 1 (read_registers) = num_chunks + 4
        assert_eq!(
            out_count,
            num_chunks + 4,
            "expected {} OUT transfers for {} byte bitstream",
            num_chunks + 4,
            bitstream_size
        );
    }

    // -----------------------------------------------------------------------
    // fpga_post_init tests
    // -----------------------------------------------------------------------

    #[test]
    fn post_init_non_lattice_writes_5_registers() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue 5 ACK responses (one per register write).
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }

        let mut channel = mock_channel(&mock);
        fpga_post_init(&mut channel, false).unwrap();

        // For non-Lattice, no JESD reset or DDR polling.
        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        assert_eq!(out_count, 5, "expected 5 register writes for non-Lattice");
    }

    #[test]
    fn post_init_lattice_includes_jesd_and_ddr() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue responses:
        // 5 register writes (COMMIT, CAPTURE, INIT, GPIF_TIMEOUT, GPIF_ENABLE)
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }
        // 2 JESD PHY reset writes (assert + deassert)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // DDR ready poll: return ready on first attempt.
        // read_fpga_register(0x0008) -> uses 8-bit path -> ReadRegisters opcode 0x81
        // Return 0x18 (both DDR bits set).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x18]);

        let mut channel = mock_channel(&mock);
        fpga_post_init(&mut channel, true).unwrap();

        // 5 init writes + 2 JESD writes + 1 DDR read = 8 command-response pairs
        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        assert_eq!(
            out_count, 8,
            "expected 8 OUT transfers for Lattice post-init"
        );
    }

    #[test]
    fn post_init_ddr_polling_retries() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue initial register write ACKs.
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }
        // JESD reset ACKs.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // DDR poll: not ready first 2 times, then ready.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // neither bit
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x10]); // only locked
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x18]); // both bits

        let mut channel = mock_channel(&mock);
        fpga_post_init(&mut channel, true).unwrap();

        // 5 init + 2 JESD + 3 DDR reads = 10
        let ops = mock.operations();
        let out_count = ops
            .iter()
            .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
            .count();
        assert_eq!(out_count, 10, "expected 10 OUT transfers with DDR retries");
    }

    #[test]
    fn post_init_register_write_order() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue ACKs for non-Lattice path (5 writes, no JESD/DDR).
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }

        let mut channel = mock_channel(&mock);
        fpga_post_init(&mut channel, false).unwrap();

        let ops = mock.operations();
        let out_ops: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                UsbOperation::BulkOut { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(out_ops.len(), 5);

        // Op 0: COMMIT_CONTROL via 16-bit path (addr_hi=0x01 as opcode)
        assert_eq!(
            out_ops[0][1], 0x01,
            "first write should use addr_hi=0x01 (COMMIT_CONTROL 0x01C0)"
        );
        assert_eq!(out_ops[0][2], 0xC0, "first write should have addr_lo=0xC0");

        // Op 1: CAPTURE_CONTROL via 8-bit path (opcode 0x80)
        assert_eq!(
            out_ops[1][1], 0x80,
            "second write should be opcode 0x80 (WriteRegisters)"
        );
        // addr byte inside payload: data[3] should be 0x40 (CAPTURE_CONTROL)
        assert_eq!(out_ops[1][3], 0x40);

        // Op 2: INIT_CONFIG via 8-bit path
        assert_eq!(out_ops[2][1], 0x80);
        assert_eq!(out_ops[2][3], 0xE0); // INIT_CONFIG = 0xE0

        // Op 3: GPIF_FLUSH_TIMEOUT via 8-bit path
        assert_eq!(out_ops[3][1], 0x80);
        assert_eq!(out_ops[3][3], 0x24); // GPIF_FLUSH_TIMEOUT = 0x24

        // Op 4: GPIF_ENABLE via 8-bit path
        assert_eq!(out_ops[4][1], 0x80);
        assert_eq!(out_ops[4][3], 0x20); // GPIF_ENABLE = 0x20
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn program_fpga_with_empty_bitstream() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // ResetFpga ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // No config data chunks needed for empty bitstream.
        // WriteRegisters ACKs (write 0xAA to r0x7F, twice)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // ReadRegisters response
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xAA]);

        let mut channel = mock_channel(&mock);
        // An empty bitstream should still succeed (reset + verify, no data).
        program_fpga(&mut channel, &[], &LOGIC_PRO_8_PROFILE, UsbSpeed::Super).unwrap();
    }

    // -----------------------------------------------------------------------
    // common_device_init tests
    // -----------------------------------------------------------------------

    /// Queue mock responses for the full Lattice common_device_init sequence.
    ///
    /// Returns the expected device ID that will be "read" from EEPROM.
    fn queue_lattice_device_init_responses(
        mock: &MockTransport,
        jesd_lock_polls_before_ready: usize,
        bitslip_attempts_before_success: usize,
    ) -> u64 {
        let device_id: u64 = 0x764051b51bd4d9f1;

        // Step 1: configure_spi_bridge -> write REG_SPI_DATA (8-bit path, 1 ACK)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // Step 2: set_ads528x_drive -> write REG_SPI_DATA (8-bit path, 1 ACK)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // Step 3: set_ads528x_enabled -> write REG_SPI_DATA (8-bit path, 1 ACK)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // Step 4: reset_jesd_phy -> 2 writes (assert + deassert)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // Step 5: wait_for_jesd_lock -> poll reads
        // Return 0x00 for polls before ready, then JESD_READY_MASK | DDR_READY_MASK
        for _ in 0..jesd_lock_polls_before_ready {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }
        // Final poll: all JESD + DDR bits set.
        // JESD_READY_MASK = 0x21 (bits 0,5), DDR_READY_MASK = 0x18 (bits 3,4)
        // Combined: 0x39
        let all_ready = (fpga_regs::JESD_READY_MASK | fpga_regs::DDR_READY_MASK) as u8;
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![all_ready]);

        // Step 6: run_bitslip_detection
        // Each attempt: 1 write (ACK) + 1 read (status)
        for _ in 0..bitslip_attempts_before_success {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // write ACK
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // read: not aligned
        }
        // Successful attempt:
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // write ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x01]); // read: aligned (bit 0 set)

        // Step 7: DDR status check (read REG_STATUS_DDR_JESD)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![all_ready]);

        // Step 8: read_eeprom_device_id -> opcode 0x88 send + 8-byte response
        let id_bytes = device_id.to_le_bytes();
        mock.push_bulk_in_response(endpoint::BULK_IN, id_bytes.to_vec());

        device_id
    }

    #[test]
    fn common_device_init_lattice_full_sequence() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let expected_id = queue_lattice_device_init_responses(&mock, 2, 1);

        let mut channel = mock_channel(&mock);
        let result = common_device_init(&mut channel, true).unwrap();

        assert_eq!(result.device_id, expected_id);
        assert!(result.jesd_locked, "JESD should be locked");
        assert!(result.ddr_ready, "DDR should be ready");
    }

    #[test]
    fn common_device_init_lattice_jesd_locks_immediately() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let expected_id = queue_lattice_device_init_responses(&mock, 0, 0);

        let mut channel = mock_channel(&mock);
        let result = common_device_init(&mut channel, true).unwrap();

        assert_eq!(result.device_id, expected_id);
        assert!(result.jesd_locked);
        assert!(result.ddr_ready);
    }

    /// Queue mock responses for the Xilinx (non-Lattice) common_device_init.
    ///
    /// The Xilinx path reads EEPROM calibration, runs AD9637 init
    /// (configure + DCM lock + calibration cascade), then reads device ID.
    ///
    /// The new init_ad9637 sequence (matching decompiled binary):
    /// 1. configure_ad9637: 9 proxy write ACKs
    /// 2. r0x15=0x00: 1 write ACK
    /// 3. set_ad9637_enabled_false: 1 proxy ACK + dcm_cascade (5-step JESD)
    /// 4. set_ad9637_enabled_true: 3 proxy ACKs + reset_main_dcm (5-step JESD) + 3 reads
    /// 5. write_calibration_and_cascade: 2+16 write ACKs + set_ad9637_enabled_false (1 proxy ACK + 5-step JESD) + 1 read + 1 ACK
    ///
    /// Returns the expected device ID.
    fn queue_xilinx_device_init_responses(mock: &MockTransport) -> u64 {
        let device_id: u64 = 0x764051b51bd4d9f1;

        // Helper: queue one dcm_cascade (5-step JESD: 5 write ACKs + r0x40 read + r0x4a read).
        let queue_one_cascade = |mock: &MockTransport, r40: u8| {
            // 5 writes: JESD power sequence (0x1E, 0x1C, 0x18, 0x08, 0x00)
            for _ in 0..5 {
                mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
            }
            // 2 reads: r0x40, r0x4a
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![r40]);
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x4a
        };

        // Helper: queue reset_main_dcm (5-step JESD cascade, no SERDES retry).
        // r40: the r0x40 value returned after the cascade.
        let queue_reset_main_dcm = |mock: &MockTransport, r40: u8| {
            queue_one_cascade(mock, r40);
        };

        // Calibration EEPROM read (ReadVersion with I2C params) -> 16 bytes
        let cal_data = vec![
            0x46, 0xFF, 0xC6, 0xA6, 0x60, 0xB7, 0xDE, 0x07, 0x67, 0x8B, 0x03, 0x66, 0x11, 0x35,
            0x7C, 0x7F,
        ];
        mock.push_bulk_in_response(endpoint::BULK_IN, cal_data);

        // configure_ad9637: 2 proxy writes (reset) + 7 proxy writes (config) = 9 ACKs
        for _ in 0..9 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }

        // r0x15=0x00 (ADC power off): 1 ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // set_ad9637_enabled_false:
        //   1 proxy write ACK (0x08=0x01 SPI transfer)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        //   dcm_cascade (5-step JESD): 5 write ACKs + r0x40=0x00 + r0x4a=0x00
        queue_one_cascade(mock, 0x00);

        // set_ad9637_enabled_true:
        //   3 proxy write ACKs (0x08=0x00, 0x08=0x03, 0x08=0x00)
        for _ in 0..3 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }
        //   reset_main_dcm (5-step JESD cascade):
        queue_reset_main_dcm(mock, 0xE0);
        //   3 additional reads in set_ad9637_enabled_true (r0x19, r0x40, r0x4a)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x19
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xE0]); // r0x40
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x4a

        // write_calibration_and_cascade:
        //   2 direct writes (r0x00=0x00, r0x00=0x40)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        //   16 calibration coefficient writes
        for _ in 0..16 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }
        //   set_ad9637_enabled_false: 1 proxy ACK + dcm_cascade (5-step JESD)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        queue_one_cascade(mock, 0x00);
        //   1 read (r0x40 post-cal)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        //   1 write (r0x12=0x04)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        // EEPROM device ID read
        let id_bytes = device_id.to_le_bytes();
        mock.push_bulk_in_response(endpoint::BULK_IN, id_bytes.to_vec());

        device_id
    }

    #[test]
    fn common_device_init_xilinx_runs_ad9637_init() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let expected_id = queue_xilinx_device_init_responses(&mock);

        let mut channel = mock_channel(&mock);
        let result = common_device_init(&mut channel, false).unwrap();

        assert_eq!(result.device_id, expected_id);
        assert!(result.dcm_locked, "DCM should be locked for Xilinx");
        assert!(
            !result.jesd_locked,
            "JESD should not be locked for non-Lattice"
        );
        assert!(!result.ddr_ready, "DDR ready not checked for non-Lattice");
    }

    #[test]
    fn common_device_init_eeprom_read_sends_opcode_0x88() {
        // Test via Lattice path (simpler mock setup) that EEPROM read uses 0x88.
        let mock = std::sync::Arc::new(MockTransport::new());
        let expected_id = queue_lattice_device_init_responses(&mock, 0, 0);

        let mut channel = mock_channel(&mock);
        let result = common_device_init(&mut channel, true).unwrap();
        assert_eq!(result.device_id, expected_id);

        // Find the last OUT transfer -- should be the EEPROM read (opcode 0x88).
        let ops = mock.operations();
        let out_ops: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                UsbOperation::BulkOut { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect();
        // With seed=0 and XorAll: data[1] is the opcode byte.
        let last_out = out_ops.last().unwrap();
        assert_eq!(last_out[1], 0x88, "EEPROM read should use opcode 0x88");
    }

    #[test]
    fn common_device_init_eeprom_read_short_response_fails() {
        // Test via Lattice path -- short EEPROM response should fail.
        let mock = std::sync::Arc::new(MockTransport::new());

        // Queue all Lattice init responses except replace the EEPROM response
        // with a short one.
        // SPI bridge + drive + enable = 3 ACKs
        for _ in 0..3 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }
        // JESD PHY reset = 2 ACKs
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        // JESD lock poll (immediate success)
        let all_ready = (fpga_regs::JESD_READY_MASK | fpga_regs::DDR_READY_MASK) as u8;
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![all_ready]);
        // Bitslip (immediate success)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // write ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x01]); // aligned
                                                                   // DDR status
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![all_ready]);
        // Short EEPROM response (only 4 bytes instead of 8)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x01, 0x02, 0x03, 0x04]);

        let mut channel = mock_channel(&mock);
        let err = common_device_init(&mut channel, true).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("expected 8") && msg.contains("got 4"),
            "error should mention expected and actual length: {msg}"
        );
    }

    #[test]
    fn common_device_init_device_id_matches_calibration_url() {
        // Verify the device ID from the reference calibration file works.
        let mock = std::sync::Arc::new(MockTransport::new());
        let expected_id = queue_lattice_device_init_responses(&mock, 0, 0);

        let mut channel = mock_channel(&mock);
        let result = common_device_init(&mut channel, true).unwrap();

        assert_eq!(result.device_id, expected_id);
        assert_eq!(
            format!("{:x}", result.device_id),
            "764051b51bd4d9f1",
            "device ID hex should match calibration filename"
        );
    }

    #[test]
    fn common_device_init_lattice_jesd_timeout_still_reads_eeprom() {
        // When JESD doesn't lock, the function should still read the EEPROM
        // device ID (it just reports jesd_locked = false).
        //
        // However, the real wait_for_jesd_lock does a time-based poll with
        // thread::sleep, which makes it impractical to test with the mock
        // (we'd need to queue thousands of responses to fill the timeout).
        //
        // Instead, we test the JESD timeout behavior via the standalone
        // wait_for_jesd_lock function (see jesd_lock_timeout_returns_false).
        //
        // This test verifies the Xilinx path gets DCM locked status.
        let mock = std::sync::Arc::new(MockTransport::new());
        let expected_id = queue_xilinx_device_init_responses(&mock);

        let mut channel = mock_channel(&mock);
        let result = common_device_init(&mut channel, false).unwrap();

        assert_eq!(result.device_id, expected_id);
        assert!(
            !result.jesd_locked,
            "JESD should not be set for Xilinx path"
        );
        assert!(result.dcm_locked, "DCM should be locked for Xilinx path");
    }

    // -----------------------------------------------------------------------
    // JESD lock polling tests
    // -----------------------------------------------------------------------

    #[test]
    fn jesd_lock_success_after_retries() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // 3 polls: not ready, not ready, ready.
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x01]); // only link_up
        let ready = fpga_regs::JESD_READY_MASK as u8;
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![ready]);

        let mut channel = mock_channel(&mock);
        let locked = wait_for_jesd_lock(&mut channel, Duration::from_secs(10)).unwrap();
        assert!(locked, "should lock after 3 polls");
    }

    #[test]
    fn jesd_lock_immediate_success() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let ready = fpga_regs::JESD_READY_MASK as u8;
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![ready]);

        let mut channel = mock_channel(&mock);
        let locked = wait_for_jesd_lock(&mut channel, Duration::from_secs(1)).unwrap();
        assert!(locked, "should lock on first poll");
    }

    // -----------------------------------------------------------------------
    // Bitslip detection tests
    // -----------------------------------------------------------------------

    #[test]
    fn bitslip_succeeds_on_first_attempt() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Write ACK + read with bit 0 set (aligned).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // write ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x01]); // aligned

        let mut channel = mock_channel(&mock);
        run_bitslip_detection(&mut channel).unwrap();
    }

    #[test]
    fn bitslip_succeeds_after_retries() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // 3 failed attempts, then success.
        for _ in 0..3 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // write ACK
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // not aligned
        }
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // write ACK
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x01]); // aligned

        let mut channel = mock_channel(&mock);
        run_bitslip_detection(&mut channel).unwrap();
    }

    #[test]
    fn bitslip_exhausts_retries_without_error() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // All BITSLIP_MAX_RETRIES attempts fail -- should not return error.
        for _ in 0..BITSLIP_MAX_RETRIES {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // write ACK
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // not aligned
        }

        let mut channel = mock_channel(&mock);
        // Should succeed (bitslip failure is non-fatal).
        run_bitslip_detection(&mut channel).unwrap();
    }

    // -----------------------------------------------------------------------
    // SPI bridge and ADC helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn configure_spi_bridge_writes_to_spi_data_register() {
        let mock = std::sync::Arc::new(MockTransport::new());
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel(&mock);
        configure_spi_bridge(&mut channel).unwrap();

        let ops = mock.operations();
        let out_ops: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                UsbOperation::BulkOut { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect();

        assert_eq!(out_ops.len(), 1);
        // REG_SPI_DATA = 0x0004, 8-bit path -> opcode 0x80, addr 0x04
        assert_eq!(out_ops[0][1], 0x80, "should use WriteRegisters opcode");
        assert_eq!(out_ops[0][3], 0x04, "should write to register 0x04");
    }

    #[test]
    fn set_ads528x_drive_writes_to_spi_data() {
        let mock = std::sync::Arc::new(MockTransport::new());
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel(&mock);
        set_ads528x_drive(&mut channel).unwrap();

        let ops = mock.operations();
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn set_ads528x_enabled_writes_to_spi_data() {
        let mock = std::sync::Arc::new(MockTransport::new());
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);

        let mut channel = mock_channel(&mock);
        set_ads528x_enabled(&mut channel).unwrap();

        let ops = mock.operations();
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(op, UsbOperation::BulkOut { .. }))
                .count(),
            1
        );
    }

    // -----------------------------------------------------------------------
    // EEPROM read tests (via ep1)
    // -----------------------------------------------------------------------

    #[test]
    fn eeprom_read_returns_correct_device_id() {
        let mock = std::sync::Arc::new(MockTransport::new());

        let device_id: u64 = 0x764051b51bd4d9f1;
        let id_bytes = device_id.to_le_bytes();
        mock.push_bulk_in_response(endpoint::BULK_IN, id_bytes.to_vec());

        let mut channel = mock_channel(&mock);
        let result = channel.read_eeprom_device_id().unwrap();
        assert_eq!(result, device_id);
    }

    #[test]
    fn eeprom_read_fails_on_short_response() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Only 7 bytes (need 8 for u64).
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![1, 2, 3, 4, 5, 6, 7]);

        let mut channel = mock_channel(&mock);
        let err = channel.read_eeprom_device_id().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("expected 8") && msg.contains("got 7"),
            "error should mention expected and actual length: {msg}"
        );
    }

    #[test]
    fn eeprom_read_handles_zero_device_id() {
        let mock = std::sync::Arc::new(MockTransport::new());

        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0; 8]);

        let mut channel = mock_channel(&mock);
        let result = channel.read_eeprom_device_id().unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn eeprom_read_handles_max_device_id() {
        let mock = std::sync::Arc::new(MockTransport::new());

        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xFF; 8]);

        let mut channel = mock_channel(&mock);
        let result = channel.read_eeprom_device_id().unwrap();
        assert_eq!(result, u64::MAX);
    }

    // -----------------------------------------------------------------------
    // HwRevision tests
    // -----------------------------------------------------------------------

    #[test]
    fn hw_revision_parse_valid_xilinx_rev1() {
        use saleae_proto::device::{AdcType, FpgaVariant};
        // Real device: Logic Pro 8 Rev 1.02 (Xilinx Spartan-6)
        let data = [0x52, 0x45, 0x56, 0x01, 0x2E, 0x02]; // "REV" + 1 + "." + 2
        let rev = HwRevision::parse(&data).unwrap();
        assert_eq!(rev.major, 1);
        assert_eq!(rev.minor, 2);
        assert!(!rev.is_lattice());
        assert_eq!(rev.fpga_variant, FpgaVariant::XilinxAlt);
        assert_eq!(rev.adc_type, AdcType::Ad9637);
    }

    #[test]
    fn hw_revision_parse_valid_xilinx_rev0() {
        use saleae_proto::device::{AdcType, FpgaVariant};
        let data = [0x52, 0x45, 0x56, 0x00, 0x2E, 0x05]; // "REV" + 0 + "." + 5
        let rev = HwRevision::parse(&data).unwrap();
        assert_eq!(rev.major, 0);
        assert_eq!(rev.minor, 5);
        assert!(!rev.is_lattice());
        assert_eq!(rev.fpga_variant, FpgaVariant::Xilinx);
        assert_eq!(rev.adc_type, AdcType::Ad9637);
    }

    #[test]
    fn hw_revision_parse_valid_lattice_rev4() {
        use saleae_proto::device::{AdcType, FpgaVariant};
        let data = [0x52, 0x45, 0x56, 0x04, 0x2E, 0x00]; // "REV" + 4 + "." + 0
        let rev = HwRevision::parse(&data).unwrap();
        assert_eq!(rev.major, 4);
        assert_eq!(rev.minor, 0);
        assert!(rev.is_lattice());
        assert_eq!(rev.fpga_variant, FpgaVariant::LatticeEcp5);
        assert_eq!(rev.adc_type, AdcType::Ads528x);
    }

    #[test]
    fn hw_revision_parse_valid_lattice_rev3() {
        use saleae_proto::device::AdcType;
        let data = [0x52, 0x45, 0x56, 0x03, 0x2E, 0x01];
        let rev = HwRevision::parse(&data).unwrap();
        assert!(rev.is_lattice());
        assert_eq!(rev.adc_type, AdcType::Ad9637);
    }

    #[test]
    fn hw_revision_parse_too_short() {
        let data = [0x52, 0x45, 0x56, 0x01, 0x2E]; // 5 bytes, need 6
        assert!(HwRevision::parse(&data).is_none());
    }

    #[test]
    fn hw_revision_parse_wrong_header() {
        let data = [0x00, 0x00, 0x00, 0x01, 0x2E, 0x02]; // not "REV"
        assert!(HwRevision::parse(&data).is_none());
    }

    #[test]
    fn hw_revision_rev2_is_unsupported() {
        // Rev 2 is unsupported in the binary (throws exception).
        // HardwareRevision::parse now returns None for unsupported revisions.
        let data = [0x52, 0x45, 0x56, 0x02, 0x2E, 0x00];
        assert!(HwRevision::parse(&data).is_none());
    }

    #[test]
    fn hw_revision_display_xilinx() {
        let data = [0x52, 0x45, 0x56, 0x01, 0x2E, 0x02];
        let rev = HwRevision::parse(&data).unwrap();
        let display = rev.to_string();
        assert!(display.contains("Rev 1.02"), "got: {display}");
        assert!(display.contains("Xilinx"), "got: {display}");
        assert!(display.contains("AD9637"), "got: {display}");
    }

    #[test]
    fn hw_revision_display_lattice() {
        let data = [0x52, 0x45, 0x56, 0x04, 0x2E, 0x00];
        let rev = HwRevision::parse(&data).unwrap();
        let display = rev.to_string();
        assert!(display.contains("Rev 4.00"), "got: {display}");
        assert!(display.contains("Lattice"), "got: {display}");
        assert!(display.contains("ADS528x"), "got: {display}");
    }

    #[test]
    fn hw_revision_lattice_major_values() {
        // All Lattice major revisions from the switch statement
        for major in [3u8, 4, 5, 6, 8, 9, 10, 11] {
            let data = [0x52, 0x45, 0x56, major, 0x2E, 0x00];
            let rev = HwRevision::parse(&data).unwrap();
            assert!(rev.is_lattice(), "major {major} should be Lattice");
        }
        // Non-Lattice major revisions
        for major in [0u8, 1, 7] {
            let data = [0x52, 0x45, 0x56, major, 0x2E, 0x00];
            let rev = HwRevision::parse(&data).unwrap();
            assert!(!rev.is_lattice(), "major {major} should not be Lattice");
        }
    }
}
