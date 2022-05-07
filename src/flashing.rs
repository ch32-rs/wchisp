//! Chip flashing logic.

use anyhow::Result;
use scroll::{Pread, LE};

use crate::{transport::UsbTransport, Chip, Command, Transport};

pub struct Flashing<T: Transport> {
    transport: T,
    chip: Chip,
    /// Chip unique identifier
    chip_uid: Vec<u8>,
    // BTVER
    bootloader_version: [u8; 4],
    code_flash_protected: bool,
}

impl Flashing<UsbTransport> {
    pub fn new_from_usb() -> Result<Self> {
        let mut transport = UsbTransport::open_any()?;

        let identify = Command::identify(0, 0);
        let resp = transport.transfer(identify)?;
        anyhow::ensure!(resp.is_ok(), "idenfity chip failed");
        let chip = Chip::guess(resp.payload()[0], resp.payload()[1])?;
        log::debug!("found chip: {}", chip);

        let read_conf = Command::read_config(0x1f);
        let resp = transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        log::debug!("read_config: {}", hex::encode(&resp.payload()));
        let code_flash_protected = chip.support_code_flash_protect() && resp.payload()[2] != 0xa5;
        let mut btver = [0u8; 4];
        btver.copy_from_slice(&resp.payload()[14..18]);

        // NOTE: just read all remain bytes as chip_uid
        let chip_uid = resp.payload()[18..].to_vec();

        let f = Flashing {
            transport,
            chip,
            chip_uid,
            bootloader_version: btver,
            code_flash_protected,
        };
        f.check_chip_uid()?;
        Ok(f)
    }
}

impl<T: Transport> Flashing<T> {
    pub fn dump_info(&self) -> Result<()> {
        log::info!(
            "Chip: {} (CodeFlash: {}KiB, EEPROM: {}KiB)",
            self.chip,
            self.chip.max_code_flash_size / 1024,
            self.chip.max_data_flash_size / 1024
        );
        log::info!("Chip UID: {}", hex::encode(&self.chip_uid));
        log::info!(
            "BTVER(bootloader version): {}",
            hex::encode(&self.bootloader_version[1..])
        );
        log::info!("Code Flash Protected: {}", self.code_flash_protected);
        Ok(())
    }

    // TODO: handle WRP
    pub fn unprotect(&mut self) -> Result<()> {
        if !self.code_flash_protected {
            return Ok(());
        }
        let read_conf = Command::read_config(0x1f);
        let resp = self.transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        let mut config = resp.payload()[2..14].to_vec(); // 4 x u32
        config[0] = 0xa5; // code flash unprotected
        config[1] = 0x5a;

        let write_conf = Command::write_config(0x1f, config);
        let resp = self.transport.transfer(write_conf)?;
        anyhow::ensure!(resp.is_ok(), "write_config failed");

        log::info!("Code Flash Unprotected");
        self.reset()?;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<()> {
        let isp_end = Command::isp_end(0);
        let resp = self.transport.transfer(isp_end)?;
        anyhow::ensure!(resp.is_ok(), "isp_end failed");

        log::info!("Device reset");
        Ok(())
    }

    pub fn flash(&mut self, _raw: &[u8]) -> Result<()> {
        unimplemented!()
    }

    pub fn erase_code(&mut self, mut sectors: u32) -> Result<()> {
        if sectors < self.chip.min_erase_sector_size() {
            sectors = self.chip.min_erase_sector_size();
            log::warn!("erase_code: too small sector size, set to {}", sectors);
        }
        let erase = Command::erase(sectors);
        let resp = self.transport.transfer(erase)?;
        anyhow::ensure!(resp.is_ok(), "erase failed");

        log::info!("Code Flash Erased");
        Ok(())
    }

    pub fn erase_data(&mut self, _sectors: u16) -> Result<()> {
        if self.chip.max_data_flash_size == 0 {
            anyhow::bail!("chip doesn't support data flash");
        }
        unimplemented!()
    }

    pub fn chip_uid(&self) -> &[u8] {
        &self.chip_uid[..self.chip.uid_size()]
    }

    fn check_chip_uid(&self) -> Result<()> {
        if self.chip.uid_size() == 8 {
            let raw = self.chip_uid();
            let checked = raw
                .pread_with::<u16>(0, LE)?
                .overflowing_add(raw.pread_with::<u16>(2, LE)?)
                .0
                .overflowing_add(raw.pread_with::<u16>(4, LE)?)
                .0
                == raw.pread_with::<u16>(6, LE)?;
            anyhow::ensure!(checked, "Chip UID checksum failed!");
        }
        Ok(())
    }
}
