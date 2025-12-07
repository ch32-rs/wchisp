//! USB Transportation.
use std::time::Duration;

use anyhow::Result;
use rusb::{Context, DeviceHandle, UsbContext};

use super::Transport;

const ENDPOINT_OUT: u8 = 0x02;
const ENDPOINT_IN: u8 = 0x82;

const USB_TIMEOUT_MS: u64 = 5000;

pub struct UsbTransport {
    device_handle: Option<DeviceHandle<rusb::Context>>,
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
                //just return the count
                log::debug!("Found {} CH375USBDevice", devices_ch375_count);
                return Ok(devices_ch375_count);
            }
        }

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

        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let ch375_index = ch375_driver::open_nth(nth)?;
            if ch375_index >= 0 {
                return Ok(UsbTransport {
                    device_handle: None,
                    ch375_index,
                });
            }
        }

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

        Ok(UsbTransport {
            device_handle: Some(device_handle),
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
        // ignore any communication error
        if let Some(ref mut handle) = self.device_handle {
            let _ = handle.release_interface(0);
        } else {
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            {
                if self.ch375_index >= 0 {
                    ch375_driver::drop(self.ch375_index as usize);
                }
            }
        }
        // self.device_handle.reset().unwrap();
    }
}

impl Transport for UsbTransport {
    fn send_raw(&mut self, raw: &[u8]) -> Result<()> {
        if let Some(ref mut handle) = self.device_handle {
            handle.write_bulk(ENDPOINT_OUT, raw, Duration::from_millis(USB_TIMEOUT_MS))?;
        } else {
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            {
                if self.ch375_index >= 0 {
                    //log::debug!("CH375USBDevice index {} send_raw {:?}", self.ch375_index, raw);
                    ch375_driver::write_raw(self.ch375_index as usize, raw)?;
                    return Ok(());
                }
            }
            anyhow::bail!("USB device handle is None while ch375_index is negative or not set");
        }
        Ok(())
    }

    fn recv_raw(&mut self, timeout: Duration) -> Result<Vec<u8>> {
        let mut buf = [0u8; 64];
        if let Some(ref mut handle) = self.device_handle {
            let nread = handle.read_bulk(ENDPOINT_IN, &mut buf, timeout)?;
            return Ok(buf[..nread].to_vec());
        } else {
            #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
            {
                if self.ch375_index >= 0 {
                    let len = ch375_driver::read_raw(self.ch375_index as usize, &mut buf)?;
                    // log::debug!("CH375USBDevice index {} , len {} recv_raw {:?}", self.ch375_index, len, buf);
                    return Ok(buf[..len].to_vec());
                }
            }
        }
        anyhow::bail!("USB device handle is None while ch375_index is negative or not set");
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub mod ch375_driver {
    use libloading::os::windows::*;
    use std::sync::OnceLock;

    use super::*;

    static CH375_DRIVER: OnceLock<Library> = OnceLock::new();

    fn ensure_library_load() -> Result<&'static Library> {
        CH375_DRIVER.get_or_try_init(|| {
            let lib = Library::new("CH375DLL64.dll")
                .map_err(|_| {
                    anyhow::Error::msg(
                        "CH375DLL64.dll not found. \
                        Please download it from the WCH official website \
                        and put the dll next to this executable. \
                        You may download it from https://www.wch-ic.com/downloads/CH372DRV_ZIP.html, \
                        or search for 'CH375' in the WCH websites if the link is broken."
                    )
                })?;
            let get_version: Symbol<unsafe extern "stdcall" fn() -> u32> =
                { lib.get(b"CH375GetVersion").unwrap() };
            let get_driver_version: Symbol<unsafe extern "stdcall" fn() -> u32> =
                { lib.get(b"CH375GetDrvVersion").unwrap() };

            log::debug!(
                "DLL version {}, driver version {}",
                get_version(),
                get_driver_version()
            );
            Ok(lib)
        })
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

        let open_device: Symbol<unsafe extern "stdcall" fn(u32) -> u32> =
            unsafe { lib.get(b"CH375OpenDevice").unwrap() };
        let close_device: Symbol<unsafe extern "stdcall" fn(u32)> =
            unsafe { lib.get(b"CH375CloseDevice").unwrap() };
        let get_device_descriptor: Symbol<
            unsafe extern "stdcall" fn(u32, *mut UsbDeviceDescriptor, *mut u32) -> bool,
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
        let open_device: Symbol<unsafe extern "stdcall" fn(u32) -> u32> =
            unsafe { lib.get(b"CH375OpenDevice").unwrap() };
        let close_device: Symbol<unsafe extern "stdcall" fn(u32)> =
            unsafe { lib.get(b"CH375CloseDevice").unwrap() };
        let get_device_descriptor: Symbol<
            unsafe extern "stdcall" fn(u32, *mut UsbDeviceDescriptor, *mut u32) -> bool,
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

        return Ok(-1 as isize);
    }

    pub fn write_raw(nth: usize, buf: &[u8]) -> Result<()> {
        let lib = ensure_library_load()?;
        let write_data: Symbol<unsafe extern "stdcall" fn(u32, *mut u8, *mut u32) -> bool> =
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
        let read_data: Symbol<unsafe extern "stdcall" fn(u32, *mut u8, *mut u32) -> bool> =
            unsafe { lib.get(b"CH375ReadData").unwrap() };

        let mut len = buf.len() as u32;
        let ret = unsafe { read_data(nth as u32, buf.as_mut_ptr(), &mut len) };
        if ret {
            Ok(len as usize)
        } else {
            Err(anyhow::anyhow!("Failed to read data with CH375USBDevice"))
        }
    }

    pub fn set_timeout(nth: usize, timeout: Duration) {
        let lib = ensure_library_load().unwrap();

        let set_timeout_ex: Symbol<unsafe extern "stdcall" fn(u32, u32, u32, u32, u32) -> bool> =
            unsafe { lib.get(b"CH375SetTimeoutEx").unwrap() };

        let ds = timeout.as_millis() as u32;

        unsafe {
            set_timeout_ex(nth as u32, ds, ds, ds, ds);
        }
    }

    pub fn drop(nth: usize) {
        let lib = ensure_library_load().unwrap();
        let close_device: Symbol<unsafe extern "stdcall" fn(u32)> =
            unsafe { lib.get(b"CH375CloseDevice").unwrap() };
        unsafe {
            close_device(nth as u32);
        }
    }
}
