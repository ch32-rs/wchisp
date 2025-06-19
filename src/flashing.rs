//! Chip flashing routine
use std::time::Duration;

use anyhow::{Ok, Result};
use indicatif::ProgressBar;
use scroll::{Pread, Pwrite, LE};

use crate::{
    constants::{CFG_MASK_ALL, CFG_MASK_RDPR_USER_DATA_WPR},
    device::{parse_number, ChipDB},
    transport::{SerialTransport, UsbTransport},
    Baudrate, Chip, Command, Transport,
};

pub struct Flashing<'a> {
    transport: Box<dyn Transport + 'a>,
    pub chip: Chip,
    /// Chip unique identifier
    chip_uid: Vec<u8>,
    // BTVER
    bootloader_version: [u8; 4],
    code_flash_protected: bool,
}

impl<'a> Flashing<'a> {
    pub fn get_chip(transport: &mut impl Transport) -> Result<Chip> {
        let identify = Command::identify(0, 0);
        let resp = transport.transfer(identify)?;

        let chip_db = ChipDB::load()?;
        let chip = chip_db.find_chip(resp.payload()[0], resp.payload()[1])?;

        Ok(chip)
    }

    pub fn new_from_transport(mut transport: impl Transport + 'a) -> Result<Self> {
        let identify = Command::identify(0, 0);
        let resp = transport.transfer(identify)?;
        anyhow::ensure!(resp.is_ok(), "idenfity chip failed");

        let chip = Flashing::get_chip(&mut transport)?;
        log::debug!("found chip: {}", chip);

        let read_conf = Command::read_config(CFG_MASK_ALL);
        let resp = transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        log::debug!("read_config: {}", hex::encode(&resp.payload()[2..]));
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
            transport: Box::new(transport),
            chip,
            chip_uid,
            bootloader_version: btver,
            code_flash_protected,
        };
        f.check_chip_uid()?;
        Ok(f)
    }

    pub fn new_from_serial(port: Option<&str>, baudrate: Option<Baudrate>) -> Result<Self> {
        let baudrate = baudrate.unwrap_or_default();

        let transport = match port {
            Some(port) => SerialTransport::open(port, baudrate)?,
            None => SerialTransport::open_any(baudrate)?,
        };

        Self::new_from_transport(transport)
    }

    pub fn new_from_usb(device: Option<usize>) -> Result<Self> {
        let transport = match device {
            Some(device) => UsbTransport::open_nth(device)?,
            None => UsbTransport::open_any()?,
        };

        Self::new_from_transport(transport)
    }

    /// Reidentify chip using correct chip uid
    pub fn reidenfity(&mut self) -> Result<()> {
        let identify = Command::identify(self.chip.chip_id, self.chip.device_type);
        let resp = self.transport.transfer(identify)?;

        anyhow::ensure!(resp.payload()[0] == self.chip.chip_id, "chip id mismatch");
        anyhow::ensure!(
            resp.payload()[1] == self.chip.device_type,
            "device type mismatch"
        );

        let read_conf = Command::read_config(CFG_MASK_ALL);
        let _ = self.transport.transfer(read_conf)?;

        Ok(())
    }

    pub fn check_chip_name(&self, name: &str) -> Result<()> {
        if !self.chip.name.starts_with(name) {
            anyhow::bail!(
                "chip name mismatch: has {}, provided {}",
                self.chip.name,
                name
            );
        }
        Ok(())
    }

    pub fn dump_info(&mut self) -> Result<()> {
        if self.chip.eeprom_size > 0 {
            if self.chip.eeprom_size % 1024 != 0 {
                log::info!(
                    "Chip: {} (Code Flash: {}KiB, Data EEPROM: {} Bytes)",
                    self.chip,
                    self.chip.flash_size / 1024,
                    self.chip.eeprom_size
                );
            } else {
                log::info!(
                    "Chip: {} (Code Flash: {}KiB, Data EEPROM: {}KiB)",
                    self.chip,
                    self.chip.flash_size / 1024,
                    self.chip.eeprom_size / 1024
                );
            }
        } else {
            log::info!(
                "Chip: {} (Code Flash: {}KiB)",
                self.chip,
                self.chip.flash_size / 1024,
            );
        }
        log::info!(
            "Chip UID: {}",
            self.chip_uid
                .iter()
                .map(|x| format!("{:02X}", x))
                .collect::<Vec<_>>()
                .join("-")
        );
        log::info!(
            "BTVER(bootloader ver): {:x}{:x}.{:x}{:x}",
            self.bootloader_version[0],
            self.bootloader_version[1],
            self.bootloader_version[2],
            self.bootloader_version[3]
        );

        if self.chip.support_code_flash_protect() {
            log::info!("Code Flash protected: {}", self.code_flash_protected);
        }
        self.dump_config()?;

        Ok(())
    }

    /// Unprotect code flash.
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
    /// Program the code flash.
    pub fn flash(&mut self, raw: &[u8]) -> Result<()> {
        let key = self.xor_key();
        let key_checksum = key.iter().fold(0_u8, |acc, &x| acc.overflowing_add(x).0);

        // NOTE: use all-zero key seed for now.
        let isp_key = Command::isp_key(vec![0; 0x1e]);
        let resp = self.transport.transfer(isp_key)?;
        anyhow::ensure!(resp.is_ok(), "isp_key failed");
        anyhow::ensure!(resp.payload()[0] == key_checksum, "isp_key checksum failed");

        const CHUNK: usize = 56;
        let mut address = 0x0;

        let bar = ProgressBar::new(raw.len() as _);
        for ch in raw.chunks(CHUNK) {
            self.flash_chunk(address, ch, key)?;
            address += ch.len() as u32;
            bar.inc(ch.len() as _);
        }
        // NOTE: require a write action of empty data for success flashing
        self.flash_chunk(address, &[], key)?;
        bar.finish();

        log::info!("Code flash {} bytes written", address);

        Ok(())
    }

    pub fn write_eeprom(&mut self, raw: &[u8]) -> Result<()> {
        let key = self.xor_key();
        // let key_checksum = key.iter().fold(0_u8, |acc, &x| acc.overflowing_add(x).0);

        // NOTE: use all-zero key seed for now.
        let isp_key = Command::isp_key(vec![0; 0x1e]);
        let resp = self.transport.transfer(isp_key)?;
        anyhow::ensure!(resp.is_ok(), "isp_key failed");
        // anyhow::ensure!(resp.payload()[0] == key_checksum, "isp_key checksum failed");

        const CHUNK: usize = 56;
        let mut address = 0x0;

        let bar = ProgressBar::new(raw.len() as _);
        for ch in raw.chunks(CHUNK) {
            self.write_data_chunk(address, ch, key)?;
            address += ch.len() as u32;
            bar.inc(ch.len() as _);
        }
        // NOTE: require a write action of empty data for success flashing
        self.flash_chunk(address, &[], key)?;
        bar.finish();

        Ok(())
    }

    pub fn verify(&mut self, raw: &[u8]) -> Result<()> {
        let key = self.xor_key();
        let key_checksum = key.iter().fold(0_u8, |acc, &x| acc.overflowing_add(x).0);
        // NOTE: use all-zero key seed for now.
        let isp_key = Command::isp_key(vec![0; 0x1e]);
        let resp = self.transport.transfer(isp_key)?;
        anyhow::ensure!(resp.is_ok(), "isp_key failed");
        anyhow::ensure!(resp.payload()[0] == key_checksum, "isp_key checksum failed");

        const CHUNK: usize = 56;
        let mut address = 0x0;
        let bar = ProgressBar::new(raw.len() as _);
        for ch in raw.chunks(CHUNK) {
            self.verify_chunk(address, ch, key)?;
            address += ch.len() as u32;
            bar.inc(ch.len() as _);
        }
        bar.finish();

        Ok(())
    }

    pub fn reset_config(&mut self) -> Result<()> {
        let read_conf = Command::read_config(CFG_MASK_RDPR_USER_DATA_WPR);
        let resp = self.transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        let mut raw = resp.payload()[2..].to_vec();

        log::info!("Current config registers: {}", hex::encode(&raw));

        for reg_desc in &self.chip.config_registers {
            if let Some(reset) = reg_desc.reset {
                raw.pwrite_with(reset, reg_desc.offset, scroll::LE)?;
            }
        }

        log::info!("Reset config registers:   {}", hex::encode(&raw));
        let write_conf = Command::write_config(CFG_MASK_RDPR_USER_DATA_WPR, raw);
        let resp = self.transport.transfer(write_conf)?;
        anyhow::ensure!(resp.is_ok(), "write_config failed");

        // read back
        let read_conf = Command::read_config(CFG_MASK_RDPR_USER_DATA_WPR);
        let resp = self.transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        Ok(())
    }

    pub fn enable_debug(&mut self) -> Result<()> {
        let read_conf = Command::read_config(CFG_MASK_RDPR_USER_DATA_WPR);
        let resp = self.transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        let mut raw = resp.payload()[2..].to_vec();

        log::info!("Current config registers: {}", hex::encode(&raw));

        for reg_desc in &self.chip.config_registers {
            if let Some(reset) = reg_desc.reset {
                raw.pwrite_with(reset, reg_desc.offset, scroll::LE)?;
            }
            if let Some(enable_debug) = reg_desc.enable_debug {
                raw.pwrite_with(enable_debug, reg_desc.offset, scroll::LE)?;
            }
        }

        log::info!(
            "Reset config registers to debug enabled:   {}",
            hex::encode(&raw)
        );
        let write_conf = Command::write_config(CFG_MASK_RDPR_USER_DATA_WPR, raw);
        let resp = self.transport.transfer(write_conf)?;
        anyhow::ensure!(resp.is_ok(), "write_config failed");

        // read back
        let read_conf = Command::read_config(CFG_MASK_RDPR_USER_DATA_WPR);
        let resp = self.transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        Ok(())
    }

    pub fn disable_debug(&mut self) -> Result<()> {
        let read_conf = Command::read_config(CFG_MASK_RDPR_USER_DATA_WPR);
        let resp = self.transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        let mut raw = resp.payload()[2..].to_vec();

        log::info!("Current config registers: {}", hex::encode(&raw));

        for reg_desc in &self.chip.config_registers {
            if let Some(reset) = reg_desc.reset {
                raw.pwrite_with(reset, reg_desc.offset, scroll::LE)?;
            }
            if let Some(disable_debug) = reg_desc.disable_debug {
                raw.pwrite_with(disable_debug, reg_desc.offset, scroll::LE)?;
            }
        }

        log::info!(
            "Reset config registers to debug disabled:   {}",
            hex::encode(&raw)
        );
        let write_conf = Command::write_config(CFG_MASK_RDPR_USER_DATA_WPR, raw);
        let resp = self.transport.transfer(write_conf)?;
        anyhow::ensure!(resp.is_ok(), "write_config failed");

        // read back
        let read_conf = Command::read_config(CFG_MASK_RDPR_USER_DATA_WPR);
        let resp = self.transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        Ok(())
    }

    /// Dump EEPROM, i.e. data flash.
    pub fn dump_eeprom(&mut self) -> Result<Vec<u8>> {
        const CHUNK: usize = 0x3a;

        if self.chip.eeprom_size == 0 {
            anyhow::bail!("Chip does not support EEPROM");
        }
        let bar = ProgressBar::new(self.chip.eeprom_size as _);

        let mut ret: Vec<u8> = Vec::with_capacity(self.chip.eeprom_size as _);
        let mut address = 0x0;
        while address < self.chip.eeprom_size as u32 {
            let chunk_size = u16::min(CHUNK as u16, self.chip.eeprom_size as u16 - address as u16);

            let cmd = Command::data_read(address, chunk_size);
            let resp = self.transport.transfer(cmd)?;
            anyhow::ensure!(resp.is_ok(), "data_read failed");

            anyhow::ensure!(
                resp.payload()[2..].len() == chunk_size as usize,
                "data_read length mismatch"
            );
            if resp.payload()[2..] == [0xfe, 0x00] {
                anyhow::bail!("EEPROM read failed, required chunk size cannot be satisfied");
            }
            ret.extend_from_slice(&resp.payload()[2..]);
            address += chunk_size as u32;

            bar.inc(chunk_size as _);
            if chunk_size < CHUNK as u16 {
                bar.finish();
                break;
            }
        }
        anyhow::ensure!(
            ret.len() == self.chip.eeprom_size as _,
            "EEPROM size mismatch, expected {}, got {}",
            self.chip.eeprom_size,
            ret.len()
        );
        Ok(ret)
    }

    fn flash_chunk(&mut self, address: u32, raw: &[u8], key: [u8; 8]) -> Result<()> {
        let xored = raw.iter().enumerate().map(|(i, x)| x ^ key[i % 8]);
        let padding = rand::random();
        let cmd = Command::program(address, padding, xored.collect());
        let resp = self
            .transport
            .transfer_with_wait(cmd, Duration::from_millis(300))?;
        anyhow::ensure!(resp.is_ok(), "program 0x{:08x} failed", address);
        Ok(())
    }

    fn write_data_chunk(&mut self, address: u32, raw: &[u8], key: [u8; 8]) -> Result<()> {
        let xored = raw.iter().enumerate().map(|(i, x)| x ^ key[i % 8]);
        let padding = rand::random();
        let cmd = Command::data_program(address, padding, xored.collect());
        // NOTE: EEPROM write might be slow. Use 5ms timeout.
        let resp = self
            .transport
            .transfer_with_wait(cmd, Duration::from_millis(5))?;
        anyhow::ensure!(resp.is_ok(), "program data 0x{:08x} failed", address);
        Ok(())
    }

    fn verify_chunk(&mut self, address: u32, raw: &[u8], key: [u8; 8]) -> Result<()> {
        let xored = raw.iter().enumerate().map(|(i, x)| x ^ key[i % 8]);
        let padding = rand::random();
        let cmd = Command::verify(address, padding, xored.collect());
        let resp = self.transport.transfer(cmd)?;
        anyhow::ensure!(resp.is_ok(), "verify response failed");
        anyhow::ensure!(resp.payload()[0] == 0x00, "Verify failed, mismatch");
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
        let resp = self
            .transport
            .transfer_with_wait(erase, Duration::from_millis(5000))?;
        anyhow::ensure!(resp.is_ok(), "erase failed");

        log::info!("Erased {} code flash sectors", sectors);
        Ok(())
    }

    pub fn erase_data(&mut self) -> Result<()> {
        if self.chip.eeprom_size == 0 {
            anyhow::bail!("chip doesn't support data EEPROM");
        }
        let sectors = (self.chip.eeprom_size / 1024).max(1) as u16;
        let erase = Command::data_erase(sectors as _);
        let resp = self
            .transport
            .transfer_with_wait(erase, Duration::from_millis(1000))?;
        anyhow::ensure!(resp.is_ok(), "erase_data failed");

        log::info!("Erased {} data flash sectors", sectors);
        Ok(())
    }

    pub fn dump_config(&mut self) -> Result<()> {
        // CH32X03x chips do not support bit mask read
        // let read_conf = Command::read_config(CFG_MASK_RDPR_USER_DATA_WPR);
        let read_conf = Command::read_config(CFG_MASK_ALL);
        let resp = self.transport.transfer(read_conf)?;
        anyhow::ensure!(resp.is_ok(), "read_config failed");

        let raw = &resp.payload()[2..];
        log::info!("Current config registers: {}", hex::encode(&raw));

        for reg_def in &self.chip.config_registers {
            let n = raw.pread_with::<u32>(reg_def.offset, LE)?;
            println!("{}: 0x{:08X}", reg_def.name, n);

            for (val, expain) in &reg_def.explaination {
                if val == "_" || Some(n) == parse_number(val) {
                    println!("  `- {}", expain);
                    break;
                }
            }

            // byte fields
            for field_def in &reg_def.fields {
                let bit_width = (field_def.bit_range[0] - field_def.bit_range[1]) as u32 + 1;
                let b = (n >> field_def.bit_range[1]) & (2_u32.pow(bit_width) - 1);
                println!(
                    "  {:<7} {} 0x{:X} (0b{:b})",
                    format!("[{:}:{:}]", field_def.bit_range[0], field_def.bit_range[1]),
                    field_def.name,
                    b,
                    b
                );
                for (val, expain) in &field_def.explaination {
                    if val == "_" || Some(b) == parse_number(val) {
                        println!("    `- {}", expain);
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    // NOTE: XOR key for all-zero key seed
    fn xor_key(&self) -> [u8; 8] {
        let checksum = self
            .chip_uid()
            .iter()
            .fold(0_u8, |acc, &x| acc.overflowing_add(x).0);
        let mut key = [checksum; 8];
        key.last_mut()
            .map(|x| *x = x.overflowing_add(self.chip.chip_id).0);
        key
    }

    pub fn chip_uid(&self) -> &[u8] {
        let uid_size = self.chip.uid_size();
        //if self.bootloader_version < [0, 2, 4, 0] {
        //    uid_size = 4
        //}
        &self.chip_uid[..uid_size]
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
