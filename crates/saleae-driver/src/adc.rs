//! AD9637 ADC initialization for Xilinx Spartan-6 devices.
//!
//! The Saleae Logic Pro 8 Rev 1.2 uses a Xilinx Spartan-6 FPGA with an
//! AD9637 ADC. This module implements the ADC initialization sequence
//! reverse-engineered from `SetAd9637Enabled` (ELF VA 0x1c23330),
//! `ResetMainDcm` (ELF VA 0x1c22af0), and the ADC register configuration
//! function (ELF VA 0x1c20d90) in libgraph_server_shared.so.
//!
//! # Sequence overview (from CommonDeviceInit for AdcType==1)
//!
//! 1. **ADC register configuration**: Reset ADC, configure output mode,
//!    clock, VREF, DCO settings via the FPGA proxy (FUN_01d20d90).
//! 2. **ADC output power off**: FPGA r0x15=0x00.
//! 3. **SetAd9637Enabled(false)**: SPI transfer (reg 0x08=0x01) +
//!    DCM cascade (5-step JESD: 0x1E->0x1C->0x18->0x08->0x00, read r0x40).
//! 4. **SetAd9637Enabled(true)**: Power normal, sleep 1ms, reset+standby,
//!    normal, sleep 50ms, ResetMainDcm, check DCM lock + bitslip.
//! 5. **Calibration**: Enable clock output, write coefficients, cascade.
//!
//! # ADC register access
//!
//! The AD9637 is accessed through the FPGA's 4-register proxy protocol,
//! implemented in [`jesd::write_adc_register`]. This module calls that
//! function for all AD9637 register writes.
//!
//! # ResetMainDcm / DCM cascade
//!
//! Both `dcm_cascade` and `reset_main_dcm` use the 5-step JESD power
//! sequence (0x1E->0x1C->0x18->0x08->0x00) via [`jesd::jesd_power_sequence`].
//! The pcap from nvdk-msdlab-09 confirms this is the only sequence that
//! locks the DCM on hardware. The binary's 2-step path (r0x00=0x04,
//! sleep, r0x00=0x00) never locks on Spartan-6 and is not used.
//!
//! # Hardware testing
//!
//! This sequence has been hardware-validated on a Logic Pro 8 Rev 1.2
//! (Xilinx Spartan-6 XC6SLX9, AD9637 ADC) via the `full-init` and
//! `capture-digital` CLI commands.

use std::time::Duration;

use saleae_proto::fpga_regs;
use tracing::{debug, info, warn};

use crate::ep1::Ep1CommandChannel;
use crate::error::{DriverError, DriverResult};
use crate::jesd;

// ---------------------------------------------------------------------------
// Configuration constants
// ---------------------------------------------------------------------------

/// Delay after ADC digital reset before configuring registers.
/// From decompiled FUN_01d20d90: DAT_00989680 = 10,000,000 ns = 10ms.
const ADC_RESET_SETTLE_MS: u64 = 10;

/// Delay between power-normal and reset+standby in SetAd9637Enabled(true).
/// From decompiled SetAd9637Enabled: first sleep_for = 1ms (0x000f4240 ns).
const ADC_ENABLE_PRE_RESET_MS: u64 = 1;

/// Delay after ADC reset cycle for DCM to lock.
/// From decompiled SetAd9637Enabled: second sleep_for = 50ms (0x02faf080 ns).
/// USB 2.0 cold boot may need extra settle time; 100ms gives margin.
const DCM_LOCK_SETTLE_MS: u64 = 100;

/// Max attempts for DCM lock. The Spartan-6 DCM lock is intermittent,
/// especially on cold boot over USB 2.0.
const DCM_LOCK_MAX_ATTEMPTS: u32 = 10;

/// DCM lock mask: bits 5 (main DCM) + 6 (slow DCM) + 7 (system PLL).
///
/// Hardware-validated: r0x40 == 0xE0 when all three are locked.
/// SetAd9637Enabled checks main (bit 5) and slow (bit 6) separately.
pub const DCM_ALL_LOCKED: u8 = 0xE0;

/// Main DCM locked (bit 5). Same as [`fpga_regs::PLL_MAIN_DCM_LOCKED`].
pub const DCM_MAIN_BIT: u8 = fpga_regs::PLL_MAIN_DCM_LOCKED;

/// Slow DCM locked (bit 6). Same as [`fpga_regs::PLL_SLOW_DCM_LOCKED`].
pub const DCM_SLOW_BIT: u8 = fpga_regs::PLL_SLOW_DCM_LOCKED;

/// DCM lock mask for main DCM + system PLL (initial lock check in ResetMainDcm).
///
/// From decompiled ResetMainDcm: `(bVar2 & 0xa0) == 0xa0`.
/// Bits 5 (main DCM) + 7 (system PLL).
pub const DCM_MAIN_LOCKED: u8 = 0xA0;

/// Bitslip ADC0 detected (r0x4a bit 1).
const BITSLIP_ADC0: u8 = 0x02;

/// Bitslip ADC1 detected (r0x4a bit 2, Logic Pro 16 only).
#[allow(dead_code)]
const BITSLIP_ADC1: u8 = 0x04;

// ---------------------------------------------------------------------------
// AD9637 register addresses (SPI register map)
// ---------------------------------------------------------------------------
// From AD9637 datasheet and pcap-validated register writes.

/// AD9637 SPI_PORT_CONFIG / power modes register.
const AD9637_POWER_MODES: u8 = 0x08;

/// AD9637 output test mode register.
const AD9637_TEST_MODE: u8 = 0x0D;

/// AD9637 output adjust register.
const AD9637_OUTPUT_ADJUST: u8 = 0x15;

/// AD9637 DCO output configuration.
const AD9637_DCO_CONFIG: u8 = 0x21;

/// AD9637 output mode 2 (LVDS/CMOS select, etc.).
const AD9637_OUTPUT_MODE_2: u8 = 0x05;

/// AD9637 output mode 1.
const AD9637_OUTPUT_MODE_1: u8 = 0x04;

/// AD9637 clock configuration.
const AD9637_CLOCK_CONFIG: u8 = 0x10;

