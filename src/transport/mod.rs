//! Abstract Device transport interface.
use std::time::Duration;

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
        self.transfer_with_wait(cmd, Duration::default())
    }

    fn transfer_with_wait(&mut self, cmd: Command, wait: Duration) -> Result<Response> {
        let req = &cmd.into_raw()?;
        log::debug!("=> {}   {}", hex::encode(&req[..3]), hex::encode(&req[3..]));
        self.send_raw(&req)?;

        std::thread::sleep(wait);

        let resp = self.recv_raw()?;
        anyhow::ensure!(req[0] == resp[0], "response command type mismatch");
        log::debug!("<= {} {}", hex::encode(&resp[..4]), hex::encode(&resp[4..]));
        Response::from_raw(&resp)
    }
}
