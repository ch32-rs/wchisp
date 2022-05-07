//! The underlying binary protocol of WCH ISP

use std::fmt;

use anyhow::Result;
use scroll::{Pread, Pwrite};

use crate::constants::commands;

/// WCH ISP Command
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Command {
    /// Identify the MCU.
    /// Return the real `device_id`, `device_type`.
    ///
    /// DeviceType = ChipSeries = SerialNumber = McuType + 0x10
    Identify { device_id: u8, device_type: u8 },
    /// End ISP session, reboot the device.
    ///
    /// Connection will lost after response packet
    IspEnd {
        reason: u8, // 0 for normal, 1 for config set
    },
    /// Send ISP key seed to MCU.
    /// Return checksum of the XOR key(1 byte sum).
    ///
    /// The detailedd key algrithm:
    ///
    /// - sum Device UID to a byte, s
    /// - initialize XOR key as [s; 8]
    /// - select 7 bytes(via some rules) from generated random key
    /// - xor with key[0] to [6]
    /// - xor key[7] with key[0]
    ///
    /// In many open source implementations, the key is initialized as [0; N],
    /// which makes it easier to do the calculation
    IspKey { key: Vec<u8> },
    /// Erase the Code Flash.
    ///
    /// Minmum sectors is either 8 or 4 depends on device type.
    Erase { sectors: u32 },
    /// Program the Code Flash.
    ///
    /// `data` is xored with the XOR key.
    /// `padding` is a random byte(Looks like a checksum, but it's not)
    Program {
        address: u32,
        padding: u8,
        data: Vec<u8>,
    },
    /// Verify the Code Flash, almost the same as `Program`
    Verify {
        address: u32,
        padding: u8,
        data: Vec<u8>,
    },
    /// Read Config Bits.
    ReadConfig { bit_mask: u8 },
    /// Write Config Bits. Can be used to unprotect the device.
    WriteConfig { bit_mask: u8, data: Vec<u8> },
    /// Erase the Data Flash, almost the same as `Erase`
    DataErase { sectors: u32 },
    /// Program the Data Flash, almost the same as `Program`
    DataProgram { address: u32, data: Vec<u8> },
    /// Read the Data Flash
    DataRead { address: u32, len: u16 },
    /// Write OTP
    WriteOTP(u8),
    /// Read OTP
    ReadOTP(u8),
    /// Set baudrate
    SetBaud { baudrate: u32 },
}

impl Command {
    pub fn identify(device_id: u8, device_type: u8) -> Self {
        Command::Identify {
            device_id,
            device_type,
        }
    }

    pub fn isp_end(reason: u8) -> Self {
        Command::IspEnd {
            reason,
        }
    }

    pub fn read_config(bit_mask: u8) -> Self {
        Command::ReadConfig { bit_mask }
    }

    pub fn write_config(bit_mask: u8, data: Vec<u8>) -> Self {
        Command::WriteConfig { bit_mask, data }
    }

    // TODO(visiblity)
    pub fn into_raw(self) -> Result<Vec<u8>> {
        match self {
            Command::Identify {
                device_id,
                device_type,
            } => {
                let mut buf = Vec::with_capacity(0x12 + 3);
                buf.push(commands::IDENTIFY);
                buf.extend_from_slice(&[0x12, 0]);
                buf.push(device_id);
                buf.push(device_type);
                buf.extend_from_slice(b"MCU ISP & WCH.CN");
                Ok(buf)
            }
            Command::IspEnd { reason } => Ok([commands::ISP_END, 0x01, 00, reason].to_vec()),
            Command::IspKey { key } => {
                let mut buf = Vec::with_capacity(3 + key.len());
                buf.push(commands::ISP_KEY);
                buf.push(key.len() as u8);
                buf.push(0x00);
                buf.extend(key);
                Ok(buf)
            }
            Command::Erase { sectors } => {
                let mut buf = [commands::ERASE, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00];
                buf.pwrite_with(sectors, 3, scroll::LE)?;
                Ok(buf.to_vec())
            }
            Command::Program {
                address,
                padding,
                data,
            } => {
                // CMD, SIZE, ADDR, PADDING, DATA
                let mut buf = vec![0u8; 1 + 2 + 4 + 1 + data.len()];
                buf[0] = commands::PROGRAM;
                buf.pwrite_with(1 + data.len() as u16, 1, scroll::LE)?;
                buf.pwrite_with(address, 3, scroll::LE)?;
                buf[6] = padding;
                buf[7..].copy_from_slice(&data);
                Ok(buf)
            }
            Command::Verify {
                address,
                padding,
                data,
            } => {
                let mut buf = vec![0u8; 1 + 2 + 4 + 1 + data.len()];
                buf[0] = commands::VERIFY;
                buf.pwrite_with(1 + data.len() as u16, 1, scroll::LE)?;
                buf.pwrite_with(address, 3, scroll::LE)?;
                buf[6] = padding;
                buf[7..].copy_from_slice(&data);
                Ok(buf)
            }
            Command::ReadConfig { bit_mask } => {
                let buf = [commands::READ_CONFIG, 0x02, 0x00, bit_mask, 0x00];
                Ok(buf.to_vec())
            }
            Command::WriteConfig { bit_mask, data } => {
                let mut buf = vec![0u8; 1 + 2 + 2 + data.len()];
                buf[0] = commands::WRITE_CONFIG;
                buf.pwrite_with(1 + data.len() as u16, 1, scroll::LE)?;
                buf[3] = bit_mask;
                buf[5..].copy_from_slice(&data);
                Ok(buf)
            }

            _ => unimplemented!(),
        }
    }
}

/// Response to a Command. The request cmd type is ommitted from the type definition.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Response {
    /// Code = 0x00
    Ok(Vec<u8>),
    /// Otherwise
    Err(u8, Vec<u8>),
}

impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Response::Ok(data) => write!(f, "OK[{}]", hex::encode(data)),
            Response::Err(code, data) => write!(f, "ERROR({:x})[{}]", code, hex::encode(data)),
        }
    }
}

impl Response {
    pub fn is_ok(&self) -> bool {
        match self {
            Response::Ok(_) => true,
            _ => false,
        }
    }

    pub fn payload(&self) -> &[u8] {
        match self {
            Response::Ok(payload) => payload,
            Response::Err(_, payload) => payload,
        }
    }

    pub(crate) fn from_raw(raw: &[u8]) -> Result<Self> {
        if raw[1] == 0x00 {
            let len = raw.pread_with::<u16>(2, scroll::LE)? as usize;
            let remain = &raw[4..];
            if remain.len() == len {
                Ok(Response::Ok(remain.to_vec()))
            } else {
                Err(anyhow::anyhow!("Invalid response length"))
            }
        } else {
            Ok(Response::Err(raw[1], raw[2..].to_vec()))
        }
    }
}
