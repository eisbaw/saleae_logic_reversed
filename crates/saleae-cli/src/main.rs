//! CLI tool for Saleae Logic analyzers.
//!
//! Provides subcommands for device discovery, firmware upload, and capture.
//! This is a thin wrapper around the `saleae-driver` crate.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::{Parser, Subcommand};

mod commands;
mod diag;

/// Saleae Logic analyzer CLI tool.
///
/// An open-source tool for driving Saleae Logic hardware without the
/// proprietary GUI. Part of the saleae_reverse project.
#[derive(Parser)]
#[command(name = "saleae-cli", version, about)]
struct Cli {
    /// Enable verbose logging (repeat for more: -v, -vv, -vvv).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Path to FX3 firmware image for auto-recovery.
    ///
    /// When specified, commands that need EP1 will automatically detect
    /// a dead command thread and recover by resetting the device to
    /// bootloader mode, re-uploading firmware, and re-initializing.
    #[arg(long, global = true)]
    firmware: Option<String>,

    /// Filter devices by USB serial number (case-insensitive substring match).
    ///
    /// When multiple Saleae devices are connected, use this to select a
    /// specific device. The filter is matched against the USB serial number
    /// descriptor. Use `list-devices` to see available serials.
    #[arg(long, global = true)]
    serial: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected Saleae devices.
    ListDevices,

    /// Upload firmware to a Saleae device.
    UploadFirmware {
        /// Path to the FX3 firmware image file.
        #[arg(long)]
        firmware: String,
    },

    /// Start a digital capture.
    ///
    /// Performs the full sequence: firmware upload (auto-recovery),
    /// FPGA programming, ADC init, capture, and teardown.
    CaptureDigital {
        /// Sample rate in Hz.
        #[arg(long, default_value = "1000000")]
        sample_rate: u64,

        /// Duration in seconds (0 = until Ctrl-C).
        #[arg(long, default_value = "1.0")]
        duration: f64,

        /// Digital channels to enable (comma-separated, e.g. "0,1,2,3").
        #[arg(long, default_value = "0,1,2,3,4,5,6,7")]
        channels: String,

        /// Output directory for exported data (CSV or binary files).
        #[arg(long)]
        output: Option<String>,

        /// Export format: "csv" (default), "binary", or "sal".
        #[arg(long, default_value = "csv")]
        format: String,

        /// Path to FX3 firmware image (overrides global --firmware).
        #[arg(long)]
        firmware: Option<String>,

        /// Path to FPGA bitstream file.
        #[arg(long)]
        bitstream: Option<String>,
    },

    /// Ping the device (ReseedDevice + Ping handshake).
    ///
    /// Tests firmware responsiveness and XOR encryption sync.
    /// Requires firmware to be already running on the device.
    Ping,

    /// Program the FPGA with a bitstream file.
    ///
    /// Requires firmware to be already running on the device.
    /// After programming, runs the post-init sequence (DDR, JESD, GPIF).
    ProgramFpga {
        /// Path to the FPGA bitstream file.
        #[arg(long)]
        bitstream: String,

        /// Skip post-init sequence (for debugging).
        #[arg(long, default_value = "false")]
        skip_post_init: bool,
    },

    /// Run CommonDeviceInit (ADC init, JESD link, EEPROM read).
    ///
    /// Requires firmware to be running and FPGA to be programmed.
    /// Initializes the ADC, establishes the JESD204B link, and reads
    /// the device ID from EEPROM. The device ID is used for calibration
    /// data download.
    DeviceInit {
        /// Path to the FPGA bitstream file (programs FPGA first if provided).
        #[arg(long)]
        bitstream: Option<String>,

        /// Skip FPGA programming (assume already programmed).
        #[arg(long, default_value = "false")]
        skip_fpga: bool,
    },

