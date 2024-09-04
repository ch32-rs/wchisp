//! USB Transportation.
use std::time::Duration;

use anyhow::Result;
use rusb::{Context, DeviceHandle, UsbContext};

use super::Transport;

const ENDPOINT_OUT: u8 = 0x02;
const ENDPOINT_IN: u8 = 0x82;

const USB_TIMEOUT_MS: u64 = 5000;

pub struct UsbTransport {
    device_handle: DeviceHandle<rusb::Context>,
}

impl UsbTransport {
    pub fn scan_devices() -> Result<usize> {
        let context = Context::new()?;

        let n = context
            .devices()?
            .iter()
            .filter(|device| {
                device
                    .device_descriptor()
                    .map(|desc| {
                        (desc.vendor_id() == 0x4348 || desc.vendor_id() == 0x1a86)
                            && desc.product_id() == 0x55e0
                    })
                    .unwrap_or(false)
            })
            .enumerate()
            .map(|(i, device)| {
                log::debug!("Found WCH ISP USB device #{}: [{:?}]", i, device);
            })
            .count();
        Ok(n)
    }

    pub fn open_nth(nth: usize) -> Result<UsbTransport> {
        log::info!("Opening USB device #{}", nth);

        let context = Context::new()?;

        let device = context
            .devices()?
            .iter()
            .filter(|device| {
                device
                    .device_descriptor()
                    .map(|desc| {
                        (desc.vendor_id() == 0x4348 || desc.vendor_id() == 0x1a86)
                            && desc.product_id() == 0x55e0
                    })
                    .unwrap_or(false)
            })
            .nth(nth)
            .ok_or(anyhow::format_err!(
                "No WCH ISP USB device found(4348:55e0 or 1a86:55e0 device not found at index #{})",
                nth
            ))?;
        log::debug!("Found USB Device {:?}", device);

        let device_handle = match device.open() {
            Ok(handle) => handle,
            #[cfg(target_os = "windows")]
            Err(rusb::Error::NotSupported) => {
                log::error!("Failed to open USB device: {:?}", device);
                log::warn!("It's likely no WinUSB/LibUSB drivers installed. Please install it from Zadig. See also: https://zadig.akeo.ie");
                anyhow::bail!("Failed to open USB device on Windows");
            }
            #[cfg(target_os = "linux")]
            Err(rusb::Error::Access) => {
                log::error!("Failed to open USB device: {:?}", device);
                log::warn!("It's likely the udev rules is not installed properly. Please refer to README.md for more details.");
                anyhow::bail!("Failed to open USB device on Linux due to no enough permission");
            }
            Err(e) => {
                log::error!("Failed to open USB device: {}", e);
                anyhow::bail!("Failed to open USB device");
            }
        };

        let config = device.config_descriptor(0)?;

        let mut endpoint_out_found = false;
        let mut endpoint_in_found = false;
        if let Some(intf) = config.interfaces().next() {
            if let Some(desc) = intf.descriptors().next() {
                for endpoint in desc.endpoint_descriptors() {
                    if endpoint.address() == ENDPOINT_OUT {
                        endpoint_out_found = true;
                    }
                    if endpoint.address() == ENDPOINT_IN {
                        endpoint_in_found = true;
                    }
                }
            }
        }

        if !(endpoint_out_found && endpoint_in_found) {
            anyhow::bail!("USB Endpoints not found");
        }

        device_handle.set_active_configuration(1)?;
        let _config = device.active_config_descriptor()?;
        let _descriptor = device.device_descriptor()?;

        device_handle.claim_interface(0)?;

        Ok(UsbTransport { device_handle })
    }

    pub fn open_any() -> Result<UsbTransport> {
        Self::open_nth(0)
    }
}

impl Drop for UsbTransport {
    fn drop(&mut self) {
        // ignore any communication error
        let _ = self.device_handle.release_interface(0);
        // self.device_handle.reset().unwrap();
    }
}

impl Transport for UsbTransport {
    fn send_raw(&mut self, raw: &[u8]) -> Result<()> {
        self.device_handle
            .write_bulk(ENDPOINT_OUT, raw, Duration::from_millis(USB_TIMEOUT_MS))?;
        Ok(())
    }

    fn recv_raw(&mut self, timeout: Duration) -> Result<Vec<u8>> {
        let mut buf = [0u8; 64];
        let nread = self
            .device_handle
            .read_bulk(ENDPOINT_IN, &mut buf, timeout)?;
        Ok(buf[..nread].to_vec())
    }
}
