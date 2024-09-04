//! Serial Transportation.
use std::{fmt::Display, io::Read, time::Duration};

use anyhow::{Error, Ok, Result};
use clap::{builder::PossibleValue, ValueEnum};
use serialport::SerialPort;

use super::Transport;

const SERIAL_TIMEOUT_MS: u64 = 1000;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Baudrate {
    #[default]
    Baud115200,
    Baud1m,
    Baud2m,
}

impl From<Baudrate> for u32 {
    fn from(value: Baudrate) -> Self {
        match value {
            Baudrate::Baud115200 => 115200,
            Baudrate::Baud1m => 1000000,
            Baudrate::Baud2m => 2000000,
        }
    }
}

impl Display for Baudrate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", u32::from(*self))
    }
}

impl ValueEnum for Baudrate {
    fn value_variants<'a>() -> &'a [Self] {
        &[Baudrate::Baud115200, Baudrate::Baud1m, Baudrate::Baud2m]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        match self {
            Baudrate::Baud115200 => Some(PossibleValue::new("Baud115200").aliases(["115200"])),
            Baudrate::Baud1m => {
                Some(PossibleValue::new("Baud1m").aliases(["1000000", "1_000_000", "1m"]))
            }
            Baudrate::Baud2m => {
                Some(PossibleValue::new("Baud2m").aliases(["2000000", "2_000_000", "2m"]))
            }
        }
    }
}

pub struct SerialTransport {
    serial_port: Box<dyn SerialPort>,
}

impl SerialTransport {
    pub fn scan_ports() -> Result<Vec<String>> {
        let ports = serialport::available_ports()?;
        Ok(ports.into_iter().map(|p| p.port_name).collect())
    }

    pub fn open(port: &str) -> Result<Self> {
        log::info!("Opening serial port: \"{}\" @ 115200 baud", port);
        let port = serialport::new(port, 115200)
            .timeout(Duration::from_millis(SERIAL_TIMEOUT_MS))
            .open()?;
        Ok(SerialTransport { serial_port: port })
    }

    pub fn open_nth(nth: usize) -> Result<Self> {
        let ports = serialport::available_ports()?;

        match ports.get(nth) {
            Some(port) => Self::open(&port.port_name),
            None => Err(Error::msg("No serial ports found!")),
        }
    }

    pub fn open_any() -> Result<Self> {
        Self::open_nth(0)
    }

    pub fn set_baudrate(&mut self, baudrate: impl Into<u32>) -> Result<()> {
        self.serial_port.set_baud_rate(baudrate.into())?;
        Ok(())
    }
}

impl Transport for SerialTransport {
    fn send_raw(&mut self, raw: &[u8]) -> Result<()> {
        let mut v = Vec::new();

        v.extend_from_slice(&[0x57, 0xab]); // Append request prefix
        v.extend_from_slice(raw);
        v.extend_from_slice(&[raw.iter().fold(0u8, |acc, &val| acc.wrapping_add(val))]); // Append the CRC

        self.serial_port.write_all(&v)?;
        self.serial_port.flush()?;
        Ok(())
    }

    fn recv_raw(&mut self, _timeout: Duration) -> Result<Vec<u8>> {
        // Ignore the custom timeout
        // self.serial_port.set_timeout(timeout)?;

        // Read the message header
        let mut header_buf = [0u8; 6];
        self.serial_port.read_exact(&mut header_buf)?;
        // Read the amount of data given in the header + the CRC
        let mut data_buf = vec![0u8; (header_buf[4] + 1) as usize];
        self.serial_port.read_exact(&mut data_buf)?;

        // Note: We strip the prefix & CRC, could we check the CRC for errors?
        let mut buf_vec = Vec::new();
        buf_vec.extend_from_slice(&header_buf[2..]);
        buf_vec.extend_from_slice(&data_buf[..data_buf.len() - 1]);
        Ok(buf_vec)
    }
}
