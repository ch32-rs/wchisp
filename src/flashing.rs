//! Chip flashing logic.
use std::thread::sleep;
use std::time::Duration;

use anyhow::Result;
use scroll::{Pread, LE};

use crate::{
    constants::{CFG_MASK_RDPR_USER_DATA_WPR, SECTOR_SIZE, CFG_MASK_ALL},
    transport::UsbTransport,
    Chip, Command, Transport,
};

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

        let read_conf = Command::read_config(CFG_MASK_ALL);
        let resp = transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        log::debug!("read_config: {}", hex::encode(&resp.payload()));
        let code_flash_protected = chip.support_code_flash_protect() && resp.payload()[2] != 0xa5;
        let mut btver = [0u8; 4];
        btver.copy_from_slice(&resp.payload()[14..18]);

        if chip.support_code_flash_protect()
            && resp.payload()[2 + 8..2 + 8 + 4] != [0xff, 0xff, 0xff, 0xff]
        {
            log::warn!(
                "WRP register: {}",
                hex::encode(&resp.payload()[2 + 8..2 + 8 + 4])
            );
        }

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
            "Chip: {} (Code Flash: {}KiB, Data EEPROM: {}KiB)",
            self.chip,
            self.chip.max_code_flash_size / 1024,
            self.chip.max_data_flash_size / 1024
        );
        log::info!("Chip UID: {}", hex::encode(&self.chip_uid));
        log::info!(
            "BTVER(bootloader ver): {}",
            hex::encode(&self.bootloader_version[1..])
        );

        log::info!("Code Flash protected: {}", self.code_flash_protected);
        Ok(())
    }

    pub fn unprotect(&mut self, force: bool) -> Result<()> {
        if !force && !self.code_flash_protected {
            return Ok(());
        }
        let read_conf = Command::read_config(CFG_MASK_RDPR_USER_DATA_WPR);
        let resp = self.transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        let mut config = resp.payload()[2..14].to_vec(); // 4 x u32
        config[0] = 0xa5; // code flash unprotected
        config[1] = 0x5a;

        // WPR register
        config[8..12].copy_from_slice(&[0xff; 4]);

        let write_conf = Command::write_config(CFG_MASK_RDPR_USER_DATA_WPR, config);
        let resp = self.transport.transfer(write_conf)?;
        anyhow::ensure!(resp.is_ok(), "write_config failed");

        log::info!("Code Flash unprotected");
        self.reset()?;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<()> {
        let isp_end = Command::isp_end(1);
        let resp = self.transport.transfer(isp_end)?;
        anyhow::ensure!(resp.is_ok(), "isp_end failed");

        log::info!("Device reset");
        Ok(())
    }

    // unprotect -> erase -> flash -> verify -> reset
    pub fn flash(&mut self, raw: &[u8]) -> Result<()> {
        let sectors = raw.len() / SECTOR_SIZE;
        self.erase_code(sectors as u32)?;

        sleep(Duration::from_secs(1));

        let key = self.xor_key();
        let key_checksum = key.iter().fold(0_u8, |acc, &x| acc.overflowing_add(x).0);

        // NOTE: use all-zero key seed for now.
        let isp_key = Command::isp_key(vec![0; 0x1e]);
        let resp = self.transport.transfer(isp_key)?;
        anyhow::ensure!(resp.is_ok(), "isp_key failed");
        anyhow::ensure!(resp.payload()[0] == key_checksum, "isp_key checksum failed");

        const CHUNK: usize = 56;
        let mut address = 0x0;
        for ch in raw.chunks(CHUNK) {
            self.flash_chunk(address, ch, key)?;
            address += ch.len() as u32;
        }
        // NOTE: require a write action of empty data for success flashing
        self.flash_chunk(address, &[], key)?;
        log::info!("Code flash {} bytes written", address);

        Ok(())
    }

    pub fn verify(&mut self, raw: &[u8]) -> Result<()> {
        sleep(Duration::from_secs(1));

        let key = self.xor_key();
        let key_checksum = key.iter().fold(0_u8, |acc, &x| acc.overflowing_add(x).0);

        // NOTE: use all-zero key seed for now.
        let isp_key = Command::isp_key(vec![0; 0x1e]);
        let resp = self.transport.transfer(isp_key)?;
        anyhow::ensure!(resp.is_ok(), "isp_key failed");
        anyhow::ensure!(resp.payload()[0] == key_checksum, "isp_key checksum failed");

        const CHUNK: usize = 56;
        let mut address = 0x0;
        for ch in raw.chunks(CHUNK) {
            self.flash_chunk(address, ch, key)?;
            address += ch.len() as u32;
        }
        // NOTE: require a write action of empty data for success flashing
        self.verify_chunk(address, &[], key)?;

        Ok(())
    }

    fn flash_chunk(&mut self, address: u32, raw: &[u8], key: [u8; 8]) -> Result<()> {
        let xored = raw.iter().enumerate().map(|(i, x)| x ^ key[i % 8]);
        let padding = rand::random();
        let cmd = Command::program(address, padding, xored.collect());
        let resp = self.transport.transfer(cmd)?;
        anyhow::ensure!(resp.is_ok(), "program 0x{:08x} failed", address);
        Ok(())
    }

    fn verify_chunk(&mut self, address: u32, raw: &[u8], key: [u8; 8]) -> Result<()> {
        let xored = raw.iter().enumerate().map(|(i, x)| x ^ key[i % 8]);
        let padding = rand::random();
        let cmd = Command::verify(address, padding, xored.collect());
        let resp = self.transport.transfer(cmd)?;
        anyhow::ensure!(resp.is_ok(), "verify 0x{:08x} failed", address);
        Ok(())
    }

    pub fn erase_code(&mut self, mut sectors: u32) -> Result<()> {
        let min_sectors = self.chip.min_erase_sector_number();
        if sectors < min_sectors {
            sectors = min_sectors;
            log::warn!(
                "erase_code: set min number of erased sectors to {}",
                sectors
            );
        }
        let erase = Command::erase(sectors);
        let resp = self.transport.transfer(erase)?;
        anyhow::ensure!(resp.is_ok(), "erase failed");

        log::info!("Erased {} code flash sectors", sectors);
        Ok(())
    }

    pub fn erase_data(&mut self, _sectors: u16) -> Result<()> {
        if self.chip.max_data_flash_size == 0 {
            anyhow::bail!("chip doesn't support data flash");
        }
        unimplemented!("TODO")
    }

    // NOTE: XOR key for all-zero key seed
    fn xor_key(&self) -> [u8; 8] {
        let checksum = self
            .chip_uid
            .iter()
            .fold(0_u8, |acc, &x| acc.overflowing_add(x).0);
        let mut key = [checksum; 8];
        key.last_mut()
            .map(|x| *x = x.overflowing_add(self.chip.chip_id).0);
        key
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
