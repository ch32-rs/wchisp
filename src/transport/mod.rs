use anyhow::Result;

use crate::protocol::{Command, Response};

pub use self::usb::UsbTransport;

mod usb;

/// Abstraction of the transport layer.
/// Might be a USB, a serial port, or Network.
pub trait Transport {
    fn send_raw(&mut self, raw: &[u8]) -> Result<()>;
    fn recv_raw(&mut self) -> Result<Vec<u8>>;

    fn transfer(&mut self, cmd: Command) -> Result<Response> {
        let raw = &cmd.into_raw()?;
        log::debug!("=> {}", hex::encode(&raw));
        self.send_raw(&raw)?;

        let raw = self.recv_raw()?;
        log::debug!("<= {}", hex::encode(&raw));
        Response::from_raw(&raw)
    }
}

/// A transport which can
pub trait MultiTransport: Transport {}
