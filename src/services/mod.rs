mod printer;
mod usb_driver;

pub use printer::{PrinterService, DEFAULT_VENDOR_ID, DEFAULT_PRODUCT_ID};
pub use usb_driver::{CustomUsbDriver, UsbConfig};