/// AD9637 VREF configuration.
const AD9637_VREF_CONFIG: u8 = 0x18;

// AD9637 power mode values
const AD9637_RESET_AND_STANDBY: u8 = 0x03;
const AD9637_POWER_NORMAL: u8 = 0x00;
const AD9637_SPI_TRANSFER: u8 = 0x01;

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

/// Result of the AD9637 DCM lock sequence.
#[derive(Debug, Clone)]
pub struct DcmLockResult {
    /// Whether all DCMs locked successfully.
    pub locked: bool,
    /// Value of status register r0x40 after lock attempt.
    pub status_40: u8,
    /// Value of bitslip register r0x4a after lock attempt.
    pub status_4a: u8,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Configure AD9637 ADC registers (Phase 3b).
///
/// Resets the ADC and writes the full register configuration. This matches
/// the decompiled FUN_01d20d90 which is called from CommonDeviceInit for
/// AdcType==1 with param_2=0x200000001 and param_3=0.
///
/// After this function returns, the ADC is configured but its output is
/// powered off. The caller should then run SetAd9637Enabled(false) followed
/// by SetAd9637Enabled(true) to bring up the DCM.
///
/// # Decompiled sequence (FUN_01d20d90)
///
/// 1. WriteAd9637Register(0x08, 0x03) -- reset+standby
/// 2. WriteAd9637Register(0x08, 0x00) -- normal
/// 3. sleep 10ms
/// 4. WriteAd9637Register(0x0D, 0x00) -- test mode off
/// 5. WriteAd9637Register(0x15, 0x21) -- output adjust (computed from params)
/// 6. WriteAd9637Register(0x21, 0xC2) -- DCO config
/// 7. WriteAd9637Register(0x05, 0x0F) -- output mode 2
/// 8. WriteAd9637Register(0x04, 0x0F) -- output mode 1
/// 9. WriteAd9637Register(0x10, 0x00) -- clock config
/// 10. WriteAd9637Register(0x18, 0x04) -- VREF config
///
/// Note: the binary does NOT include r0x15=0x00 or SPI commit here.
/// Those happen in CommonDeviceInit after this function returns.
pub fn configure_ad9637(ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
    info!("Configuring AD9637 ADC registers");

    // Digital reset cycle
    debug!("ADC digital reset (reg 0x08: 0x03 -> 0x00)");
    jesd::write_adc_register(ep1, AD9637_POWER_MODES, AD9637_RESET_AND_STANDBY)?;
    jesd::write_adc_register(ep1, AD9637_POWER_MODES, AD9637_POWER_NORMAL)?;

    debug!("Sleeping {}ms for reset settle", ADC_RESET_SETTLE_MS);
    std::thread::sleep(Duration::from_millis(ADC_RESET_SETTLE_MS));

    // Configure ADC registers (pcap-validated values, matches decompiled FUN_01d20d90)
    debug!("Writing ADC configuration registers");
    jesd::write_adc_register(ep1, AD9637_TEST_MODE, 0x00)?; // test mode off
    jesd::write_adc_register(ep1, AD9637_OUTPUT_ADJUST, 0x21)?; // output adjust
    jesd::write_adc_register(ep1, AD9637_DCO_CONFIG, 0xC2)?; // DCO config
    jesd::write_adc_register(ep1, AD9637_OUTPUT_MODE_2, 0x0F)?; // output mode 2
    jesd::write_adc_register(ep1, AD9637_OUTPUT_MODE_1, 0x0F)?; // output mode 1
    jesd::write_adc_register(ep1, AD9637_CLOCK_CONFIG, 0x00)?; // clock config
    jesd::write_adc_register(ep1, AD9637_VREF_CONFIG, 0x04)?; // VREF config

    info!("AD9637 configuration complete");
    Ok(())
}

/// ResetMainDcm -- 5-step JESD cascade, pcap-validated.
///
/// Runs a single 5-step JESD power sequence (0x1E->0x1C->0x18->0x08->0x00)
/// and reads the DCM status registers. The pcap from nvdk-msdlab-09 shows
/// no SERDES retry writes and no 2-step fallback -- just the 5-step cascade.
///
/// The caller (`set_ad9637_enabled_true`) checks the lock bits and errors
/// if the DCM didn't lock.
pub fn reset_main_dcm(ep1: &mut Ep1CommandChannel) -> DriverResult<DcmLockResult> {
    debug!("ResetMainDcm: 5-step JESD cascade (AdcType==0 path, pcap-validated)");
    let (r40, r4a) = dcm_cascade(ep1)?;
    Ok(DcmLockResult {
        locked: (r40 & DCM_MAIN_LOCKED) == DCM_MAIN_LOCKED,
        status_40: r40,
        status_4a: r4a,
    })
}

/// SetAd9637Enabled(false) -- disable ADC and run DCM cascade.
///
/// From decompiled SetAd9637Enabled with param_2==0:
/// 1. WriteAd9637Register(0x08, 0x01) -- SPI transfer (commit config)
/// 2. ResetMainDcm()
/// 3. Read r0x40 -- log if main/slow DCM locked after power-down
///
/// The pcap confirms r0x40=0x00 after this cascade is NORMAL.
/// The DCM locks later during set_ad9637_enabled_true.
pub fn set_ad9637_enabled_false(ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
    debug!("SetAd9637Enabled(false)");

    // SPI transfer to commit configuration
    jesd::write_adc_register(ep1, AD9637_POWER_MODES, AD9637_SPI_TRANSFER)?;

    // Single DCM cascade -- r0x40=0x00 is expected here (pcap seq 51-52).
    // We use dcm_cascade() directly since we don't need the DcmLockResult
    // wrapper and the disabled phase is not expected to lock.
    let (r40, r4a) = dcm_cascade(ep1)?;

    debug!("SetAd9637Enabled(false) cascade: r0x40={r40:#04x} r0x4a={r4a:#04x}");

    Ok(())
}

/// SetAd9637Enabled(true) -- enable ADC and verify DCM lock.
///
/// From decompiled SetAd9637Enabled with param_2!=0:
/// 1. WriteAd9637Register(0x08, 0x00) -- power normal
/// 2. Sleep 1ms
/// 3. WriteAd9637Register(0x08, 0x03) -- reset + standby
/// 4. WriteAd9637Register(0x08, 0x00) -- power normal (clear reset)
/// 5. Sleep 50ms
/// 6. ResetMainDcm()
/// 7. Read r0x40 -- check main DCM (bit 5) and slow DCM (bit 6)
/// 8. If main DCM not locked -> throw "Main DCM didn't lock"
/// 9. If slow DCM not locked -> throw "Slow DCM didn't lock"
/// 10. Read r0x19, r0x40, r0x4a
/// 11. If r0x4a bit 1 -> throw "ADC 1 bitslip detected"
/// 12. If r0x4a bit 2 AND Pro 16 -> throw "ADC 2 bitslip detected"
pub fn set_ad9637_enabled_true(ep1: &mut Ep1CommandChannel) -> DriverResult<DcmLockResult> {
    debug!("SetAd9637Enabled(true)");

    // Power normal
    jesd::write_adc_register(ep1, AD9637_POWER_MODES, AD9637_POWER_NORMAL)?;

    // Sleep 1ms (from decompiled: first sleep_for with DAT_000f4240)
    std::thread::sleep(Duration::from_millis(ADC_ENABLE_PRE_RESET_MS));

    // Reset cycle: reset+standby -> normal
    jesd::write_adc_register(ep1, AD9637_POWER_MODES, AD9637_RESET_AND_STANDBY)?;
    jesd::write_adc_register(ep1, AD9637_POWER_MODES, AD9637_POWER_NORMAL)?;

    // Sleep 50ms (from decompiled: second sleep_for with DAT_02faf080)
    std::thread::sleep(Duration::from_millis(DCM_LOCK_SETTLE_MS));

    // ResetMainDcm
    let result = reset_main_dcm(ep1)?;
    let r40 = result.status_40;

    // Check main DCM lock (bit 5)
    if (r40 & DCM_MAIN_BIT) == 0 {
        return Err(DriverError::FpgaProgramming {
            context: format!("Main DCM didn't lock (r0x40={r40:#04x})"),
        });
    }

    // Check slow DCM lock (bit 6)
    if (r40 & DCM_SLOW_BIT) == 0 {
        return Err(DriverError::FpgaProgramming {
            context: format!("Slow DCM didn't lock (r0x40={r40:#04x})"),
        });
    }

    // Read additional status registers (matches decompiled)
    let r19 = ep1.read_register_8(fpga_regs::FEAT_STATUS_GENERAL)?;
    let r40_final = ep1.read_register_8(fpga_regs::FEAT_PLL_STATUS)?;
    let mut r4a = ep1.read_register_8(fpga_regs::FEAT_PLL_STATUS_2)?;

    debug!("Post-enable: r0x19={r19:#04x} r0x40={r40_final:#04x} r0x4a={r4a:#04x}");

    // Bitslip check: r0x4a bit 1 = LVDS deserializer has performed bit
    // alignment. On cold init (pcap), this flag appears transiently in the
    // cascade read and clears by the post-enable read. On warm restarts,
    // the flag stays latched because the FPGA hasn't been reprogrammed.
    //
    // The Saleae binary treats this as a hard error, but it only ever does
    // cold inits. For warm restarts, a latched bitslip flag with locked DCM
    // (r0x40=0xE0) indicates successful alignment, not an error.
    if (r4a & BITSLIP_ADC0) != 0 {
        // Try one brief settle to see if it clears (as in the pcap).
        std::thread::sleep(Duration::from_millis(1));
        r4a = ep1.read_register_8(fpga_regs::FEAT_PLL_STATUS_2)?;
        if (r4a & BITSLIP_ADC0) != 0 {
            if r40_final == DCM_ALL_LOCKED {
                // DCM locked + bitslip latched = warm restart, alignment already done.
                warn!("ADC bitslip flag latched (r0x4a={r4a:#04x}), proceeding (DCM locked)");
            } else {
                // Bitslip without stable clock = real alignment error.
                return Err(DriverError::FpgaProgramming {
                    context: format!(
                        "ADC bitslip with DCM not fully locked \
                         (r0x4a={r4a:#04x}, r0x40={r40_final:#04x})"
                    ),
                });
            }
        }
    }
    // ADC 2 bitslip only for Logic Pro 16 -- not checked for Pro 8.

    info!("ADC enabled: r0x40={r40_final:#04x} r0x4a={r4a:#04x}");

    Ok(DcmLockResult {
        locked: true,
        status_40: r40_final,
        status_4a: r4a,
    })
}

/// Run a single DCM cascade via the 5-step JESD power sequence.
///
/// Executes [`jesd::jesd_power_sequence`] (writes r0x00 = 0x1E, 0x1C, 0x18,
/// 0x08, 0x00) then reads r0x40 and r0x4a. This matches the pcap from
/// nvdk-msdlab-09 exactly. DCM lock is NOT expected after the first cascade
/// (during `set_ad9637_enabled_false`); it locks on the second cascade
/// (during `set_ad9637_enabled_true`).
///
/// # Returns
///
/// Tuple of (r0x40, r0x4a) register values after the cascade.
pub fn dcm_cascade(ep1: &mut Ep1CommandChannel) -> DriverResult<(u8, u8)> {
    debug!("Running DCM cascade (5-step JESD: 0x1E->0x1C->0x18->0x08->0x00)");
    jesd::jesd_power_sequence(ep1)?;
    let status_40 = ep1.read_register_8(fpga_regs::FEAT_PLL_STATUS)?;
    let status_4a = ep1.read_register_8(fpga_regs::FEAT_PLL_STATUS_2)?;
    debug!("DCM cascade result: r0x40={status_40:#04x} r0x4a={status_4a:#04x}");
    Ok((status_40, status_4a))
}

/// Write calibration coefficients and run post-calibration DCM cascade (Phase 3d-3e).
///
/// After initial DCM lock, this function:
/// 1. Enables ADC clock output (r0x00=0x00, r0x00=0x40)
/// 2. Writes calibration coefficients to r0x17
/// 3. Commits ADC config (reg 0x08=0x01)
/// 4. Runs DCM cascade again (post-calibration uses ResetMainDcm)
/// 5. Writes r0x12=0x04 (capture config)
///
/// The DCM may not lock after this step -- that's expected. It will be
/// re-locked at capture start via SetAd9637Enabled(true).
///
/// # Arguments
///
/// - `ep1`: EP1 command channel.
/// - `calibration`: Calibration coefficients (typically 16 bytes from EEPROM).
///
/// # Returns
///
/// DCM status register r0x40 after post-calibration cascade.
pub fn write_calibration_and_cascade(
    ep1: &mut Ep1CommandChannel,
    calibration: &[u8],
) -> DriverResult<u8> {
    info!("Post-DCM calibration sequence");

    // Enable ADC clock output (from CommonDeviceInit lines 567-569)
    debug!("Enabling ADC clock output (r0x00=0x00, r0x00=0x40)");
    ep1.write_register_8(fpga_regs::FEAT_CAPTURE_INIT, 0x00)?;
    ep1.write_register_8(fpga_regs::FEAT_CAPTURE_INIT, 0x40)?;

    // Write calibration coefficients
    debug!("Writing {} calibration coefficients", calibration.len());
    jesd::write_calibration_coefficients(ep1, calibration)?;

    // Commit ADC config + run SetAd9637Enabled(false) which does
    // SPI transfer + ResetMainDcm
    debug!("Post-calibration SetAd9637Enabled(false)");
    set_ad9637_enabled_false(ep1)?;

    let r40 = ep1.read_register_8(fpga_regs::FEAT_PLL_STATUS)?;

    // Capture config register
    ep1.write_register_8(fpga_regs::FEAT_CAPTURE_CONFIG_2, 0x04)?;

    info!("Post-calibration cascade complete (r0x40={r40:#04x})");
    Ok(r40)
}

/// Power on ADC and lock DCMs for capture (Phase 7 pre-capture).
///
/// This is called before starting a capture. It:
/// 1. Powers on the ADC bank (FPGA r0x15=0x03)
/// 2. Runs SetAd9637Enabled(true) which does the full enable sequence
///
/// # Errors
///
/// Returns [`DriverError::FpgaProgramming`] if DCM fails to lock.
pub fn power_on_for_capture(ep1: &mut Ep1CommandChannel) -> DriverResult<DcmLockResult> {
    info!("Powering on ADC for capture");

    debug!("FPGA r0x15=0x03 (ADC bank power on)");
    ep1.write_register_8(fpga_regs::FEAT_BANK_POWER, 0x03)?;

    // Reset the LVDS deserializer state before enabling. On warm restarts
    // (fast path), the ADC may be in an unknown state from a previous
    // session. The enabled_false call does SPI transfer + JESD cascade
    // which resets the FPGA's alignment logic.
    if let Err(e) = set_ad9637_enabled_false(ep1) {
        debug!("Pre-enable reset failed (expected on some warm paths): {e}");
    }

    // Enable ADC with retry (DCM lock is intermittent on Spartan-6).
    let mut last_err = None;
    for attempt in 0..DCM_LOCK_MAX_ATTEMPTS {
        match set_ad9637_enabled_true(ep1) {
            Ok(result) => {
                if attempt > 0 {
                    info!(attempt, "DCM locked on retry (power_on_for_capture)");
                }
                info!(
                    "ADC powered on, DCM locked (r0x40={:#04x})",
                    result.status_40
                );
                return Ok(result);
            }
            Err(e) => {
                warn!(
                    attempt,
                    remaining = DCM_LOCK_MAX_ATTEMPTS - attempt - 1,
                    "DCM lock failed during power_on, retrying: {e}"
                );
                last_err = Some(e);
                if let Err(e) = set_ad9637_enabled_false(ep1) {
                    debug!("Retry disable failed (continuing): {e}");
                }
                // Power-cycle the ADC bank before retrying (100ms off, 100ms on).
                if let Err(e) = ep1.write_register_8(fpga_regs::FEAT_BANK_POWER, 0x00) {
                    debug!("Bank power off failed (continuing): {e}");
                }
                std::thread::sleep(Duration::from_millis(100));
                if let Err(e) = ep1.write_register_8(fpga_regs::FEAT_BANK_POWER, 0x03) {
                    debug!("Bank power on failed (continuing): {e}");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    Err(last_err.unwrap())
}

/// Power down ADC (Phase 8 teardown).
///
/// Powers off the ADC bank and runs SetAd9637Enabled(false) to cleanly
/// shut down the DCM.
pub fn power_down(ep1: &mut Ep1CommandChannel) -> DriverResult<()> {
    info!("Powering down AD9637 ADC");

    ep1.write_register_8(fpga_regs::FEAT_BANK_POWER, 0x00)?;
    set_ad9637_enabled_false(ep1)?;

    info!("ADC powered down");
    Ok(())
}

/// Complete AD9637 init sequence for Xilinx Spartan-6 devices.
///
/// Matches the CommonDeviceInit flow for AdcType==1:
/// 1. Configure ADC registers (FUN_01d20d90)
/// 2. Power off ADC bank (r0x15=0x00)
/// 3. SetAd9637Enabled(false) -- SPI commit + ResetMainDcm
/// 4. SetAd9637Enabled(true) -- enable, reset cycle, DCM lock + bitslip check
/// 5. Write calibration coefficients
///
/// # Arguments
///
/// - `ep1`: EP1 command channel (must be initialized).
/// - `calibration`: Calibration coefficients from EEPROM (16 bytes).
///
/// # Returns
///
/// [`DcmLockResult`] from the enable sequence.
///
/// # Errors
///
/// Returns error if DCM fails to lock or bitslip is detected.
pub fn init_ad9637(ep1: &mut Ep1CommandChannel, calibration: &[u8]) -> DriverResult<DcmLockResult> {
    info!("Starting AD9637 init sequence (Xilinx Spartan-6 path)");

    // Phase 3b: Configure ADC registers
    configure_ad9637(ep1)?;

    // ADC output power off (from CommonDeviceInit line 546)
    debug!("FPGA r0x15=0x00 (ADC output power off)");
    ep1.write_register_8(fpga_regs::FEAT_BANK_POWER, 0x00)?;

    // SetAd9637Enabled(false/true) with retry -- DCM lock is intermittent.
    let mut last_err = None;
    for attempt in 0..DCM_LOCK_MAX_ATTEMPTS {
        if attempt == 0 {
            set_ad9637_enabled_false(ep1)?;
        } else {
            // On retries, swallow disable errors (device may be in a bad state).
            let _ = set_ad9637_enabled_false(ep1);
        }
        match set_ad9637_enabled_true(ep1) {
            Ok(result) => {
                if attempt > 0 {
                    info!(attempt, "DCM locked on retry (init_ad9637)");
                }
                // Phase 3d-3e: Calibration + post-cal cascade
                write_calibration_and_cascade(ep1, calibration)?;
                info!("AD9637 init sequence complete");
                return Ok(result);
            }
            Err(e) => {
                warn!(
                    attempt,
                    remaining = DCM_LOCK_MAX_ATTEMPTS - attempt - 1,
                    "DCM lock failed during init, retrying: {e}"
                );
                last_err = Some(e);
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    Err(last_err.unwrap())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_proto::device::endpoint;
    use saleae_proto::obfuscation::EncryptionMode;
    use saleae_usb::mock::{MockTransport, UsbOperation};

    /// Create a mock EP1 channel in None mode (no encryption, simpler assertions).
    fn mock_channel_none(mock: &std::sync::Arc<MockTransport>) -> Ep1CommandChannel {
        let transport: Box<dyn saleae_usb::transport::UsbTransport> = Box::new(mock.clone());
        Ep1CommandChannel::new(transport, EncryptionMode::None, true)
    }

    /// Extract outbound bulk transfer payloads from mock operations.
    fn extract_out_payloads(mock: &MockTransport) -> Vec<Vec<u8>> {
        mock.operations()
            .iter()
            .filter_map(|op| match op {
                UsbOperation::BulkOut { data, .. } => Some(data.clone()),
                _ => None,
            })
            .collect()
    }

    /// Push `n` ACK responses (0x00) for write commands.
    fn push_write_acks(mock: &MockTransport, n: usize) {
        for _ in 0..n {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        }
    }

    /// Push responses for a full dcm_cascade call:
    /// 5 write ACKs (JESD power sequence) + 2 register reads (r0x40, r0x4a).
    fn push_dcm_cascade_responses(mock: &MockTransport, r40: u8, r4a: u8) {
        push_write_acks(mock, 5); // JESD 5-step writes
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![r40]); // r0x40
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![r4a]); // r0x4a
    }

    /// Push responses for set_ad9637_enabled_false:
    /// 1 write_adc_register ACK + dcm_cascade (5 write ACKs + 2 reads).
    fn push_enabled_false_responses(mock: &MockTransport, r40: u8, r4a: u8) {
        push_write_acks(mock, 1); // write_adc_register (SPI transfer)
        push_dcm_cascade_responses(mock, r40, r4a);
    }

    /// Push responses for set_ad9637_enabled_true (happy path, all DCMs locked):
    /// 3 write_adc_register ACKs + dcm_cascade (5 ACKs + 2 reads) +
    /// 3 read_register_8 reads (r0x19, r0x40, r0x4a).
    fn push_enabled_true_responses(mock: &MockTransport, r40: u8, r4a: u8) {
        push_write_acks(mock, 3); // 3 write_adc_register calls
        push_dcm_cascade_responses(mock, r40, r4a); // reset_main_dcm
                                                    // Post-enable status reads: r0x19, r0x40, r0x4a
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x19
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![r40]); // r0x40
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![r4a]); // r0x4a
    }

    /// Assert that an outbound payload is a write_adc_register batch
    /// with the expected bank and value.
    ///
    /// Wire format: [0x80, 0x04, r0x03, bank, r0x16, 0x00, r0x04, value, r0x05, 0x00]
    fn assert_adc_write(payloads: &[Vec<u8>], idx: usize, bank: u8, value: u8) {
        let p = &payloads[idx];
        assert_eq!(p[0], 0x80, "payload[{idx}] opcode should be WriteRegisters");
        assert_eq!(p[1], 0x04, "payload[{idx}] count should be 4");
        assert_eq!(
            p[3], bank,
            "payload[{idx}] ADC bank should be {bank:#04x}, got {:#04x}",
            p[3]
        );
        assert_eq!(
            p[7], value,
            "payload[{idx}] ADC value should be {value:#04x}, got {:#04x}",
            p[7]
        );
    }

    /// Assert that an outbound payload is a single write_register_8 call.
    ///
    /// Wire format: [0x80, 0x01, addr, value]
    fn assert_single_write(payloads: &[Vec<u8>], idx: usize, addr: u8, value: u8) {
        let p = &payloads[idx];
        assert_eq!(p[0], 0x80, "payload[{idx}] opcode should be WriteRegisters");
        assert_eq!(p[1], 0x01, "payload[{idx}] count should be 1");
        assert_eq!(
            p[2], addr,
            "payload[{idx}] addr should be {addr:#04x}, got {:#04x}",
            p[2]
        );
        assert_eq!(
            p[3], value,
            "payload[{idx}] value should be {value:#04x}, got {:#04x}",
            p[3]
        );
    }

    /// Assert that an outbound payload is a single read_register_8 call.
    ///
    /// Wire format: [0x81, 0x01, addr]
    fn assert_single_read(payloads: &[Vec<u8>], idx: usize, addr: u8) {
        let p = &payloads[idx];
        assert_eq!(p[0], 0x81, "payload[{idx}] opcode should be ReadRegisters");
        assert_eq!(p[1], 0x01, "payload[{idx}] count should be 1");
        assert_eq!(
            p[2], addr,
            "payload[{idx}] addr should be {addr:#04x}, got {:#04x}",
            p[2]
        );
    }

    // -------------------------------------------------------------------
    // configure_ad9637 tests
    // -------------------------------------------------------------------

    #[test]
    fn configure_ad9637_sends_9_adc_register_writes_in_order() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // 9 write_adc_register calls, each = 1 batch write ACK
        push_write_acks(&mock, 9);

        let mut ep1 = mock_channel_none(&mock);
        configure_ad9637(&mut ep1).unwrap();

        let payloads = extract_out_payloads(&mock);
        assert_eq!(
            payloads.len(),
            9,
            "configure_ad9637 must send exactly 9 ADC register writes"
        );

        // Verify exact sequence from decompiled FUN_01d20d90:
        // 1. reset+standby (0x08=0x03)
        assert_adc_write(&payloads, 0, 0x08, 0x03);
        // 2. power normal (0x08=0x00)
        assert_adc_write(&payloads, 1, 0x08, 0x00);
        // 3. test mode off (0x0D=0x00)
        assert_adc_write(&payloads, 2, 0x0D, 0x00);
        // 4. output adjust (0x15=0x21)
        assert_adc_write(&payloads, 3, 0x15, 0x21);
        // 5. DCO config (0x21=0xC2)
        assert_adc_write(&payloads, 4, 0x21, 0xC2);
        // 6. output mode 2 (0x05=0x0F)
        assert_adc_write(&payloads, 5, 0x05, 0x0F);
        // 7. output mode 1 (0x04=0x0F)
        assert_adc_write(&payloads, 6, 0x04, 0x0F);
        // 8. clock config (0x10=0x00)
        assert_adc_write(&payloads, 7, 0x10, 0x00);
        // 9. VREF config (0x18=0x04)
        assert_adc_write(&payloads, 8, 0x18, 0x04);
    }

    #[test]
    fn configure_ad9637_error_propagation_on_third_write() {
        use saleae_usb::error::UsbError;

        let mock = std::sync::Arc::new(MockTransport::new());

        // First 2 writes succeed, third fails
        push_write_acks(&mock, 2);
        mock.push_bulk_in_error(endpoint::BULK_IN, UsbError::Timeout { timeout_ms: 1000 });

        let mut ep1 = mock_channel_none(&mock);
        let result = configure_ad9637(&mut ep1);
        assert!(result.is_err(), "should propagate error from third write");
    }

    // -------------------------------------------------------------------
    // set_ad9637_enabled_false tests
    // -------------------------------------------------------------------

    #[test]
    fn set_ad9637_enabled_false_sends_spi_transfer_then_cascade() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // r0x40=0x00 is normal after disabling (DCM not locked)
        push_enabled_false_responses(&mock, 0x00, 0x00);

        let mut ep1 = mock_channel_none(&mock);
        set_ad9637_enabled_false(&mut ep1).unwrap();

        let payloads = extract_out_payloads(&mock);
        // 1 write_adc_register + 5 JESD writes + 2 reads = 8
        assert_eq!(payloads.len(), 8);

        // First: SPI transfer (ADC reg 0x08 = 0x01)
        assert_adc_write(&payloads, 0, 0x08, 0x01);

        // Then: JESD 5-step power sequence (r0x00 = 0x1E, 0x1C, 0x18, 0x08, 0x00)
        assert_single_write(&payloads, 1, 0x00, 0x1E);
        assert_single_write(&payloads, 2, 0x00, 0x1C);
        assert_single_write(&payloads, 3, 0x00, 0x18);
        assert_single_write(&payloads, 4, 0x00, 0x08);
        assert_single_write(&payloads, 5, 0x00, 0x00);

        // Then: 2 status register reads
        assert_single_read(&payloads, 6, 0x40);
        assert_single_read(&payloads, 7, 0x4A);
    }

    // -------------------------------------------------------------------
    // set_ad9637_enabled_true tests
    // -------------------------------------------------------------------

    #[test]
    fn set_ad9637_enabled_true_happy_path_all_dcms_locked() {
        let mock = std::sync::Arc::new(MockTransport::new());

        push_enabled_true_responses(&mock, 0xE0, 0x00);

        let mut ep1 = mock_channel_none(&mock);
        let result = set_ad9637_enabled_true(&mut ep1).unwrap();

        assert!(result.locked);
        assert_eq!(result.status_40, 0xE0);
        assert_eq!(result.status_4a, 0x00);

        let payloads = extract_out_payloads(&mock);
        // 3 write_adc_register + 5 JESD writes + 2 reads + 3 reads = 13
        assert_eq!(payloads.len(), 13);

        // ADC power-on sequence:
        // 1. Power normal (0x08=0x00)
        assert_adc_write(&payloads, 0, 0x08, 0x00);
        // 2. Reset+standby (0x08=0x03)
        assert_adc_write(&payloads, 1, 0x08, 0x03);
        // 3. Power normal again (0x08=0x00)
        assert_adc_write(&payloads, 2, 0x08, 0x00);

        // JESD cascade (r0x00 = 0x1E, 0x1C, 0x18, 0x08, 0x00)
        assert_single_write(&payloads, 3, 0x00, 0x1E);
        assert_single_write(&payloads, 4, 0x00, 0x1C);
        assert_single_write(&payloads, 5, 0x00, 0x18);
        assert_single_write(&payloads, 6, 0x00, 0x08);
        assert_single_write(&payloads, 7, 0x00, 0x00);

        // DCM status reads from cascade
        assert_single_read(&payloads, 8, 0x40);
        assert_single_read(&payloads, 9, 0x4A);

        // Post-enable status reads (r0x19, r0x40, r0x4a)
        assert_single_read(&payloads, 10, 0x19);
        assert_single_read(&payloads, 11, 0x40);
        assert_single_read(&payloads, 12, 0x4A);
    }

    #[test]
    fn set_ad9637_enabled_true_errors_if_main_dcm_not_locked() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // r0x40=0x40 means slow DCM locked but main DCM NOT locked (bit 5 = 0)
        push_write_acks(&mock, 3);
        push_dcm_cascade_responses(&mock, 0x40, 0x00);

        let mut ep1 = mock_channel_none(&mock);
        let err = set_ad9637_enabled_true(&mut ep1).unwrap_err();
        assert!(
            err.to_string().contains("Main DCM didn't lock"),
            "expected main DCM error, got: {err}"
        );
    }

    #[test]
    fn set_ad9637_enabled_true_errors_if_slow_dcm_not_locked() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // r0x40=0xA0 means main DCM + system PLL locked, but slow DCM NOT locked
        // Main check passes (0xA0 & 0x20 = 0x20), slow check fails (0xA0 & 0x40 = 0)
        push_write_acks(&mock, 3);
        push_dcm_cascade_responses(&mock, 0xA0, 0x00);

        let mut ep1 = mock_channel_none(&mock);
        let err = set_ad9637_enabled_true(&mut ep1).unwrap_err();
        assert!(
            err.to_string().contains("Slow DCM didn't lock"),
            "expected slow DCM error, got: {err}"
        );
    }

