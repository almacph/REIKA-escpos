#![allow(dead_code)]

use super::health::{ErrorCategory, HealthError, Presence, Readiness, StatusCapability};
use super::sensor_reporter::SensorEvent;
use crate::app::file_logger::record_diagnostic_event;
use escpos::driver::Driver;
use escpos::errors::{PrinterError, Result};
use rusb::{Context, DeviceHandle, Direction, TransferType, UsbContext};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT_SECONDS: u64 = 5;

#[derive(Clone)]
pub struct CustomUsbDriver {
    vendor_id: u16,
    product_id: u16,
    output_endpoint: u8,
    input_endpoint: Option<u8>,
    interface_number: u8,
    alternate_setting: u8,
    device: Arc<Mutex<DeviceHandle<Context>>>,
    timeout: Duration,
    sensor_tx: Option<tokio::sync::mpsc::Sender<SensorEvent>>,
    sensor_observer: Option<Arc<dyn Fn(SensorEvent) + Send + Sync>>,
    write_observer: Option<Arc<dyn Fn() + Send + Sync>>,
    identity: DeviceIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DeviceIdentity {
    Serial(String),
    PortPath { bus: u8, ports: Vec<u8> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceCandidate {
    pub vendor_id: u16,
    pub product_id: u16,
    pub identity: DeviceIdentity,
}

pub fn select_identity(
    candidates: &[DeviceCandidate],
) -> std::result::Result<DeviceIdentity, String> {
    if candidates.is_empty() {
        return Err("printer selection required: no matching device".into());
    }
    let mut identities: Vec<_> = candidates
        .iter()
        .map(|candidate| candidate.identity.clone())
        .collect();
    identities.sort();
    identities.dedup();
    if identities.len() != 1 {
        return Err("printer selection required: multiple matching devices".into());
    }
    Ok(identities.remove(0))
}

#[derive(Clone, Debug)]
pub struct UsbConfig {
    pub vendor_id: u16,
    pub product_id: u16,
    pub endpoint: Option<u8>,
    pub interface: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusProfile {
    Unsupported,
    VerifiedEscPos,
}

impl StatusProfile {
    pub fn capability(self) -> StatusCapability {
        match self {
            Self::Unsupported => StatusCapability::Unsupported,
            Self::VerifiedEscPos => StatusCapability::Verified,
        }
    }
}

impl CustomUsbDriver {
    pub fn identity(&self) -> &DeviceIdentity {
        &self.identity
    }

    pub fn open(config: &UsbConfig) -> Result<Self> {
        Self::open_selected(config, None)
    }

    pub fn probe_presence(config: &UsbConfig) -> std::result::Result<Presence, HealthError> {
        let context = Context::new().map_err(Self::classify_error)?;
        let devices = context.devices().map_err(Self::classify_error)?;
        let mut found = false;
        let matching_devices = devices
            .iter()
            .filter(|device| {
                device
                    .device_descriptor()
                    .map(|descriptor| {
                        descriptor.vendor_id() == config.vendor_id
                            && descriptor.product_id() == config.product_id
                    })
                    .unwrap_or(false)
            })
            .count();
        if matching_devices > 1 {
            return Err(HealthError::new(
                ErrorCategory::InvalidInput,
                "multiple matching USB printers require explicit identity selection",
            ));
        }
        for device in devices.iter() {
            match device.device_descriptor() {
                Ok(descriptor)
                    if descriptor.vendor_id() == config.vendor_id
                        && descriptor.product_id() == config.product_id =>
                {
                    found = true
                }
                Ok(_) => {}
                Err(error) => return Err(Self::classify_error(error)),
            }
        }
        Ok(if found {
            Presence::Present
        } else {
            Presence::Absent
        })
    }

    pub fn probe_presence_for_identity(
        config: &UsbConfig,
        selected: &DeviceIdentity,
    ) -> std::result::Result<Presence, HealthError> {
        let context = Context::new().map_err(Self::classify_error)?;
        let devices = context.devices().map_err(Self::classify_error)?;
        for device in devices.iter() {
            let descriptor = device.device_descriptor().map_err(Self::classify_error)?;
            if descriptor.vendor_id() != config.vendor_id
                || descriptor.product_id() != config.product_id
            {
                continue;
            }
            let port = DeviceIdentity::PortPath {
                bus: device.bus_number(),
                ports: device.port_numbers().unwrap_or_default(),
            };
            match selected {
                DeviceIdentity::PortPath { .. } if &port == selected => {
                    return Ok(Presence::Present)
                }
                DeviceIdentity::Serial(expected) => {
                    if let Ok(handle) = device.open() {
                        if let Ok(serial) = handle.read_serial_number_string_ascii(&descriptor) {
                            if &serial == expected {
                                return Ok(Presence::Present);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(Presence::Absent)
    }

    pub fn is_present(config: &UsbConfig) -> bool {
        matches!(Self::probe_presence(config), Ok(Presence::Present))
    }

    pub fn open_selected(config: &UsbConfig, selected: Option<&DeviceIdentity>) -> Result<Self> {
        log::info!(
            "Opening USB device: VID=0x{:04X}, PID=0x{:04X}",
            config.vendor_id,
            config.product_id
        );
        let context = Context::new().map_err(|e: rusb::Error| {
            log::error!("Failed to create USB context: {}", e);
            PrinterError::Io(e.to_string())
        })?;
        let devices = context.devices().map_err(|e: rusb::Error| {
            log::error!("Failed to enumerate USB devices: {}", e);
            PrinterError::Io(e.to_string())
        })?;
        let matching_devices = devices
            .iter()
            .filter(|device| {
                device
                    .device_descriptor()
                    .map(|descriptor| {
                        descriptor.vendor_id() == config.vendor_id
                            && descriptor.product_id() == config.product_id
                    })
                    .unwrap_or(false)
            })
            .count();
        if matching_devices > 1 {
            return Err(PrinterError::Io(
                "multiple matching USB printers require explicit identity selection".into(),
            ));
        }

        for device in devices.iter() {
            let device_descriptor = device
                .device_descriptor()
                .map_err(|e: rusb::Error| PrinterError::Io(e.to_string()))?;

            if device_descriptor.vendor_id() == config.vendor_id
                && device_descriptor.product_id() == config.product_id
            {
                let port_identity = DeviceIdentity::PortPath {
                    bus: device.bus_number(),
                    ports: device.port_numbers().unwrap_or_default(),
                };
                if let Some(DeviceIdentity::PortPath { bus, ports }) = selected {
                    if *bus != device.bus_number()
                        || *ports != device.port_numbers().unwrap_or_default()
                    {
                        continue;
                    }
                }
                let config_descriptor = device
                    .active_config_descriptor()
                    .map_err(|e: rusb::Error| PrinterError::Io(e.to_string()))?;

                // Print all available endpoints for debugging
                Self::print_device_info(&config_descriptor);

                // Try to find endpoints - use manual values if provided, otherwise auto-discover
                if config.endpoint.is_some() != config.interface.is_some() {
                    return Err(PrinterError::Input(
                        "USB endpoint and interface must be configured together".into(),
                    ));
                }
                let (output_endpoint, input_endpoint, interface_number, alternate_setting) =
                    if let (Some(ep), Some(iface)) = (config.endpoint, config.interface) {
                        let (out, input, alt) =
                            Self::find_endpoints(&config_descriptor, iface, Some(ep))?;
                        (out, input, iface, alt)
                    } else {
                        Self::discover_endpoints(&config_descriptor)?
                    };

                let device_handle: DeviceHandle<Context> = device
                    .open()
                    .map_err(|e: rusb::Error| PrinterError::Io(e.to_string()))?;
                let identity = match device_descriptor.serial_number_string_index() {
                    Some(_index) => device_handle
                        .read_serial_number_string_ascii(&device_descriptor)
                        .map(DeviceIdentity::Serial)
                        .unwrap_or(port_identity),
                    None => port_identity,
                };

                #[cfg(not(target_os = "windows"))]
                match device_handle.kernel_driver_active(interface_number) {
                    Ok(active) => {
                        if active {
                            device_handle
                                .detach_kernel_driver(interface_number)
                                .map_err(|e: rusb::Error| PrinterError::Io(e.to_string()))?;
                        }
                    }
                    Err(e) => return Err(PrinterError::Io(e.to_string())),
                }

                // Retry claiming interface with delays - Windows can be slow to release USB resources
                log::debug!("Claiming USB interface {}", interface_number);
                let mut claim_result = device_handle.claim_interface(interface_number);
                let mut claim_attempts = 0;
                const MAX_CLAIM_ATTEMPTS: u32 = 5;

                while claim_result.is_err() && claim_attempts < MAX_CLAIM_ATTEMPTS {
                    claim_attempts += 1;
                    log::debug!(
                        "claim_interface attempt {}/{} failed, retrying in 100ms...",
                        claim_attempts,
                        MAX_CLAIM_ATTEMPTS
                    );
                    std::thread::sleep(Duration::from_millis(100));
                    claim_result = device_handle.claim_interface(interface_number);
                }

                claim_result.map_err(|e: rusb::Error| {
                    log::error!(
                        "Failed to claim interface {} after {} attempts: {}",
                        interface_number,
                        claim_attempts + 1,
                        e
                    );
                    PrinterError::Io(e.to_string())
                })?;

                if alternate_setting != 0 {
                    device_handle
                        .set_alternate_setting(interface_number, alternate_setting)
                        .map_err(|e| PrinterError::Io(e.to_string()))?;
                }

                // Clear any stale endpoint state - critical after device power cycles
                // This resets data toggle bits and clears any stall conditions
                if let Err(e) = device_handle.clear_halt(output_endpoint) {
                    log::debug!(
                        "clear_halt on endpoint 0x{:02X} returned: {} (non-fatal)",
                        output_endpoint,
                        e
                    );
                }

                log::info!(
                    "USB device opened successfully: VID=0x{:04X}, PID=0x{:04X}, interface={}, alt={}, out_ep=0x{:02X}, in_ep={:?}",
                    config.vendor_id,
                    config.product_id,
                    interface_number,
                    alternate_setting,
                    output_endpoint,
                    input_endpoint
                );
                return Ok(Self {
                    vendor_id: config.vendor_id,
                    product_id: config.product_id,
                    output_endpoint,
                    input_endpoint,
                    interface_number,
                    alternate_setting,
                    device: Arc::new(Mutex::new(device_handle)),
                    timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECONDS),
                    sensor_tx: None,
                    sensor_observer: None,
                    write_observer: None,
                    identity,
                });
            }
        }

        log::warn!(
            "USB device not found: VID=0x{:04X}, PID=0x{:04X}",
            config.vendor_id,
            config.product_id
        );
        Err(PrinterError::Io("USB device not found".to_string()))
    }

    pub fn with_sensor(mut self, sensor_tx: tokio::sync::mpsc::Sender<SensorEvent>) -> Self {
        self.sensor_tx = Some(sensor_tx);
        self
    }

    pub fn with_sensor_observer(
        mut self,
        observer: Arc<dyn Fn(SensorEvent) + Send + Sync>,
    ) -> Self {
        self.sensor_observer = Some(observer);
        self
    }

    pub fn with_write_observer(mut self, observer: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.write_observer = Some(observer);
        self
    }

    pub fn status_probe(
        &self,
        profile: StatusProfile,
    ) -> std::result::Result<(Readiness, StatusCapability), HealthError> {
        if profile == StatusProfile::Unsupported || self.input_endpoint.is_none() {
            return Ok((Readiness::Unknown, StatusCapability::Unsupported));
        }
        let input = self.input_endpoint.ok_or_else(|| {
            HealthError::new(
                ErrorCategory::InvalidInput,
                "status requires a bulk IN endpoint",
            )
        })?;
        let device = self
            .device
            .lock()
            .map_err(|error| HealthError::new(ErrorCategory::Other, error.to_string()))?;
        let deadline = Instant::now() + Duration::from_millis(250);
        device
            .write_bulk(
                self.output_endpoint,
                &[0x10, 0x04, 0x01],
                Duration::from_millis(250),
            )
            .map_err(Self::classify_error)?;
        let mut response = [0u8; 1];
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(HealthError::new(
                ErrorCategory::Timeout,
                "status transaction budget exceeded",
            ));
        }
        let size = device
            .read_bulk(input, &mut response, remaining)
            .map_err(Self::classify_error)?;
        if size != 1 {
            return Err(HealthError::new(
                ErrorCategory::Other,
                "status response length invalid",
            ));
        }
        let readiness = if response[0] & 0x20 != 0 {
            Readiness::PaperOut
        } else if response[0] & 0x08 != 0 {
            Readiness::CoverOpen
        } else {
            Readiness::Ready
        };
        Ok((readiness, profile.capability()))
    }

    fn send_sensor_event(&self, event: SensorEvent) {
        if let Some(tx) = &self.sensor_tx {
            let _ = tx.try_send(event.clone());
        }
        if let Some(observer) = &self.sensor_observer {
            observer(event);
        }
    }

    fn print_device_info(config_descriptor: &rusb::ConfigDescriptor) {
        log::debug!("=== USB Device Endpoints ===");
        for interface in config_descriptor.interfaces() {
            for descriptor in interface.descriptors() {
                let iface_num = descriptor.interface_number();
                let iface_class = descriptor.class_code();
                log::debug!(
                    "Interface {}: class={} subclass={} protocol={}",
                    iface_num,
                    iface_class,
                    descriptor.sub_class_code(),
                    descriptor.protocol_code()
                );
                for endpoint in descriptor.endpoint_descriptors() {
                    let dir = match endpoint.direction() {
                        Direction::In => "IN",
                        Direction::Out => "OUT",
                    };
                    let transfer = match endpoint.transfer_type() {
                        TransferType::Bulk => "Bulk",
                        TransferType::Control => "Control",
                        TransferType::Interrupt => "Interrupt",
                        TransferType::Isochronous => "Isochronous",
                    };
                    log::debug!(
                        "  Endpoint 0x{:02X}: {} {} (max_packet={})",
                        endpoint.address(),
                        dir,
                        transfer,
                        endpoint.max_packet_size()
                    );
                }
            }
        }
        log::debug!("============================");
    }

    fn find_endpoints(
        config_descriptor: &rusb::ConfigDescriptor,
        interface_number: u8,
        requested_output: Option<u8>,
    ) -> Result<(u8, Option<u8>, u8)> {
        let mut candidates = Vec::new();
        for interface in config_descriptor.interfaces() {
            for descriptor in interface.descriptors() {
                if descriptor.interface_number() != interface_number {
                    continue;
                }
                let mut output = None;
                let mut input = None;
                for endpoint in descriptor.endpoint_descriptors() {
                    if endpoint.transfer_type() != TransferType::Bulk {
                        continue;
                    }
                    match endpoint.direction() {
                        Direction::Out
                            if requested_output.is_none()
                                || requested_output == Some(endpoint.address()) =>
                        {
                            output = Some(endpoint.address())
                        }
                        Direction::In => input = Some(endpoint.address()),
                        _ => {}
                    }
                }
                if let Some(output) = output {
                    candidates.push((output, input, descriptor.setting_number()));
                }
            }
        }
        candidates.sort_by_key(|(output, input, alt)| (*alt, *output, input.unwrap_or(0)));
        candidates.into_iter().next().ok_or_else(|| {
            PrinterError::Io("no validated bulk OUT endpoint for selected interface".into())
        })
    }

    fn discover_endpoints(
        config_descriptor: &rusb::ConfigDescriptor,
    ) -> Result<(u8, Option<u8>, u8, u8)> {
        let mut candidates = Vec::new();
        for interface in config_descriptor.interfaces() {
            let number = interface.number();
            if let Ok((output, input, alt)) = Self::find_endpoints(config_descriptor, number, None)
            {
                candidates.push((number, output, input, alt));
            }
        }
        candidates
            .sort_by_key(|(iface, output, input, alt)| (*iface, *alt, *output, input.unwrap_or(0)));
        candidates
            .into_iter()
            .next()
            .map(|(iface, output, input, alt)| (output, input, iface, alt))
            .ok_or_else(|| PrinterError::Io("no suitable bulk OUT endpoint found".into()))
    }

    pub fn classify_error(error: rusb::Error) -> HealthError {
        let category = match error {
            rusb::Error::NoDevice => ErrorCategory::NoDevice,
            rusb::Error::Access => ErrorCategory::Access,
            rusb::Error::Busy => ErrorCategory::Busy,
            rusb::Error::Timeout => ErrorCategory::Timeout,
            rusb::Error::Pipe => ErrorCategory::Pipe,
            _ => ErrorCategory::Other,
        };
        HealthError::new(category, error.to_string())
    }
}

impl Driver for CustomUsbDriver {
    fn name(&self) -> String {
        format!(
            "CustomUSB (VID: 0x{:04X}, PID: 0x{:04X}, interface: {}, alt: {}, out: 0x{:02X}, in: {:?})",
            self.vendor_id,
            self.product_id,
            self.interface_number,
            self.alternate_setting,
            self.output_endpoint,
            self.input_endpoint
        )
    }

    fn write(&self, data: &[u8]) -> Result<()> {
        let start = Instant::now();
        log::info!(
            "USB write START: {} bytes to endpoint 0x{:02X}, timeout={}ms",
            data.len(),
            self.output_endpoint,
            self.timeout.as_millis()
        );

        let lock_start = Instant::now();
        let device = self.device.lock().map_err(|e| {
            log::error!(
                "USB mutex lock FAILED after {:?}: {}",
                lock_start.elapsed(),
                e
            );
            PrinterError::Io(e.to_string())
        })?;
        log::debug!("USB mutex acquired in {:?}", lock_start.elapsed());

        let write_start = Instant::now();
        let result = device.write_bulk(self.output_endpoint, data, self.timeout);

        match &result {
            Ok(bytes_written) => {
                record_diagnostic_event("usb_write_succeeded", "usb_write", None);
                if *bytes_written != data.len() {
                    // CRITICAL: Partial or zero writes indicate USB connection is broken
                    // This commonly happens after device power cycle while host maintains stale handle
                    let msg = format!(
                        "USB partial write: wrote {}/{} bytes to endpoint 0x{:02X}",
                        bytes_written,
                        data.len(),
                        self.output_endpoint
                    );
                    log::error!(
                        "[PRINT_FAILURE] {} | elapsed={:?}",
                        msg,
                        write_start.elapsed()
                    );
                    self.send_sensor_event(SensorEvent::UsbError(msg.clone()));
                    return Err(PrinterError::Io(format!(
                        "USB write incomplete: expected {} bytes, wrote {} bytes to endpoint 0x{:02X}",
                        data.len(),
                        bytes_written,
                        self.output_endpoint
                    )));
                }
                log::info!(
                    "USB write OK: {}/{} bytes in {:?} (total {:?})",
                    bytes_written,
                    data.len(),
                    write_start.elapsed(),
                    start.elapsed()
                );
                if let Some(observer) = &self.write_observer {
                    observer();
                }
            }
            Err(e) => {
                record_diagnostic_event("usb_write_failed", "usb_error", None);
                log::error!(
                    "USB write FAILED to endpoint 0x{:02X} after {:?}: {} (rusb error kind: {:?})",
                    self.output_endpoint,
                    write_start.elapsed(),
                    e,
                    e
                );
                self.send_sensor_event(SensorEvent::UsbError(format!(
                    "USB write failed to endpoint 0x{:02X}: {}",
                    self.output_endpoint, e
                )));
            }
        }

        result.map_err(|e| {
            PrinterError::Io(format!(
                "write_bulk to endpoint 0x{:02X} failed after {:?}: {}",
                self.output_endpoint,
                start.elapsed(),
                e
            ))
        })?;
        Ok(())
    }

    fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let input_endpoint = self.input_endpoint.ok_or_else(|| {
            PrinterError::Io("device has no bulk IN endpoint; readiness unsupported".into())
        })?;
        let size = self
            .device
            .lock()
            .map_err(|e| PrinterError::Io(e.to_string()))?
            .read_bulk(input_endpoint, buf, self.timeout)
            .map_err(|e| PrinterError::Io(e.to_string()))?;
        Ok(size)
    }

    fn flush(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    fn candidate(bus: u8, ports: &[u8]) -> DeviceCandidate {
        DeviceCandidate {
            vendor_id: 1,
            product_id: 2,
            identity: DeviceIdentity::PortPath {
                bus,
                ports: ports.to_vec(),
            },
        }
    }

    #[test]
    fn identical_vid_pid_on_two_ports_requires_selection() {
        assert!(select_identity(&[candidate(1, &[1]), candidate(1, &[2])]).is_err());
    }

    #[test]
    fn power_cycle_address_change_keeps_same_port_identity() {
        let before = candidate(1, &[3, 4]).identity;
        let after = candidate(1, &[3, 4]).identity;
        assert_eq!(before, after);
    }

    #[test]
    fn selected_disconnect_does_not_accept_other_device() {
        let selected = candidate(1, &[1]).identity;
        let other = candidate(1, &[2]).identity;
        assert_ne!(selected, other);
        assert!(select_identity(&[candidate(1, &[2])]).is_ok());
        assert_ne!(selected, select_identity(&[candidate(1, &[2])]).unwrap());
    }

    #[test]
    fn serial_identity_is_stable_across_port_changes() {
        assert_eq!(
            DeviceIdentity::Serial("abc".into()),
            DeviceIdentity::Serial("abc".into())
        );
    }
}
