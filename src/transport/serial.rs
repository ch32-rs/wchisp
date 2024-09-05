//! Serial Transportation.
use std::{fmt::Display, io::Read, time::Duration};

use anyhow::{Error, Ok, Result};
use clap::{builder::PossibleValue, ValueEnum};
use scroll::Pread;
use serialport::SerialPort;

use super::{Command, Transport};

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

    pub fn open(port: &str, baudrate: Baudrate) -> Result<Self> {
        log::info!("Opening serial port: \"{}\" @ 115200 baud", port);
        let port = serialport::new(port, Baudrate::default().into())
            .timeout(Duration::from_millis(SERIAL_TIMEOUT_MS))
            .open()?;

        let mut transport = SerialTransport { serial_port: port };
        transport.set_baudrate(baudrate)?;

        Ok(transport)
    }

    pub fn open_nth(nth: usize, baudrate: Baudrate) -> Result<Self> {
        let ports = serialport::available_ports()?;

        match ports.get(nth) {
            Some(port) => Self::open(&port.port_name, baudrate),
            None => Err(Error::msg("No serial ports found!")),
        }
    }

    pub fn open_any(baudrate: Baudrate) -> Result<Self> {
        Self::open_nth(0, baudrate)
    }

    pub fn set_baudrate(&mut self, baudrate: impl Into<u32>) -> Result<()> {
        let baudrate: u32 = baudrate.into();

        if baudrate != self.serial_port.baud_rate()? {
            let resp: crate::Response = self.transfer(Command::set_baud(baudrate))?;
            anyhow::ensure!(resp.is_ok(), "set baudrate failed");

            if let Some(0xfe) = resp.payload().first() {
                log::info!("Custom baudrate not supported by the current chip. Using 115200");
            } else {
                log::info!("Switching baudrate to: {baudrate} baud");
                self.serial_port.set_baud_rate(baudrate.into())?;
            }
        }

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

        // Read the serial header and validate.
        let mut head_buf = [0u8; 2];
        self.serial_port.read_exact(&mut head_buf)?;
        anyhow::ensure!(
            head_buf == [0x55, 0xaa],
            "Response has invalid serial header {head_buf:02x?}",
        );

        // Read the payload header and extract given length value.
        let mut payload_head_buf = [0u8; 4];
        self.serial_port.read_exact(&mut payload_head_buf)?;
        let payload_data_len = payload_head_buf.pread_with::<u16>(2, scroll::LE)? as usize;
        anyhow::ensure!(payload_data_len > 0, "Response data length is zero");

        // Read the amount of payload data given in the header.
        let mut payload_data_buf = vec![0u8; payload_data_len];
        self.serial_port.read_exact(&mut payload_data_buf)?;

        // Read the checksum and verify against actual sum calculated from
        // entire payload (header + data).
        let mut cksum_buf = [0u8; 1];
        self.serial_port.read_exact(&mut cksum_buf)?;

        // Stuff the payload header and data into response to be returned.
        let resp_vec: Vec<u8> = payload_head_buf
            .into_iter()
            .chain(payload_data_buf.into_iter())
            .collect();

        // Read the checksum and verify against actual sum calculated from
        // entire payload (header + data).
        let checksum = resp_vec.iter().fold(0u8, |acc, &val| acc.wrapping_add(val));
        anyhow::ensure!(
            checksum == cksum_buf[0],
            "Response has incorrect checksum ({:02x} != {:02x})",
            cksum_buf[0],
            checksum
        );

        Ok(resp_vec)
    }
}

impl Drop for SerialTransport {
    fn drop(&mut self) {
        let _ = self.set_baudrate(Baudrate::Baud115200);
    }
}