    /// Monitor USB bus for Saleae device connect/disconnect events.
    ///
    /// Polls the USB bus every second and prints events as they happen.
    /// Runs until Ctrl+C.
    Monitor {
        /// Poll interval in milliseconds.
        #[arg(long, default_value = "1000")]
        interval_ms: u64,
    },

    /// Raw EP1 diagnostic test (bypasses encryption).
    ///
    /// Sends raw bytes on EP1 OUT/IN to test if the endpoints are
    /// responsive at all. Used for debugging.
    Ep1Test,

    /// Reset and re-enumerate the device (returns to bootloader).
    ///
    /// Sends ResetAndReEnumerate (opcode 0x82) via EP1.
    /// After this, the device should appear in bootloader mode (bcdDevice=1.00)
    /// and firmware can be re-uploaded.
    ResetDevice,

    /// Full init with auto-recovery: firmware upload + FPGA + ADC + capture.
    ///
    /// Works from any device state (bootloader, firmware running, dead EP1).
    /// Uses --firmware (global) or auto-detects firmware path.
    /// Does the complete sequence:
    /// 1. Ensure firmware is running (auto-recovery if EP1 thread is dead)
    /// 2. FPGA program + verify + ADC init + DCM lock
    /// 3. Digital capture
    FullInit {
        /// Path to FX3 firmware image (overrides global --firmware).
        #[arg(long)]
        firmware: Option<String>,

        /// Path to FPGA bitstream.
        #[arg(long, default_value = "firmware/bitstream-spartan6-slx9-03.bin")]
        bitstream: String,

        /// Auto-detect the correct bitstream by trying all SLX9 variants.
        #[arg(long, default_value = "false")]
        auto_bitstream: bool,

        /// Path to calibration .cal file (derived from device serial if omitted).
        #[arg(long)]
        cal_file: Option<String>,
    },

    /// Start an analog capture (Pro devices only).
    ///
    /// Performs the full sequence: firmware upload (auto-recovery),
    /// FPGA programming, ADC init, capture, disaggregation, calibration,
    /// and export.
    CaptureAnalog {
        /// Sample rate in Hz.
        #[arg(long, default_value = "1000000")]
        sample_rate: u64,

        /// Duration in seconds (0 = until Ctrl-C).
        #[arg(long, default_value = "1.0")]
        duration: f64,

        /// Analog channels to enable (comma-separated, e.g. "0,1").
        #[arg(long, default_value = "0,1")]
        channels: String,

        /// Output directory for exported data (CSV or binary files).
        #[arg(long)]
        output: Option<String>,

        /// Export format: "csv" (default), "binary", or "sal".
        #[arg(long, default_value = "csv")]
        format: String,

        /// Path to calibration .cal file (searches common locations if omitted).
        #[arg(long)]
        cal_file: Option<String>,

        /// Path to FX3 firmware image (overrides global --firmware).
        #[arg(long)]
        firmware: Option<String>,

        /// Path to FPGA bitstream file.
        #[arg(long)]
        bitstream: Option<String>,
    },

