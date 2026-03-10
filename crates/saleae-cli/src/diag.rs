//! Diagnostic commands for debugging EP1 communication.
//!
//! This module provides a low-level EP1 test that bypasses the normal
//! `Ep1CommandChannel` and uses thread-based timeouts to avoid hanging
//! when the firmware doesn't respond. This is essential for debugging
//! because the normal `NusbTransport` bulk transfers have no timeout
//! and will block forever if the device's DMA is not processing.

use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};

/// EP1 diagnostic test.
///
/// Tests whether the firmware's EP1 command processing is functional
/// by sending a 4-byte reseed followed by an encrypted ping. Uses
/// thread-based timeouts to prevent indefinite blocking.
///
/// # Background
///
/// The FX3 firmware sets up EP1 DMA channels in response to the SETCONF
/// USB event (triggered by SET_CONFIGURATION). The `LogicProThread_Entry`
/// function runs a command processing loop that calls `GetCommandRxBuffer()`
/// to receive data from the EP1 OUT DMA channel. If the command thread
/// is not consuming from the DMA buffer, the first bulk OUT transfer
/// will succeed (data goes into the DMA hardware buffer) but the second
/// will hang (DMA buffer is full, firmware never drains it).
///
/// If EP1 is not working, the most likely cause is stale firmware state.
/// A physical power cycle of the Saleae device (unplug and replug) will
/// reset the FX3 to bootloader mode, after which fresh firmware can be
/// uploaded.
pub fn raw_ep1_test() -> Result<()> {
    let device =
        saleae_usb::discovery::find_first_saleae().context("failed to find Saleae device")?;

    println!("Found device: {device}");
    println!("  Boot state: {}", device.boot_state);
    if let Some(speed) = device.usb_speed {
        println!("  USB speed:  {speed}");
    }
    println!();

    let usb_device =
        saleae_usb::discovery::open_first_device().context("failed to open Saleae device")?;

    // Check active configuration before claiming.
    match usb_device.active_configuration() {
        Ok(config) => println!("Active configuration: {}", config.configuration_value()),
        Err(e) => println!("Active configuration: unknown ({e})"),
    }

    // SET_CONFIGURATION(1) triggers the FX3 firmware's SETCONF event handler,
    // which creates the EP1 DMA channels. Without this, EP1 commands hang.
    match usb_device.set_configuration(1) {
        Ok(()) => println!("set_configuration(1): OK"),
        Err(e) => println!("set_configuration(1): {e}"),
    }

    let interface = usb_device
        .claim_interface(0)
        .map_err(|e| anyhow::anyhow!("failed to claim interface: {e}"))?;

    println!("Interface 0 claimed.");

    // Clear any stale endpoint halts.
    let _ = interface.clear_halt(0x01);
    let _ = interface.clear_halt(0x81);

    // Small delay to let firmware process the alt setting change.
    std::thread::sleep(Duration::from_millis(200));
    println!();

    let seed_bytes = [0x01, 0x02, 0x03, 0x04];
    let seed = u32::from_le_bytes(seed_bytes);

    // -----------------------------------------------------------------------
    // Step 1: Send reseed (4 raw bytes on EP1 OUT)
    // -----------------------------------------------------------------------
    println!("[1/4] Sending reseed (4 bytes) on EP1 OUT...");
    match bulk_out_with_timeout(
        &interface,
        0x01,
        seed_bytes.to_vec(),
        Duration::from_secs(3),
    ) {
        Ok((status, len, elapsed)) => {
            println!("      OUT: {elapsed:?}, status={status:?}, len={len}");
            if status.is_err() {
                println!("      STALL or error. Endpoints may need clear_halt.");
                println!();
                println!("RESULT: EP1 OUT not functional.");
                return Ok(());
            }
        }
        Err(e) => {
            println!("      {e}");
            println!();
            println!("RESULT: EP1 OUT hung (DMA not accepting data).");
            return Ok(());
        }
    }
    println!("      First OUT accepted by DMA.");
    println!();

    // -----------------------------------------------------------------------
    // Step 2: Send encrypted ping on EP1 OUT
    // -----------------------------------------------------------------------
    let mut enc = saleae_proto::SimpleEncryption::new(seed, saleae_proto::EncryptionMode::XorAll);
    let encrypted_ping = enc.encrypt_command(0x7D, &[0x55], false);
    println!(
        "[2/4] Sending encrypted ping {:02x?} on EP1 OUT...",
        &encrypted_ping
    );
    match bulk_out_with_timeout(&interface, 0x01, encrypted_ping, Duration::from_secs(3)) {
        Ok((status, len, elapsed)) => {
            println!("      OUT: {elapsed:?}, status={status:?}, len={len}");
            if status.is_err() {
                println!("      STALL. Firmware processed first transfer but rejected second.");
                println!();
                println!("RESULT: Firmware consumed reseed but rejected ping.");
                println!("        Possible encryption mismatch or command format error.");
                return Ok(());
            }
        }
        Err(e) => {
            println!("      {e}");
            println!();
            println!("RESULT: Second OUT hung. Firmware did NOT consume the first");
            println!("        transfer from the DMA buffer. The command processing");
            println!("        thread (LogicProThread_Entry) is not running or blocked.");
            println!();
            println!("        This indicates stale firmware state. The device needs");
            println!("        a physical power cycle (unplug/replug) to reset the FX3");
            println!("        back to bootloader mode, then fresh firmware upload.");
            return Ok(());
        }
    }
    println!("      Second OUT accepted. Firmware is consuming data.");
    println!();

    // -----------------------------------------------------------------------
    // Step 3: Read ping response on EP1 IN
    // -----------------------------------------------------------------------
    println!("[3/4] Reading ping response on EP1 IN...");
    match bulk_in_with_timeout(&interface, 0x81, 1024, Duration::from_secs(3)) {
        Ok((mut data, elapsed)) => {
            println!(
                "      IN: {elapsed:?}, {} bytes raw: {:02x?}",
                data.len(),
                data
            );
            enc.decrypt_response(&mut data, false);
            println!("      Decrypted: {:02x?}", &data);

            // ---------------------------------------------------------------
            // Step 4: Verify ping response
            // ---------------------------------------------------------------
            println!();
            if !data.is_empty() && data[0] == 0xAA {
                println!("[4/4] PING SUCCESS: got 0xAA (complement of 0x55)");
                println!();
                println!("RESULT: EP1 command channel is fully functional.");
                println!("        XOR encryption is in sync with firmware.");
                println!("        The LFSR implementation is correct.");
            } else if !data.is_empty() {
                println!("[4/4] PING MISMATCH: expected 0xAA, got {:#04x}", data[0]);
                println!();
                println!("RESULT: Firmware responded but the value is wrong.");
                println!("        This likely means the LFSR implementation or");
                println!("        encryption mode is incorrect.");
            } else {
                println!("[4/4] Empty response (0 bytes).");
                println!();
                println!("RESULT: Firmware processed the command but sent no data.");
            }
        }
        Err(e) => {
            println!("      {e}");
            println!();
            println!("RESULT: EP1 IN hung. Firmware accepted the command data but");
            println!("        did not send a response. The command may have been");
            println!("        processed but the response DMA channel is not active.");
        }
    }

    Ok(())
}

