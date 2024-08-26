//! Serial Transportation.
use std::{io::Read, thread::sleep, time::Duration};

use anyhow::{Error, Ok, Result};
use serialport::SerialPort;

use super::Transport;

const SERIAL_TIMEOUT_MS: u64 = 5000;

pub struct SerialTransport {
    serial_port: Box<dyn SerialPort>,
}

impl SerialTransport {
    pub fn scan_ports() -> Result<Vec<String>> {
        let ports = serialport::available_ports()?;
        Ok(ports.into_iter().map(|p| p.port_name).collect())
    }

    pub fn open(port: &str) -> Result<Self> {
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
}

impl Transport for SerialTransport {
    fn send_raw(&mut self, raw: &[u8]) -> Result<()> {
        let mut v = Vec::new();

        v.extend_from_slice(&[0x57, 0xab]); // Append request prefix
        v.extend_from_slice(raw);
        v.extend_from_slice(&[raw.iter().sum()]); // Append the CRC

        self.serial_port.write_all(&v)?;
        self.serial_port.flush()?;
        Ok(())
    }

    fn recv_raw(&mut self, timeout: Duration) -> Result<Vec<u8>> {
        self.serial_port.set_timeout(timeout)?;

        // Note: Delay needed for the message to arrive on Rx
        sleep(Duration::from_millis(50));

        let mut buf = [0u8; 64 + 3]; // Note: 64 bytes + prefix & CRC
        let nread = self.serial_port.read(&mut buf)?;

        // Note: We strip the prefix & CRC, could we check the CRC for errors?
        let mut buf_vec = Vec::new();
        if let Some(data) = buf.get(2..nread - 1) {
            buf_vec.extend_from_slice(data);
        }
        Ok(buf_vec)
    }
}
