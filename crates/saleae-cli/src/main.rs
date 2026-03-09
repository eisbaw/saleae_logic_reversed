//! CLI tool for Saleae Logic analyzers.
//!
//! Provides subcommands for device discovery, firmware upload, and capture.
//! This is a thin wrapper around the `saleae-driver` crate.

use clap::{Parser, Subcommand};

mod commands;

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
