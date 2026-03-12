//! USB hotplug device management with polling-based detection.
//!
//! This module implements the `DeviceManager` which detects device
//! connect/disconnect events by periodically comparing the current USB
//! device list against a set of known (tracked) devices. This matches
//! the Saleae binary's `LinuxDevicesManager::CheckForConnectionThread`
//! approach: polling via `libusb_get_device_list` rather than hotplug
//! callbacks.
//!
//! # Architecture
//!
//! ```text
//! DeviceManager
//!   |
//!   +-- poll() -> Vec<DeviceEvent>
//!   |     |
//!   |     +-- enumerator.list_devices()   // get current USB state
//!   |     +-- diff against known_devices  // detect changes
//!   |     +-- emit Connected / Disconnected events
//!   |
//!   +-- known_devices() -> &[TrackedDevice]
//!   +-- remove_device(bus, address)
//! ```
//!
//! The `DeviceEnumerator` trait abstracts USB enumeration so the manager
//! is testable without real hardware.
//!
//! # Usage
//!
//! ```ignore
//! use saleae_driver::device_manager::{DeviceManager, UsbDeviceEnumerator};
//!
//! let mut manager = DeviceManager::new(UsbDeviceEnumerator);
//!
//! loop {
//!     let events = manager.poll().unwrap();
//!     for event in events {
//!         println!("{event:?}");
//!     }
//!     std::thread::sleep(std::time::Duration::from_secs(1));
//! }
//! ```

use std::time::Instant;

use saleae_proto::device::{DeviceType, SaleaeDevice};

use crate::error::DriverResult;

// ---------------------------------------------------------------------------
// Device enumerator trait
// ---------------------------------------------------------------------------

/// Trait for enumerating USB devices.
///
/// This abstracts the USB bus scan so `DeviceManager` can be tested
/// with mock device lists. The production implementation wraps
/// `saleae_usb::discovery::list_devices()`.
pub trait DeviceEnumerator {
    /// List all Saleae devices currently on the USB bus.
    fn list_devices(&self) -> DriverResult<Vec<SaleaeDevice>>;
}

/// Production USB device enumerator using `saleae_usb::discovery`.
///
/// Requires the `nusb-backend` feature on `saleae-usb` (enabled by default).
pub struct UsbDeviceEnumerator;

impl DeviceEnumerator for UsbDeviceEnumerator {
    fn list_devices(&self) -> DriverResult<Vec<SaleaeDevice>> {
        Ok(saleae_usb::discovery::list_devices()?)
    }
}

// ---------------------------------------------------------------------------
// Device key (bus + address)
// ---------------------------------------------------------------------------

/// Unique identifier for a USB device on the bus.
///
/// The (bus, address) pair uniquely identifies a device connection.
/// If a device is unplugged and re-plugged, it typically gets a new
/// address, so this pair changes -- which is exactly what we want for
/// detecting reconnections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceKey {
    pub bus: u8,
    pub address: u8,
}

impl DeviceKey {
    pub fn new(bus: u8, address: u8) -> Self {
        Self { bus, address }
    }
}

impl std::fmt::Display for DeviceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bus {:03} addr {:03}", self.bus, self.address)
    }
}

// ---------------------------------------------------------------------------
// Tracked device state
// ---------------------------------------------------------------------------

/// Lifecycle state of a tracked device.
///
/// Mirrors the Saleae binary's device state machine at the manager
/// level (not the per-device DSInitializing/DSReady/DSError variant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackedDeviceState {
    /// Device just appeared on USB bus, not yet initialized.
    Discovered,
    /// Device is being initialized (firmware upload, FPGA programming).
    Initializing,
    /// Device is fully initialized and ready for use.
    Ready,
    /// Device was previously known but is no longer on the USB bus.
    Lost,
}