    #[test]
    fn set_ad9637_enabled_true_warns_on_bitslip_but_succeeds() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // DCM locks fine but bitslip flag set on r0x4a
        push_write_acks(&mock, 3);
        push_dcm_cascade_responses(&mock, 0xE0, 0x00);
        // Post-enable reads: r0x19=0x00, r0x40=0xE0, r0x4a=0x02 (BITSLIP_ADC0)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x19
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xE0]); // r0x40
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x02]); // r0x4a (bitslip!)

        // Re-read after 1ms settle: still 0x02 (latched, warm restart)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x02]);

        let mut ep1 = mock_channel_none(&mock);
        // Should succeed with a warning, not error
        let result = set_ad9637_enabled_true(&mut ep1).unwrap();
        assert!(result.locked);
        assert_eq!(result.status_40, 0xE0);
        assert_eq!(result.status_4a, 0x02);
    }

    // -------------------------------------------------------------------
    // power_on_for_capture tests
    // -------------------------------------------------------------------

    #[test]
    fn power_on_for_capture_writes_bank_power_then_enables_adc() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // 1 write_register_8 ACK (r0x15=0x03) +
        // set_ad9637_enabled_false (1 ADC write + 5 JESD + 2 reads = 8) +
        // set_ad9637_enabled_true (3 ADC writes + 5 JESD + 2 reads + 3 reads = 13)
        push_write_acks(&mock, 1);
        push_enabled_false_responses(&mock, 0x00, 0x00);
        push_enabled_true_responses(&mock, 0xE0, 0x00);

        let mut ep1 = mock_channel_none(&mock);
        let result = power_on_for_capture(&mut ep1).unwrap();

        assert!(result.locked);
        assert_eq!(result.status_40, 0xE0);

        let payloads = extract_out_payloads(&mock);
        // 1 + 8 + 13 = 22
        assert_eq!(payloads.len(), 22);

        // First command: bank power on (r0x15=0x03)
        assert_single_write(&payloads, 0, 0x15, 0x03);
    }

    // -------------------------------------------------------------------
    // power_down tests
    // -------------------------------------------------------------------

    #[test]
    fn power_down_writes_bank_power_off_then_disables_adc() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // 1 write_register_8 ACK (r0x15=0x00) + set_ad9637_enabled_false
        push_write_acks(&mock, 1);
        push_enabled_false_responses(&mock, 0x00, 0x00);

        let mut ep1 = mock_channel_none(&mock);
        power_down(&mut ep1).unwrap();

        let payloads = extract_out_payloads(&mock);
        // 1 + 8 = 9
        assert_eq!(payloads.len(), 9);

        // First command: bank power off (r0x15=0x00)
        assert_single_write(&payloads, 0, 0x15, 0x00);

        // Second: SPI transfer via write_adc_register
        assert_adc_write(&payloads, 1, 0x08, 0x01);
    }

    #[test]
    fn power_down_propagates_write_error() {
        use saleae_usb::error::UsbError;

        let mock = std::sync::Arc::new(MockTransport::new());
        mock.push_bulk_in_error(endpoint::BULK_IN, UsbError::Timeout { timeout_ms: 1000 });

        let mut ep1 = mock_channel_none(&mock);
        let result = power_down(&mut ep1);
        assert!(
            result.is_err(),
            "should propagate error from write_register_8"
        );
    }

    // -------------------------------------------------------------------
    // init_ad9637 tests
    // -------------------------------------------------------------------

    #[test]
    fn init_ad9637_full_sequence_command_count() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let cal = [0xAA; 16];

        // configure_ad9637: 9 ACKs
        push_write_acks(&mock, 9);
        // write_register_8 (r0x15=0x00): 1 ACK
        push_write_acks(&mock, 1);
        // set_ad9637_enabled_false: 1 + 5 + 2 = 8
        push_enabled_false_responses(&mock, 0x00, 0x00);
        // set_ad9637_enabled_true: 3 + 5 + 2 + 3 = 13
        push_enabled_true_responses(&mock, 0xE0, 0x00);
        // write_calibration_and_cascade:
        //   2 write_register_8 ACKs (r0x00=0x00, r0x00=0x40)
        push_write_acks(&mock, 2);
        //   16 write_register_8 ACKs (calibration coefficients)
        push_write_acks(&mock, 16);
        //   set_ad9637_enabled_false: 1 + 5 + 2 = 8
        push_enabled_false_responses(&mock, 0x00, 0x00);
        //   1 read_register_8 (r0x40)
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        //   1 write_register_8 (r0x12=0x04)
        push_write_acks(&mock, 1);

        let mut ep1 = mock_channel_none(&mock);
        let result = init_ad9637(&mut ep1, &cal).unwrap();

        // The result comes from set_ad9637_enabled_true (before calibration)
        assert!(result.locked);
        assert_eq!(result.status_40, 0xE0);

        let payloads = extract_out_payloads(&mock);
        // 9 + 1 + 8 + 13 + 2 + 16 + 8 + 1 + 1 = 59
        assert_eq!(
            payloads.len(),
            59,
            "init_ad9637 full sequence command count"
        );
    }

    #[test]
    fn init_ad9637_starts_with_configure_ad9637_sequence() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let cal = [0xBB; 16];

        // Full init responses (same as above)
        push_write_acks(&mock, 9);
        push_write_acks(&mock, 1);
        push_enabled_false_responses(&mock, 0x00, 0x00);
        push_enabled_true_responses(&mock, 0xE0, 0x00);
        push_write_acks(&mock, 2);
        push_write_acks(&mock, 16);
        push_enabled_false_responses(&mock, 0x00, 0x00);
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]);
        push_write_acks(&mock, 1);

        let mut ep1 = mock_channel_none(&mock);
        init_ad9637(&mut ep1, &cal).unwrap();

        let payloads = extract_out_payloads(&mock);

        // First 9 payloads should be configure_ad9637
        assert_adc_write(&payloads, 0, 0x08, 0x03); // reset+standby
        assert_adc_write(&payloads, 1, 0x08, 0x00); // power normal

        // Payload 9: r0x15=0x00 (ADC output power off)
        assert_single_write(&payloads, 9, 0x15, 0x00);
    }

    #[test]
    fn init_ad9637_errors_if_dcm_fails_to_lock() {
        let mock = std::sync::Arc::new(MockTransport::new());
        let cal = [0x00; 16];

        // configure_ad9637 (9 writes) + bank power off (1 write)
        push_write_acks(&mock, 9);
        push_write_acks(&mock, 1);

        // DCM_LOCK_MAX_ATTEMPTS attempts, each: enabled_false + enabled_true (fails)
        for _ in 0..DCM_LOCK_MAX_ATTEMPTS {
            push_enabled_false_responses(&mock, 0x00, 0x00);
            // set_ad9637_enabled_true: 3 writes + DCM cascade with 0x00 (fail)
            push_write_acks(&mock, 3);
            push_dcm_cascade_responses(&mock, 0x00, 0x00);
        }

        let mut ep1 = mock_channel_none(&mock);
        let err = init_ad9637(&mut ep1, &cal).unwrap_err();
        assert!(
            err.to_string().contains("Main DCM didn't lock"),
            "expected DCM lock error, got: {err}"
        );
    }

    // -------------------------------------------------------------------
    // reset_main_dcm tests
    // -------------------------------------------------------------------

    #[test]
    fn reset_main_dcm_returns_lock_result() {
        let mock = std::sync::Arc::new(MockTransport::new());

        push_dcm_cascade_responses(&mock, 0xA0, 0x00);

        let mut ep1 = mock_channel_none(&mock);
        let result = reset_main_dcm(&mut ep1).unwrap();

        // DCM_MAIN_LOCKED = 0xA0: bits 5 (main) + 7 (system PLL)
        assert!(result.locked, "0xA0 should be considered locked");
        assert_eq!(result.status_40, 0xA0);
        assert_eq!(result.status_4a, 0x00);
    }

    #[test]
    fn reset_main_dcm_unlocked_when_status_missing_bits() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // Only bit 7 set (system PLL), missing bit 5 (main DCM)
        push_dcm_cascade_responses(&mock, 0x80, 0x00);

        let mut ep1 = mock_channel_none(&mock);
        let result = reset_main_dcm(&mut ep1).unwrap();

        assert!(
            !result.locked,
            "0x80 should NOT be considered locked (missing main DCM bit)"
        );
    }

    #[test]
    fn dcm_cascade_runs_5step_jesd_and_reads_status() {
        let mock = std::sync::Arc::new(MockTransport::new());

        // 5 writes (JESD power sequence: 0x1E, 0x1C, 0x18, 0x08, 0x00) + 2 reads (r0x40, r0x4a)
        for _ in 0..5 {
            mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // write ACK
        }
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0xE0]); // r0x40
        mock.push_bulk_in_response(endpoint::BULK_IN, vec![0x00]); // r0x4a

        let mut ep1 = mock_channel_none(&mock);
        let (r40, r4a) = dcm_cascade(&mut ep1).unwrap();

        assert_eq!(r40, 0xE0, "r0x40 should be 0xE0");
        assert_eq!(r4a, 0x00, "r0x4a should be 0x00");

        let payloads = extract_out_payloads(&mock);
        // 5 writes + 2 reads = 7 commands
        assert_eq!(payloads.len(), 7);
        // Verify JESD power sequence values in write payloads
        // Wire format: [opcode=0x80, count=0x01, addr=0x00, value]
        assert_eq!(payloads[0][3], 0x1E, "first write should be 0x1E");
        assert_eq!(payloads[1][3], 0x1C, "second write should be 0x1C");
        assert_eq!(payloads[2][3], 0x18, "third write should be 0x18");
        assert_eq!(payloads[3][3], 0x08, "fourth write should be 0x08");
        assert_eq!(payloads[4][3], 0x00, "fifth write should be 0x00");
    }

    #[test]
    fn dcm_lock_constants() {
        assert_eq!(DCM_ALL_LOCKED, 0xE0);
        assert_eq!(DCM_MAIN_LOCKED, 0xA0);
        assert_eq!(DCM_MAIN_BIT, 0x20);
        assert_eq!(DCM_SLOW_BIT, 0x40);
        assert_eq!(BITSLIP_ADC0, 0x02);
        assert_eq!(BITSLIP_ADC1, 0x04);
        assert_eq!(DCM_ALL_LOCKED, DCM_MAIN_BIT | DCM_SLOW_BIT | 0x80);
    }

    #[test]
    fn ad9637_register_values() {
        // Verify our AD9637 register constants match the pcap-observed values.
        assert_eq!(AD9637_POWER_MODES, 0x08);
        assert_eq!(AD9637_TEST_MODE, 0x0D);
        assert_eq!(AD9637_OUTPUT_ADJUST, 0x15);
        assert_eq!(AD9637_DCO_CONFIG, 0x21);
        assert_eq!(AD9637_OUTPUT_MODE_2, 0x05);
        assert_eq!(AD9637_OUTPUT_MODE_1, 0x04);
        assert_eq!(AD9637_CLOCK_CONFIG, 0x10);
        assert_eq!(AD9637_VREF_CONFIG, 0x18);

        assert_eq!(AD9637_RESET_AND_STANDBY, 0x03);
        assert_eq!(AD9637_POWER_NORMAL, 0x00);
        assert_eq!(AD9637_SPI_TRANSFER, 0x01);
    }
}
