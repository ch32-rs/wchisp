//! USB Transportation.
use std::io::{Read, Write};
use std::time::Duration;

use anyhow::Result;
use nusb::transfer::{Bulk, In, Out};
use nusb::MaybeFuture;

use super::Transport;

const ENDPOINT_OUT: u8 = 0x02;
const ENDPOINT_IN: u8 = 0x82;

#[allow(dead_code)]
const USB_TIMEOUT_MS: u64 = 5000;

/// Check if a device matches WCH ISP VID/PID
fn is_wch_isp_device(info: &nusb::DeviceInfo) -> bool {
    let vid = info.vendor_id();
    let pid = info.product_id();
    (vid == 0x4348 || vid == 0x1a86) && pid == 0x55e0
}

pub struct UsbTransport {
    interface: Option<nusb::Interface>,
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    ch375_index: isize,
}

impl UsbTransport {
    pub fn scan_devices() -> Result<usize> {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let devices_ch375 = ch375_driver::list_devices()?;
            let devices_ch375_count = devices_ch375.len();
            if devices_ch375_count > 0 {
                log::debug!("Found {} CH375USBDevice", devices_ch375_count);
                return Ok(devices_ch375_count);
            }
        }

        let n = nusb::list_devices()
            .wait()?
            .filter(is_wch_isp_device)
            .enumerate()
            .map(|(i, device)| {
                log::debug!(
                    "Found WCH ISP USB device #{}: {:04x}:{:04x}",
                    i,
                    device.vendor_id(),
                    device.product_id()
                );
            })
            .count();
        Ok(n)
    }

    pub fn open_nth(nth: usize) -> Result<UsbTransport> {
        log::info!("Opening USB device #{}", nth);

        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let ch375_index = ch375_driver::open_nth(nth)?;
            if ch375_index >= 0 {
                return Ok(UsbTransport {
                    interface: None,
                    ch375_index,
                });
            }
        }

        let device_info = nusb::list_devices()
            .wait()?
            .filter(is_wch_isp_device)
            .nth(nth)
            .ok_or_else(|| {
                anyhow::format_err!(
                    "No WCH ISP USB device found (4348:55e0 or 1a86:55e0 device not found at index #{})",
                    nth
                )
            })?;

        log::debug!(
            "Found USB Device {:04x}:{:04x}",
            device_info.vendor_id(),
            device_info.product_id()
        );

        let device = device_info.open().wait().map_err(|e| {
            log::error!("Failed to open USB device: {}", e);
            #[cfg(target_os = "windows")]
            log::warn!("It's likely no WinUSB driver installed. Please install it from Zadig. See also: https://zadig.akeo.ie");
            #[cfg(target_os = "linux")]
            log::warn!("It's likely the udev rules are not installed properly. Please refer to README.md for more details.");
            anyhow::anyhow!("Failed to open USB device: {}", e)
        })?;

        let interface = device.claim_interface(0).wait().map_err(|e| {
            log::error!("Failed to claim interface: {}", e);
            anyhow::anyhow!("Failed to claim USB interface: {}", e)
        })?;

        Ok(UsbTransport {
            interface: Some(interface),
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            ch375_index: -1,
        })
    }

    pub fn open_any() -> Result<UsbTransport> {
        Self::open_nth(0)
    }
}

impl Drop for UsbTransport {
    fn drop(&mut self) {
        // nusb Interface is automatically released on drop
        if self.interface.is_none() {
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            {
                if self.ch375_index >= 0 {
                    ch375_driver::drop(self.ch375_index as usize);
                }
            }
        }
    }
}

impl Transport for UsbTransport {
    fn send_raw(&mut self, raw: &[u8]) -> Result<()> {
        if let Some(ref interface) = self.interface {
            let endpoint = interface
                .endpoint::<Bulk, Out>(ENDPOINT_OUT)
                .map_err(|e| anyhow::anyhow!("Failed to get OUT endpoint: {}", e))?;
            let mut writer = endpoint.writer(64);
            writer.write_all(raw)?;
            writer.flush()?;
        } else {
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            {
                if self.ch375_index >= 0 {
                    ch375_driver::write_raw(self.ch375_index as usize, raw)?;
                    return Ok(());
                }
            }
            anyhow::bail!("USB device handle is None while ch375_index is negative or not set");
        }
        Ok(())
    }

