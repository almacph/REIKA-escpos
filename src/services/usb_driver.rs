use escpos::driver::Driver;
use escpos::errors::{PrinterError, Result};
use rusb::{Context, DeviceHandle, Direction, TransferType, UsbContext};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use super::sensor_reporter::SensorEvent;

const DEFAULT_TIMEOUT_SECONDS: u64 = 5;

#[derive(Clone)]
pub struct CustomUsbDriver {
    vendor_id: u16,
    product_id: u16,
    output_endpoint: u8,
    input_endpoint: u8,
    device: Arc<Mutex<DeviceHandle<Context>>>,
    timeout: Duration,
    sensor_tx: Option<tokio::sync::mpsc::Sender<SensorEvent>>,
}

#[derive(Clone, Debug)]
pub struct UsbConfig {
    pub vendor_id: u16,
    pub product_id: u16,
    pub endpoint: Option<u8>,
    pub interface: Option<u8>,
}

impl CustomUsbDriver {
    pub fn open(config: &UsbConfig) -> Result<Self> {
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

        for device in devices.iter() {
            let device_descriptor = device
                .device_descriptor()
                .map_err(|e: rusb::Error| PrinterError::Io(e.to_string()))?;

            if device_descriptor.vendor_id() == config.vendor_id
                && device_descriptor.product_id() == config.product_id
            {
                let config_descriptor = device
                    .active_config_descriptor()
                    .map_err(|e: rusb::Error| PrinterError::Io(e.to_string()))?;

                // Print all available endpoints for debugging
                Self::print_device_info(&config_descriptor);

                // Try to find endpoints - use manual values if provided, otherwise auto-discover
                let (output_endpoint, input_endpoint, interface_number) =
                    if let (Some(ep), Some(iface)) = (config.endpoint, config.interface) {
                        // Manual endpoint configuration
                        // endpoint is the OUT endpoint, IN endpoint is typically endpoint | 0x80
                        let out_ep = ep;
                        let in_ep = ep | 0x80;
                        log::debug!(
                            "Using manual USB config: interface={}, out_ep=0x{:02X}, in_ep=0x{:02X}",
                            iface, out_ep, in_ep
                        );
                        (out_ep, in_ep, iface)
                    } else {
                        // Auto-discover endpoints (original behavior)
                        Self::discover_endpoints(&config_descriptor)?
                    };

                let device_handle: DeviceHandle<Context> = device.open()
                    .map_err(|e: rusb::Error| PrinterError::Io(e.to_string()))?;

                #[cfg(not(target_os = "windows"))]
                match device_handle.kernel_driver_active(interface_number) {
                    Ok(active) => {
                        if active {
                            device_handle.detach_kernel_driver(interface_number)
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
                    "USB device opened successfully: VID=0x{:04X}, PID=0x{:04X}, out_ep=0x{:02X}, in_ep=0x{:02X}",
                    config.vendor_id,
                    config.product_id,
                    output_endpoint,
                    input_endpoint
                );
                return Ok(Self {
                    vendor_id: config.vendor_id,
                    product_id: config.product_id,
                    output_endpoint,
                    input_endpoint,
                    device: Arc::new(Mutex::new(device_handle)),
                    timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECONDS),
                    sensor_tx: None,
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

    fn send_sensor_event(&self, event: SensorEvent) {
        if let Some(tx) = &self.sensor_tx {
            let _ = tx.try_send(event);
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

    fn discover_endpoints(
        config_descriptor: &rusb::ConfigDescriptor,
    ) -> Result<(u8, u8, u8)> {
        config_descriptor
            .interfaces()
            .flat_map(|interface| interface.descriptors())
            .flat_map(|descriptor| {
                let interface_number = descriptor.interface_number();

                let mut input_endpoint = None;
                let mut output_endpoint = None;
                for endpoint in descriptor.endpoint_descriptors() {
                    if endpoint.transfer_type() == TransferType::Bulk
                        && endpoint.direction() == Direction::In
                    {
                        input_endpoint = Some(endpoint.address());
                    } else if endpoint.transfer_type() == TransferType::Bulk
                        && endpoint.direction() == Direction::Out
                    {
                        output_endpoint = Some(endpoint.address());
                    }
                }

                match (output_endpoint, input_endpoint) {
                    (Some(out), Some(inp)) => Some((out, inp, interface_number)),
                    _ => None,
                }
            })
            .next()
            .ok_or_else(|| {
                PrinterError::Io(
                    "no suitable endpoints or interface number found for USB device".to_string(),
                )
            })
    }

}

impl Driver for CustomUsbDriver {
    fn name(&self) -> String {
        format!(
            "CustomUSB (VID: 0x{:04X}, PID: 0x{:04X}, out: 0x{:02X}, in: 0x{:02X})",
            self.vendor_id, self.product_id, self.output_endpoint, self.input_endpoint
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
            log::error!("USB mutex lock FAILED after {:?}: {}", lock_start.elapsed(), e);
            PrinterError::Io(e.to_string())
        })?;
        log::debug!("USB mutex acquired in {:?}", lock_start.elapsed());

        let write_start = Instant::now();
        let result = device.write_bulk(self.output_endpoint, data, self.timeout);

        match &result {
            Ok(bytes_written) => {
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
            }
            Err(e) => {
                log::error!(
                    "USB write FAILED to endpoint 0x{:02X} after {:?}: {} (rusb error kind: {:?})",
                    self.output_endpoint,
                    write_start.elapsed(),
                    e,
                    e
                );
                self.send_sensor_event(SensorEvent::UsbError(
                    format!("USB write failed to endpoint 0x{:02X}: {}", self.output_endpoint, e)
                ));
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
        let size = self
            .device
            .lock()
            .map_err(|e| PrinterError::Io(e.to_string()))?
            .read_bulk(self.input_endpoint, buf, self.timeout)
            .map_err(|e| PrinterError::Io(e.to_string()))?;
        Ok(size)
    }

    fn flush(&self) -> Result<()> {
        Ok(())
    }
}