    /// Quick capture: minimal sequence to get EP2 data flowing.
    ///
    /// Assumes firmware is already running (unless --firmware given).
    /// Optionally programs FPGA.
    /// For digital-only capture, skips all ADC/DCM init entirely.
    /// For mixed (analog+digital), does ADC init with best-effort DCM lock.
    CaptureQuick {
        /// Duration in seconds (0 = until Ctrl-C).
        #[arg(long, default_value = "2.0")]
        duration: f64,

        /// Digital channels to enable (bitmask 0x00-0xFF, e.g. "0xff" for all 8).
        #[arg(long, default_value = "0xff")]
        digital_mask: String,

        /// Analog channels to enable (bitmask 0x00-0xFF, 0x00 = digital-only).
        #[arg(long, default_value = "0x00")]
        analog_mask: String,

        /// Path to FPGA bitstream (optional; skips FPGA programming if omitted).
        #[arg(long)]
        bitstream: Option<String>,

        /// Save raw EP2 data to this file.
        #[arg(long)]
        output: Option<String>,

        /// Path to FX3 firmware image (enables auto-recovery).
        #[arg(long)]
        firmware: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Set up tracing subscriber with verbosity level.
    let filter = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .init();

    // Set up Ctrl-C handler: sets an AtomicBool flag that capture loops check.
    let stop_flag = Arc::new(AtomicBool::new(false));
    {
        let flag = Arc::clone(&stop_flag);
        ctrlc::set_handler(move || {
            // On first Ctrl-C, set the flag for graceful shutdown.
            // On second Ctrl-C, force-exit immediately.
            if flag.load(Ordering::Relaxed) {
                eprintln!("\nForce exit.");
                std::process::exit(1);
            }
            eprintln!("\nCtrl-C received, stopping capture gracefully...");
            flag.store(true, Ordering::Relaxed);
        })
        .expect("failed to set Ctrl-C handler");
    }

    let serial = cli.serial.as_deref();

    match cli.command {
        Command::ListDevices => commands::list_devices()?,
        Command::UploadFirmware { firmware } => commands::upload_firmware(&firmware, serial)?,
        Command::Ping => commands::ping(serial)?,
        Command::ProgramFpga {
            bitstream,
            skip_post_init,
        } => commands::program_fpga(&bitstream, skip_post_init, serial)?,
        Command::DeviceInit {
            bitstream,
            skip_fpga,
        } => commands::device_init(bitstream.as_deref(), skip_fpga, serial)?,
        Command::Monitor { interval_ms } => commands::monitor(interval_ms)?,
        Command::Ep1Test => diag::raw_ep1_test(serial)?,
        Command::CaptureDigital {
            sample_rate,
            duration,
            channels,
            output,
            format,
            firmware,
            bitstream,
        } => {
            let fw_path = firmware
                .or(cli.firmware.clone())
                .unwrap_or_else(|| commands::DEFAULT_FIRMWARE_PATH.to_string());
            let bs_path = bitstream.unwrap_or_else(|| commands::DEFAULT_BITSTREAM_PATH.to_string());
            commands::capture_digital(
                sample_rate,
                duration,
                &channels,
                output.as_deref(),
                &format,
                &fw_path,
                &bs_path,
                serial,
                &stop_flag,
            )?
        }
        Command::ResetDevice => commands::reset_device(serial)?,
        Command::FullInit {
            firmware,
            bitstream,
            auto_bitstream,
            cal_file,
        } => {
            let fw_path = firmware
                .or(cli.firmware.clone())
                .unwrap_or_else(|| commands::DEFAULT_FIRMWARE_PATH.to_string());
            commands::full_init(&fw_path, &bitstream, auto_bitstream, cal_file.as_deref(), serial)?
        }
        Command::CaptureAnalog {
            sample_rate,
            duration,
            channels,
            output,
            format,
            cal_file,
            firmware,
            bitstream,
        } => {
            let fw_path = firmware
                .or(cli.firmware.clone())
                .unwrap_or_else(|| commands::DEFAULT_FIRMWARE_PATH.to_string());
            let bs_path = bitstream.unwrap_or_else(|| commands::DEFAULT_BITSTREAM_PATH.to_string());
            commands::capture_analog(
                sample_rate,
                duration,
                &channels,
                output.as_deref(),
                &format,
                cal_file.as_deref(),
                &fw_path,
                &bs_path,
                serial,
                &stop_flag,
            )?
        }
        Command::CaptureQuick {
            duration,
            digital_mask,
            analog_mask,
            bitstream,
            output,
            firmware,
        } => {
            let fw_path = firmware.or(cli.firmware.clone());
            commands::capture_quick(
                duration,
                &digital_mask,
                &analog_mask,
                bitstream.as_deref(),
                output.as_deref(),
                fw_path.as_deref(),
                serial,
                &stop_flag,
            )?
        }
    }

    Ok(())
}