fn bulk_out_with_timeout(
    interface: &nusb::Interface,
    endpoint: u8,
    data: Vec<u8>,
    timeout: Duration,
) -> Result<(
    std::result::Result<(), nusb::transfer::TransferError>,
    usize,
    Duration,
)> {
    let (tx, rx) = mpsc::channel();
    let iface = interface.clone();
    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let c = futures_lite::future::block_on(iface.bulk_out(endpoint, data));
        let _ = tx.send((c.status, c.data.actual_length(), start.elapsed()));
    });
    rx.recv_timeout(timeout)
        .map_err(|_| anyhow::anyhow!("bulk OUT timed out after {timeout:?}"))
}

fn bulk_in_with_timeout(
    interface: &nusb::Interface,
    endpoint: u8,
    max_len: usize,
    timeout: Duration,
) -> Result<(Vec<u8>, Duration)> {
    let (tx, rx) = mpsc::channel();
    let iface = interface.clone();
    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let buf = nusb::transfer::RequestBuffer::new(max_len);
        let c = futures_lite::future::block_on(iface.bulk_in(endpoint, buf));
        if let Err(e) = c.status {
            let _ = tx.send(Err(anyhow::anyhow!("bulk IN error: {e}")));
        } else {
            let _ = tx.send(Ok((c.data.to_vec(), start.elapsed())));
        }
    });
    rx.recv_timeout(timeout)
        .map_err(|_| anyhow::anyhow!("bulk IN timed out after {timeout:?}"))?
}
