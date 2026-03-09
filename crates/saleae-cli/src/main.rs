//! CLI tool for Saleae Logic analyzers.
//!
//! Provides subcommands for device discovery, firmware upload, and capture.
//! This is a thin wrapper around the `saleae-driver` crate.

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

    /// Raw EP1 diagnostic test (bypasses encryption).
    ///
    /// Sends raw bytes on EP1 OUT/IN to test if the endpoints are
    /// responsive at all. Used for debugging.
    Ep1Test,

    /// Start an analog capture (Pro devices only).
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

    match cli.command {
        Command::ListDevices => commands::list_devices()?,
        Command::UploadFirmware { firmware } => commands::upload_firmware(&firmware)?,
        Command::Ping => commands::ping()?,
        Command::ProgramFpga {
            bitstream,
            skip_post_init,
        } => commands::program_fpga(&bitstream, skip_post_init)?,
        Command::Ep1Test => diag::raw_ep1_test()?,
        Command::CaptureDigital {
            sample_rate,
            duration,
            channels,
        } => commands::capture_digital(sample_rate, duration, &channels)?,
        Command::CaptureAnalog {
            sample_rate,
            duration,
            channels,
        } => commands::capture_analog(sample_rate, duration, &channels)?,
    }

    Ok(())
}