impl std::fmt::Display for TrackedDeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discovered => f.write_str("Discovered"),
            Self::Initializing => f.write_str("Initializing"),
            Self::Ready => f.write_str("Ready"),
            Self::Lost => f.write_str("Lost"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tracked device
// ---------------------------------------------------------------------------

/// A device being tracked by the DeviceManager.
#[derive(Debug, Clone)]
pub struct TrackedDevice {
    /// Device type (Logic Pro 8, Logic 8, etc.).
    pub device_type: DeviceType,
    /// USB bus number.
    pub bus: u8,
    /// USB device address.
    pub address: u8,
    /// Serial number, if available.
    pub serial: Option<String>,
    /// When this device was first seen.
    pub first_seen: Instant,
    /// Current lifecycle state.
    pub state: TrackedDeviceState,
}

impl TrackedDevice {
    /// Create a new tracked device from a discovered USB device.
    fn from_saleae_device(dev: &SaleaeDevice) -> Self {
        Self {
            device_type: dev.device_type,
            bus: dev.bus,
            address: dev.address,
            serial: dev.serial.clone(),
            first_seen: Instant::now(),
            state: TrackedDeviceState::Discovered,
        }
    }

    /// Return the device key (bus, address) for this device.
    pub fn key(&self) -> DeviceKey {
        DeviceKey::new(self.bus, self.address)
    }
}

impl std::fmt::Display for TrackedDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at {} [{}]", self.device_type, self.key(), self.state,)?;
        if let Some(serial) = &self.serial {
            write!(f, " serial={serial}")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Device events
// ---------------------------------------------------------------------------

/// Events emitted by the DeviceManager when the USB device list changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    /// A new Saleae device appeared on the USB bus.
    Connected {
        device_type: DeviceType,
        bus: u8,
        address: u8,
        serial: Option<String>,
    },
    /// A previously tracked device is no longer on the USB bus.
    Disconnected { bus: u8, address: u8 },
}

impl std::fmt::Display for DeviceEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceEvent::Connected {
                device_type,
                bus,
                address,
                serial,
            } => {
                write!(
                    f,
                    "Connected: {device_type} at bus {bus:03} addr {address:03}"
                )?;
                if let Some(s) = serial {
                    write!(f, " serial={s}")?;
                }
                Ok(())
            }
            DeviceEvent::Disconnected { bus, address } => {
                write!(f, "Disconnected: bus {bus:03} addr {address:03}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DeviceManager
// ---------------------------------------------------------------------------

/// Manages USB device hotplug detection via polling.
///
/// Each call to [`poll`](DeviceManager::poll) enumerates USB devices
/// and compares against the tracked set, emitting `Connected` and
/// `Disconnected` events for changes.
///
/// The polling approach matches the Saleae binary's
/// `CheckForConnectionThread` which calls `GetAllDevices()` in a loop
/// rather than using libusb hotplug callbacks.
pub struct DeviceManager<E: DeviceEnumerator> {
    enumerator: E,
    devices: Vec<TrackedDevice>,
}

impl<E: DeviceEnumerator> DeviceManager<E> {
    /// Create a new DeviceManager with the given enumerator.
    pub fn new(enumerator: E) -> Self {
        Self {
            enumerator,
            devices: Vec::new(),
        }
    }

    /// Poll for device changes.
    ///
    /// Enumerates the current USB device list and diffs against the
    /// tracked set. Returns a list of events:
    /// - `Connected` for devices present on USB but not in the tracked set.
    /// - `Disconnected` for devices in the tracked set but not on USB.
    ///
    /// Connected devices are added to the tracked set with state `Discovered`.
    /// Disconnected devices are removed from the tracked set.
    pub fn poll(&mut self) -> DriverResult<Vec<DeviceEvent>> {
        let current = self.enumerator.list_devices()?;
        let mut events = Vec::new();

        // Detect disconnected devices: in tracked set but not in current scan.
        // Collect indices to remove (in reverse order to avoid index shifting).
        let mut to_remove = Vec::new();
        for (i, tracked) in self.devices.iter().enumerate() {
            let still_present = current
                .iter()
                .any(|dev| dev.bus == tracked.bus && dev.address == tracked.address);
            if !still_present {
                events.push(DeviceEvent::Disconnected {
                    bus: tracked.bus,
                    address: tracked.address,
                });
                to_remove.push(i);
            }
        }

        // Remove disconnected devices (reverse order to preserve indices).
        for &i in to_remove.iter().rev() {
            self.devices.remove(i);
        }

        // Detect new devices: in current scan but not in tracked set.
        for dev in &current {
            let already_tracked = self
                .devices
                .iter()
                .any(|tracked| tracked.bus == dev.bus && tracked.address == dev.address);
            if !already_tracked {
                events.push(DeviceEvent::Connected {
                    device_type: dev.device_type,
                    bus: dev.bus,
                    address: dev.address,
                    serial: dev.serial.clone(),
                });
                self.devices.push(TrackedDevice::from_saleae_device(dev));
            }
        }

        Ok(events)
    }

    /// Return the currently tracked devices.
    pub fn known_devices(&self) -> &[TrackedDevice] {
        &self.devices
    }

    /// Manually remove a device from tracking.
    ///
    /// Use this after failed communication to force the device out of
    /// the tracked set. The next `poll()` will re-detect it if it is
    /// still physically present, emitting a new `Connected` event.
    ///
    /// Returns `true` if the device was found and removed.
    pub fn remove_device(&mut self, bus: u8, address: u8) -> bool {
        let before = self.devices.len();
        self.devices
            .retain(|d| !(d.bus == bus && d.address == address));
        self.devices.len() < before
    }

    /// Update the state of a tracked device.
    ///
    /// Returns `true` if the device was found and updated.
    pub fn set_device_state(&mut self, bus: u8, address: u8, state: TrackedDeviceState) -> bool {
        if let Some(dev) = self
            .devices
            .iter_mut()
            .find(|d| d.bus == bus && d.address == address)
        {
            dev.state = state;
            true
        } else {
            false
        }
    }

    /// Find a tracked device by bus and address.
    pub fn find_device(&self, bus: u8, address: u8) -> Option<&TrackedDevice> {
        self.devices
            .iter()
            .find(|d| d.bus == bus && d.address == address)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use saleae_proto::device::{BootState, UsbSpeed};
    use std::cell::RefCell;

    /// Mock enumerator that returns a configurable device list.
    struct MockEnumerator {
        devices: RefCell<Vec<SaleaeDevice>>,
    }

    impl MockEnumerator {
        fn new() -> Self {
            Self {
                devices: RefCell::new(Vec::new()),
            }
        }

        fn set_devices(&self, devices: Vec<SaleaeDevice>) {
            *self.devices.borrow_mut() = devices;
        }
    }

    impl DeviceEnumerator for MockEnumerator {
        fn list_devices(&self) -> DriverResult<Vec<SaleaeDevice>> {
            Ok(self.devices.borrow().clone())
        }
    }

    /// Helper to create a SaleaeDevice for testing.
    fn make_device(device_type: DeviceType, bus: u8, address: u8) -> SaleaeDevice {
        SaleaeDevice {
            usb_id: device_type.usb_id(),
            device_type,
            bus,
            address,
            serial: Some("SERNUM".to_string()),
            boot_state: BootState::FirmwareRunning,
            usb_speed: Some(UsbSpeed::Super),
        }
    }

    // -------------------------------------------------------------------
    // Basic lifecycle tests
    // -------------------------------------------------------------------

    #[test]
    fn new_manager_has_no_devices() {
        let enumerator = MockEnumerator::new();
        let manager = DeviceManager::new(enumerator);
        assert!(manager.known_devices().is_empty());
    }

    #[test]
    fn poll_with_no_devices_returns_no_events() {
        let enumerator = MockEnumerator::new();
        let mut manager = DeviceManager::new(enumerator);
        let events = manager.poll().unwrap();
        assert!(events.is_empty());
        assert!(manager.known_devices().is_empty());
    }

    // -------------------------------------------------------------------
    // Connection detection
    // -------------------------------------------------------------------

    #[test]
    fn poll_detects_new_device() {
        let enumerator = MockEnumerator::new();
        let mut manager = DeviceManager::new(enumerator);

        // No devices initially.
        let events = manager.poll().unwrap();
        assert!(events.is_empty());

        // Device appears.
        let dev = make_device(DeviceType::LogicPro8, 2, 5);
        manager.enumerator.set_devices(vec![dev]);

        let events = manager.poll().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            DeviceEvent::Connected {
                device_type: DeviceType::LogicPro8,
                bus: 2,
                address: 5,
                serial: Some("SERNUM".to_string()),
            }
        );

        // Device is now tracked.
        assert_eq!(manager.known_devices().len(), 1);
        assert_eq!(
            manager.known_devices()[0].device_type,
            DeviceType::LogicPro8
        );
        assert_eq!(manager.known_devices()[0].bus, 2);
        assert_eq!(manager.known_devices()[0].address, 5);
        assert_eq!(
            manager.known_devices()[0].state,
            TrackedDeviceState::Discovered
        );
    }

    #[test]
    fn poll_detects_device_removal() {
        let enumerator = MockEnumerator::new();
        let mut manager = DeviceManager::new(enumerator);

        // Device appears.
        let dev = make_device(DeviceType::LogicPro8, 2, 5);
        manager.enumerator.set_devices(vec![dev]);
        manager.poll().unwrap();
        assert_eq!(manager.known_devices().len(), 1);

        // Device disappears.
        manager.enumerator.set_devices(vec![]);
        let events = manager.poll().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], DeviceEvent::Disconnected { bus: 2, address: 5 });

        // No longer tracked.
        assert!(manager.known_devices().is_empty());
    }

    // -------------------------------------------------------------------
    // Stable device produces no events
    // -------------------------------------------------------------------

    #[test]
    fn same_device_produces_no_event_on_repoll() {
        let enumerator = MockEnumerator::new();
        let mut manager = DeviceManager::new(enumerator);

        let dev = make_device(DeviceType::LogicPro8, 2, 5);
        manager.enumerator.set_devices(vec![dev.clone()]);

        // First poll: Connected event.
        let events = manager.poll().unwrap();
        assert_eq!(events.len(), 1);

        // Second poll: same device still there, no events.
        let events = manager.poll().unwrap();
        assert!(events.is_empty());

        // Third poll: still no events.
        let events = manager.poll().unwrap();
        assert!(events.is_empty());
    }

    // -------------------------------------------------------------------
    // Multiple devices
    // -------------------------------------------------------------------

    #[test]
    fn multiple_devices_tracked_independently() {
        let enumerator = MockEnumerator::new();
        let mut manager = DeviceManager::new(enumerator);

        let dev1 = make_device(DeviceType::LogicPro8, 1, 3);
        let dev2 = make_device(DeviceType::Logic8, 2, 7);
        manager
            .enumerator
            .set_devices(vec![dev1.clone(), dev2.clone()]);

        let events = manager.poll().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(manager.known_devices().len(), 2);

        // Remove one device.
        manager.enumerator.set_devices(vec![dev1.clone()]);
        let events = manager.poll().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], DeviceEvent::Disconnected { bus: 2, address: 7 });
        assert_eq!(manager.known_devices().len(), 1);
        assert_eq!(
            manager.known_devices()[0].device_type,
            DeviceType::LogicPro8
        );
    }

    #[test]
    fn simultaneous_connect_and_disconnect() {
        let enumerator = MockEnumerator::new();
        let mut manager = DeviceManager::new(enumerator);

        // Device A is present.
        let dev_a = make_device(DeviceType::LogicPro8, 1, 3);
        manager.enumerator.set_devices(vec![dev_a.clone()]);
        manager.poll().unwrap();

        // Device A disappears, device B appears.
        let dev_b = make_device(DeviceType::Logic4, 2, 10);
        manager.enumerator.set_devices(vec![dev_b.clone()]);

        let events = manager.poll().unwrap();
        assert_eq!(events.len(), 2);

        // Should have disconnect for A and connect for B.
        let disconnect = events
            .iter()
            .find(|e| matches!(e, DeviceEvent::Disconnected { .. }));
        let connect = events
            .iter()
            .find(|e| matches!(e, DeviceEvent::Connected { .. }));
        assert!(disconnect.is_some());
        assert!(connect.is_some());

        assert_eq!(
            *disconnect.unwrap(),
            DeviceEvent::Disconnected { bus: 1, address: 3 }
        );
        assert_eq!(
            *connect.unwrap(),
            DeviceEvent::Connected {
                device_type: DeviceType::Logic4,
                bus: 2,
                address: 10,
                serial: Some("SERNUM".to_string()),
            }
        );
    }

    // -------------------------------------------------------------------
    // Manual removal
    // -------------------------------------------------------------------

    #[test]
    fn remove_device_manually() {
        let enumerator = MockEnumerator::new();
        let mut manager = DeviceManager::new(enumerator);

        let dev = make_device(DeviceType::LogicPro8, 2, 5);
        manager.enumerator.set_devices(vec![dev.clone()]);
        manager.poll().unwrap();
        assert_eq!(manager.known_devices().len(), 1);

        // Manually remove (e.g., after communication failure).
        assert!(manager.remove_device(2, 5));
        assert!(manager.known_devices().is_empty());

        // Remove nonexistent device returns false.
        assert!(!manager.remove_device(99, 99));
    }

    #[test]
    fn removed_device_re_detected_on_next_poll() {
        let enumerator = MockEnumerator::new();
        let mut manager = DeviceManager::new(enumerator);

        let dev = make_device(DeviceType::LogicPro8, 2, 5);
        manager.enumerator.set_devices(vec![dev.clone()]);
        manager.poll().unwrap();

        // Manually remove while device is still physically present.
        manager.remove_device(2, 5);

        // Next poll should re-detect it as Connected.
        let events = manager.poll().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], DeviceEvent::Connected { .. }));
        assert_eq!(manager.known_devices().len(), 1);
    }

    // -------------------------------------------------------------------
    // State management
    // -------------------------------------------------------------------

    #[test]
    fn set_device_state() {
        let enumerator = MockEnumerator::new();
        let mut manager = DeviceManager::new(enumerator);

        let dev = make_device(DeviceType::LogicPro8, 2, 5);
        manager.enumerator.set_devices(vec![dev]);
        manager.poll().unwrap();

        assert_eq!(
            manager.known_devices()[0].state,
            TrackedDeviceState::Discovered
        );

        assert!(manager.set_device_state(2, 5, TrackedDeviceState::Initializing));
        assert_eq!(
            manager.known_devices()[0].state,
            TrackedDeviceState::Initializing
        );

        assert!(manager.set_device_state(2, 5, TrackedDeviceState::Ready));
        assert_eq!(manager.known_devices()[0].state, TrackedDeviceState::Ready);

        // Nonexistent device returns false.
        assert!(!manager.set_device_state(99, 99, TrackedDeviceState::Lost));
    }

    // -------------------------------------------------------------------
    // Find device
    // -------------------------------------------------------------------

    #[test]
    fn find_device_by_key() {
        let enumerator = MockEnumerator::new();
        let mut manager = DeviceManager::new(enumerator);

        let dev = make_device(DeviceType::LogicPro8, 2, 5);
        manager.enumerator.set_devices(vec![dev]);
        manager.poll().unwrap();

        assert!(manager.find_device(2, 5).is_some());
        assert_eq!(
            manager.find_device(2, 5).unwrap().device_type,
            DeviceType::LogicPro8
        );
        assert!(manager.find_device(99, 99).is_none());
    }

    // -------------------------------------------------------------------
    // Device reconnection (different address)
    // -------------------------------------------------------------------

    #[test]
    fn device_reconnect_new_address_produces_disconnect_then_connect() {
        let enumerator = MockEnumerator::new();
        let mut manager = DeviceManager::new(enumerator);

        // Device at address 5.
        let dev1 = make_device(DeviceType::LogicPro8, 2, 5);
        manager.enumerator.set_devices(vec![dev1]);
        manager.poll().unwrap();

        // Device re-enumerates at address 6 (simulating unplug/replug).
        let dev2 = make_device(DeviceType::LogicPro8, 2, 6);
        manager.enumerator.set_devices(vec![dev2]);

        let events = manager.poll().unwrap();
        assert_eq!(events.len(), 2);

        assert_eq!(events[0], DeviceEvent::Disconnected { bus: 2, address: 5 });
        assert_eq!(
            events[1],
            DeviceEvent::Connected {
                device_type: DeviceType::LogicPro8,
                bus: 2,
                address: 6,
                serial: Some("SERNUM".to_string()),
            }
        );
    }

    // -------------------------------------------------------------------
    // Display implementations
    // -------------------------------------------------------------------

    #[test]
    fn device_event_display() {
        let connected = DeviceEvent::Connected {
            device_type: DeviceType::LogicPro8,
            bus: 2,
            address: 5,
            serial: Some("SERNUM".to_string()),
        };
        let s = connected.to_string();
        assert!(s.contains("Connected"));
        assert!(s.contains("LogicPro8"));
        assert!(s.contains("SERNUM"));

        let disconnected = DeviceEvent::Disconnected { bus: 2, address: 5 };
        let s = disconnected.to_string();
        assert!(s.contains("Disconnected"));
    }

    #[test]
    fn tracked_device_state_display() {
        assert_eq!(TrackedDeviceState::Discovered.to_string(), "Discovered");
        assert_eq!(TrackedDeviceState::Initializing.to_string(), "Initializing");
        assert_eq!(TrackedDeviceState::Ready.to_string(), "Ready");
        assert_eq!(TrackedDeviceState::Lost.to_string(), "Lost");
    }

    #[test]
    fn device_key_display() {
        let key = DeviceKey::new(2, 5);
        assert_eq!(key.to_string(), "bus 002 addr 005");
    }
}