    fn recv_raw(&mut self, _timeout: Duration) -> Result<Vec<u8>> {
        if let Some(ref interface) = self.interface {
            let endpoint = interface
                .endpoint::<Bulk, In>(ENDPOINT_IN)
                .map_err(|e| anyhow::anyhow!("Failed to get IN endpoint: {}", e))?;
            let mut reader = endpoint.reader(64);
            let mut buf = [0u8; 64];
            let n = reader.read(&mut buf)?;
            Ok(buf[..n].to_vec())
        } else {
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            {
                if self.ch375_index >= 0 {
                    let mut buf = [0u8; 64];
                    let len = ch375_driver::read_raw(self.ch375_index as usize, &mut buf)?;
                    return Ok(buf[..len].to_vec());
                }
            }
            anyhow::bail!("USB device handle is None while ch375_index is negative or not set");
        }
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub mod ch375_driver {
    use libloading::os::windows::*;
    use std::sync::OnceLock;

    use super::*;

    static CH375_DRIVER: OnceLock<Library> = OnceLock::new();

    fn ensure_library_load() -> Result<&'static Library> {
        if let Some(lib) = CH375_DRIVER.get() {
            return Ok(lib);
        }

        let lib = unsafe { Library::new("CH375DLL64.dll") }.map_err(|_| {
            anyhow::Error::msg(
                "CH375DLL64.dll not found. \
                Please download it from the WCH official website \
                and put the dll next to this executable. \
                You may download it from https://www.wch-ic.com/downloads/CH372DRV_ZIP.html, \
                or search for 'CH375' in the WCH websites if the link is broken.",
            )
        })?;

        let get_version: Symbol<unsafe extern "system" fn() -> u32> =
            unsafe { lib.get(b"CH375GetVersion").unwrap() };
        let get_driver_version: Symbol<unsafe extern "system" fn() -> u32> =
            unsafe { lib.get(b"CH375GetDrvVersion").unwrap() };

        log::debug!(
            "DLL version {}, driver version {}",
            unsafe { get_version() },
            unsafe { get_driver_version() }
        );

        Ok(CH375_DRIVER.get_or_init(|| lib))
    }

    #[allow(non_snake_case, unused)]
    #[derive(Debug)]
    #[repr(packed)]
    pub struct UsbDeviceDescriptor {
        bLength: u8,
        bDescriptorType: u8,
        bcdUSB: u16,
        bDeviceClass: u8,
        bDeviceSubClass: u8,
        bDeviceProtocol: u8,
        bMaxPacketSize0: u8,
        idVendor: u16,
        idProduct: u16,
        bcdDevice: u16,
        iManufacturer: u8,
        iProduct: u8,
        iSerialNumber: u8,
        bNumConfigurations: u8,
    }

    pub fn list_devices() -> Result<Vec<String>> {
        let lib = ensure_library_load()?;
        let mut ret: Vec<String> = vec![];

        let open_device: Symbol<unsafe extern "system" fn(u32) -> u32> =
            unsafe { lib.get(b"CH375OpenDevice").unwrap() };
        let close_device: Symbol<unsafe extern "system" fn(u32)> =
            unsafe { lib.get(b"CH375CloseDevice").unwrap() };
        let get_device_descriptor: Symbol<
            unsafe extern "system" fn(u32, *mut UsbDeviceDescriptor, *mut u32) -> bool,
        > = unsafe { lib.get(b"CH375GetDeviceDescr").unwrap() };

        const INVALID_HANDLE: u32 = 0xffffffff;

        for i in 0..8 {
            let h = unsafe { open_device(i) };
            if h != INVALID_HANDLE {
                let mut descr = unsafe { core::mem::zeroed() };
                let mut len = core::mem::size_of::<UsbDeviceDescriptor>() as u32;
                let _ = unsafe { get_device_descriptor(i, &mut descr, &mut len) };

                let id_vendor = descr.idVendor;
                let id_product = descr.idProduct;

                if (id_vendor == 0x4348 || id_vendor == 0x1a86) && id_product == 0x55e0 {
                    ret.push(format!(
                        "<WCH-ISP#{} WCHLinkDLL device> CH375Driver Device {:04x}:{:04x}",
                        i, id_vendor, id_product
                    ));

                    log::debug!("Device #{}: {:04x}:{:04x}", i, id_vendor, id_product);
                }
                unsafe { close_device(i) };
            }
        }

        Ok(ret)
    }

    pub fn open_nth(nth: usize) -> Result<isize> {
        let lib = ensure_library_load()?;
        let open_device: Symbol<unsafe extern "system" fn(u32) -> u32> =
            unsafe { lib.get(b"CH375OpenDevice").unwrap() };
        let close_device: Symbol<unsafe extern "system" fn(u32)> =
            unsafe { lib.get(b"CH375CloseDevice").unwrap() };
        let get_device_descriptor: Symbol<
            unsafe extern "system" fn(u32, *mut UsbDeviceDescriptor, *mut u32) -> bool,
        > = unsafe { lib.get(b"CH375GetDeviceDescr").unwrap() };

        const INVALID_HANDLE: u32 = 0xffffffff;

        let mut idx = 0;
        for i in 0..8 {
            let h = unsafe { open_device(i) };
            if h != INVALID_HANDLE {
                let mut descr = unsafe { core::mem::zeroed() };
                let mut len = core::mem::size_of::<UsbDeviceDescriptor>() as u32;
                let _ = unsafe { get_device_descriptor(i, &mut descr, &mut len) };

                let id_vendor = descr.idVendor;
                let id_product = descr.idProduct;

                if (id_vendor == 0x4348 || id_vendor == 0x1a86) && id_product == 0x55e0 {
                    if idx == nth {
                        log::debug!("Device #{}: {:04x}:{:04x}", i, id_vendor, id_product);
                        return Ok(i as isize);
                    } else {
                        idx += 1;
                    }
                }
                unsafe { close_device(i) };
            }
        }

        Ok(-1_isize)
    }

    pub fn write_raw(nth: usize, buf: &[u8]) -> Result<()> {
        let lib = ensure_library_load()?;
        let write_data: Symbol<unsafe extern "system" fn(u32, *mut u8, *mut u32) -> bool> =
            unsafe { lib.get(b"CH375WriteData").unwrap() };

        let mut len = buf.len() as u32;
        let ret = unsafe { write_data(nth as u32, buf.as_ptr() as *mut u8, &mut len) };
        if ret {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Failed to write data with CH375USBDevice"))
        }
    }

    pub fn read_raw(nth: usize, buf: &mut [u8]) -> Result<usize> {
        let lib = ensure_library_load()?;
        let read_data: Symbol<unsafe extern "system" fn(u32, *mut u8, *mut u32) -> bool> =
            unsafe { lib.get(b"CH375ReadData").unwrap() };

        let mut len = buf.len() as u32;
        let ret = unsafe { read_data(nth as u32, buf.as_mut_ptr(), &mut len) };
        if ret {
            Ok(len as usize)
        } else {
            Err(anyhow::anyhow!("Failed to read data with CH375USBDevice"))
        }
    }

    #[allow(dead_code)]
    pub fn set_timeout(nth: usize, timeout: Duration) {
        let lib = ensure_library_load().unwrap();

        let set_timeout_ex: Symbol<unsafe extern "system" fn(u32, u32, u32, u32, u32) -> bool> =
            unsafe { lib.get(b"CH375SetTimeoutEx").unwrap() };

        let ds = timeout.as_millis() as u32;

        unsafe {
            set_timeout_ex(nth as u32, ds, ds, ds, ds);
        }
    }

    pub fn drop(nth: usize) {
        let lib = ensure_library_load().unwrap();
        let close_device: Symbol<unsafe extern "system" fn(u32)> =
            unsafe { lib.get(b"CH375CloseDevice").unwrap() };
        unsafe {
            close_device(nth as u32);
        }
    }
}
