//! Firmware upload operations.
//!
//! Implements the FX3 firmware upload protocol documented in
//! `doc/protocol/fx3-firmware.md`.

use saleae_proto::firmware::fx3::{self, Fx3Image};
use saleae_usb::transport::{ControlTransferParams, UsbTransport};
use tracing::{debug, info};

use crate::error::DriverResult;

/// Upload an FX3 firmware image to a device.
///
/// This sends the firmware sections as USB vendor control transfers
/// (bmRequestType=0x40, bRequest=0xA0) in chunks of up to 4096 bytes.
///
/// After all sections are sent, a zero-length transfer to the entry
/// address triggers firmware execution. The device will re-enumerate
/// with its Saleae VID:PID.
///
/// # Safety
///
/// This writes to volatile RAM only. A power cycle restores the
/// bootloader. See `doc/protocol/fx3-firmware.md` for the safety
/// analysis.
pub fn upload_fx3_firmware(transport: &dyn UsbTransport, image: &Fx3Image) -> DriverResult<()> {
    info!(
        sections = image.sections.len(),
        total_bytes = image.total_data_bytes(),
        entry = format_args!("{:#010x}", image.entry_address),
        "uploading FX3 firmware"
    );

    // Send each section in chunks.
    for (i, (addr, chunk)) in image.upload_chunks().enumerate() {
        debug!(
            chunk = i,
            address = format_args!("{addr:#010x}"),
            len = chunk.len(),
            "sending firmware chunk"
        );

        let params = ControlTransferParams {
            bm_request_type: fx3::usb::BM_REQUEST_TYPE,
            b_request: fx3::usb::B_REQUEST,
            w_value: (addr & 0xFFFF) as u16,
            w_index: (addr >> 16) as u16,
        };

        transport.control_transfer_out(&params, chunk)?;
    }

    // Send zero-length transfer to entry address to trigger execution.
    info!(
        entry = format_args!("{:#010x}", image.entry_address),
        "triggering firmware execution"
    );

    let params = ControlTransferParams {
        bm_request_type: fx3::usb::BM_REQUEST_TYPE,
        b_request: fx3::usb::B_REQUEST,
        w_value: (image.entry_address & 0xFFFF) as u16,
        w_index: (image.entry_address >> 16) as u16,
    };

    transport.control_transfer_out(&params, &[])?;

    info!("firmware upload complete; device should re-enumerate");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_proto::firmware::fx3::{Fx3Header, Fx3Image, Fx3Section, IMAGE_TYPE_NORMAL};
    use saleae_usb::mock::{MockTransport, UsbOperation};

    fn make_test_image() -> Fx3Image {
        Fx3Image {
            header: Fx3Header {
                image_ctl: 0x00,
                image_type: IMAGE_TYPE_NORMAL,
            },
            sections: vec![
                Fx3Section {
                    address: 0x0000_0100,
                    data: vec![0xAAu8; 256],
                },
                Fx3Section {
                    address: 0x4000_3000,
                    data: vec![0xBBu8; 128],
                },
            ],
            entry_address: 0x4000_e020,
            checksum: 0,
        }
    }

    #[test]
    fn upload_sends_correct_control_transfers() {
        let mock = MockTransport::new();
        let image = make_test_image();

        upload_fx3_firmware(&mock, &image).unwrap();

        let ops = mock.operations();

        // Both sections fit in one chunk each (< 4096 bytes).
        // Plus one zero-length entry transfer.
        assert_eq!(
            ops.len(),
            3,
            "expected 3 control transfers, got {}",
            ops.len()
        );

        // First section.
        match &ops[0] {
            UsbOperation::ControlOut {
                bm_request_type,
                b_request,
                w_value,
                w_index,
                data,
            } => {
                assert_eq!(*bm_request_type, 0x40);
                assert_eq!(*b_request, 0xA0);
                assert_eq!(*w_value, 0x0100); // low 16 bits of 0x00000100
                assert_eq!(*w_index, 0x0000); // high 16 bits
                assert_eq!(data.len(), 256);
            }
            other => panic!("expected ControlOut, got {other:?}"),
        }

        // Second section.
        match &ops[1] {
            UsbOperation::ControlOut {
                w_value,
                w_index,
                data,
                ..
            } => {
                assert_eq!(*w_value, 0x3000); // low 16 bits of 0x40003000
                assert_eq!(*w_index, 0x4000); // high 16 bits
                assert_eq!(data.len(), 128);
            }
            other => panic!("expected ControlOut, got {other:?}"),
        }

        // Entry trigger (zero-length).
        match &ops[2] {
            UsbOperation::ControlOut {
                w_value,
                w_index,
                data,
                ..
            } => {
                assert_eq!(*w_value, 0xe020); // low 16 bits of 0x4000e020
                assert_eq!(*w_index, 0x4000); // high 16 bits
                assert!(data.is_empty(), "entry transfer should be zero-length");
            }
            other => panic!("expected ControlOut, got {other:?}"),
        }
    }

    #[test]
    fn upload_chunks_large_section() {
        let mock = MockTransport::new();
        let image = Fx3Image {
            header: Fx3Header {
                image_ctl: 0x00,
                image_type: IMAGE_TYPE_NORMAL,
            },
            sections: vec![Fx3Section {
                address: 0x4000_3000,
                // 10000 bytes = 2 full chunks of 4096 + 1 partial chunk of 1808
                data: vec![0xCCu8; 10000],
            }],
            entry_address: 0x4000_e020,
            checksum: 0,
        };

        upload_fx3_firmware(&mock, &image).unwrap();

        let ops = mock.operations();
        // 3 data chunks + 1 entry transfer = 4 operations.
        assert_eq!(ops.len(), 4);
    }
}
