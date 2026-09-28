//! Chip flashing routine
use std::time::Duration;

use anyhow::{Ok, Result};
use indicatif::ProgressBar;
use scroll::{Pread, Pwrite, LE};

use crate::{
    ch585::{self, FlashSession},
    constants::{CFG_MASK_ALL, CFG_MASK_RDPR_USER_DATA_WPR},
    device::{parse_number, ChipDB},
    transport::{SerialTransport, UsbTransport},
    Baudrate, Chip, Command, Response, Transport,
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
        anyhow::ensure!(resp.is_ok(), "identify chip failed");

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
    pub fn reidentify(&mut self) -> Result<()> {
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
        if self.is_ch585() {
            self.ensure_operation_ok(resp, "isp_end")?;
        } else {
            anyhow::ensure!(resp.is_ok(), "isp_end failed");
        }

        log::info!("Device reset");
        Ok(())
    }

    // unprotect -> erase -> flash -> verify -> reset
    /// Program the code flash.
    pub fn flash(&mut self, raw: &[u8]) -> Result<()> {
        let key = self.begin_encrypted_session()?;
        self.flash_with_key(raw, key)
    }

    fn flash_with_key(&mut self, raw: &[u8], key: [u8; 8]) -> Result<()> {

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
        let key = self.begin_encrypted_session()?;
        self.verify_with_key(raw, key)
    }

    fn verify_with_key(&mut self, raw: &[u8], key: [u8; 8]) -> Result<()> {

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
        if self.is_ch585() {
            return self.set_ch585_debug(true);
        }

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
        if self.is_ch585() {
            return self.set_ch585_debug(false);
        }

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
        if self.is_ch585() {
            self.ensure_operation_ok(resp, &format!("program 0x{address:08x}"))?;
        }
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
        if self.is_ch585() {
            self.ensure_operation_ok(resp, &format!("verify 0x{address:08x}"))?;
        } else {
            anyhow::ensure!(resp.is_ok(), "verify response failed");
            anyhow::ensure!(resp.payload()[0] == 0x00, "Verify failed, mismatch");
        }
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
        if self.is_ch585() {
            self.ensure_operation_ok(resp, "erase")?;
        } else {
            anyhow::ensure!(resp.is_ok(), "erase failed");
        }

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

            for (val, explain) in &reg_def.explanation {
                if val == "_" || Some(n) == parse_number(val) {
                    println!("  `- {}", explain);
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
                for (val, explain) in &field_def.explanation {
                    if val == "_" || Some(b) == parse_number(val) {
                        println!("    `- {}", explain);
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Apply the CH585 BootROM programming configuration without closing the
    /// current ISP transport session.
    ///
    /// The CH585 BootROM rejects Program commands while CFG_DEBUG_EN or
    /// CFG_ROM_READ is set. This clears only those bits, writes the complete
    /// 12-byte config, and verifies the same-session readback. Retain the
    /// returned transition and restore it only after a full successful
    /// code-flash verify.
    pub fn prepare_ch585_isp_flash(&mut self) -> Result<FlashSession> {
        anyhow::ensure!(
            self.is_ch585(),
            "CH585 ISP preparation requires a CH585 target"
        );
        let transition = ch585::prepare_flash_config(self.read_ch585_config()?)?;
        self.write_and_verify_ch585_config(transition.programmed, "prepare CH585 ISP flash")?;
        // Establish the key before erase so preparation and programming remain
        // in one validated ISP session. flash() establishes it again before
        // Program, preserving the existing API behavior for direct callers.
        let key = self.begin_encrypted_session()?;
        Ok(FlashSession {
            config: transition,
            key,
        })
    }

    /// Program CH585 using the key established before erase by
    /// `prepare_ch585_isp_flash`; do not issue another ISP_KEY in between.
    pub fn flash_prepared_ch585(&mut self, raw: &[u8], session: &FlashSession) -> Result<()> {
        anyhow::ensure!(
            self.is_ch585(),
            "prepared CH585 flash requires a CH585 target"
        );
        self.flash_with_key(raw, session.key)
    }

    /// Verify CH585 using an already established standalone verify session.
    pub fn verify_prepared_ch585(&mut self, raw: &[u8], session: &FlashSession) -> Result<()> {
        anyhow::ensure!(
            self.is_ch585(),
            "prepared CH585 verify requires a CH585 target"
        );
        self.verify_with_key(raw, session.key)
    }

    /// Restore the exact CH585 configuration captured before programming.
    /// Call this only after code-flash verification succeeds.
    pub fn restore_ch585_flash_config(
        &mut self,
        session: FlashSession,
    ) -> Result<()> {
        anyhow::ensure!(
            self.is_ch585(),
            "CH585 config restoration requires a CH585 target"
        );
        anyhow::ensure!(
            self.read_ch585_config()? == session.config.programmed,
            "CH585 config changed before post-verify restoration"
        );
        if session.config.requires_restore() {
            self.write_and_verify_ch585_config(
                session.config.original,
                "restore CH585 post-verify config",
            )?;
        }
        Ok(())
    }

    fn is_ch585(&self) -> bool {
        self.chip.name == "CH585" && self.chip.chip_id == 0x85 && self.chip.device_type == 0x16
    }

    fn read_ch585_config(&mut self) -> Result<[u8; ch585::CONFIG_BYTES]> {
        let response = self
            .transport
            .transfer(Command::read_config(ch585::CONFIG_MASK))?;
        anyhow::ensure!(response.is_ok(), "CH585 read_config failed: {response:?}");
        ch585::parse_config_payload(response.payload())
    }

    fn write_and_verify_ch585_config(
        &mut self,
        config: [u8; ch585::CONFIG_BYTES],
        operation: &str,
    ) -> Result<()> {
        let response = self.transport.transfer(Command::write_config(
            ch585::CONFIG_MASK,
            config.to_vec(),
        ))?;
        self.ensure_operation_ok(response, operation)?;
        // The CH585 BootROM needs the configuration write to settle before
        // the readback and encrypted flash session are established.
        std::thread::sleep(Duration::from_millis(20));
        anyhow::ensure!(
            self.read_ch585_config()? == config,
            "{operation} readback mismatch"
        );
        Ok(())
    }

    fn set_ch585_debug(&mut self, enabled: bool) -> Result<()> {
        let requested = ch585::set_debug(self.read_ch585_config()?, enabled)?;
        self.write_and_verify_ch585_config(
            requested,
            if enabled {
                "enable CH585 debug"
            } else {
                "disable CH585 debug"
            },
        )
    }

    fn ensure_operation_ok(&self, response: Response, operation: &str) -> Result<()> {
        anyhow::ensure!(response.is_ok(), "{operation} failed: {response:?}");
        ch585::check_bootrom_status(response.payload(), operation)
    }

    fn begin_encrypted_session(&mut self) -> Result<[u8; 8]> {
        let (payload, key) = if self.is_ch585() {
            let generated = ch585::generate_isp_key(self.chip_uid(), self.chip.chip_id);
            (generated.payload, generated.xor)
        } else {
            (vec![0; 0x1e], self.xor_key())
        };
        let key_checksum = key.iter().fold(0_u8, |acc, &x| acc.overflowing_add(x).0);
        let response = self.transport.transfer(Command::isp_key(payload))?;
        anyhow::ensure!(response.is_ok(), "isp_key failed: {response:?}");
        if self.is_ch585() {
            // CH58x BootROM revisions may return either zero or the derived
            // key checksum in byte zero; byte one is the operation status.
            anyhow::ensure!(
                response.payload() == [key_checksum, 0] || response.payload() == [0, 0],
                "CH585 isp_key checksum mismatch: expected {:02x}00, got {}",
                key_checksum,
                hex::encode(response.payload())
            );
        } else {
            anyhow::ensure!(!response.payload().is_empty(), "isp_key returned no checksum");
            anyhow::ensure!(
                response.payload()[0] == key_checksum,
                "isp_key checksum failed"
            );
        }
        Ok(key)
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

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::VecDeque, rc::Rc, time::Duration};

    use super::*;
    use crate::constants::commands;

    const CH585_UID: [u8; 8] = [0x98, 0x5b, 0x29, 0x5a, 0x04, 0xdc, 0xc5, 0x91];
    const DEBUG_ENABLED: [u8; ch585::CONFIG_BYTES] = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xdf, 0x3f, 0x0f, 0x45,
    ];
    const DEBUG_DISABLED: [u8; ch585::CONFIG_BYTES] = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x4f, 0x3f, 0x0f, 0x45,
    ];
    const DEBUG_ENABLED_ROM_READ_DISABLED: [u8; ch585::CONFIG_BYTES] = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x5f, 0x3f, 0x0f, 0x45,
    ];

    struct MockTransport {
        requests: Rc<RefCell<Vec<Vec<u8>>>>,
        responses: VecDeque<Vec<u8>>,
    }

    impl Transport for MockTransport {
        fn send_raw(&mut self, raw: &[u8]) -> Result<()> {
            self.requests.borrow_mut().push(raw.to_vec());
            Ok(())
        }

        fn recv_raw(&mut self, _timeout: Duration) -> Result<Vec<u8>> {
            self.responses
                .pop_front()
                .ok_or_else(|| anyhow::format_err!("mock response queue is empty"))
        }
    }

    fn response(command: u8, payload: &[u8]) -> Vec<u8> {
        let mut raw = vec![command, 0, payload.len() as u8, 0];
        raw.extend_from_slice(payload);
        raw
    }

    fn config_response(config: [u8; ch585::CONFIG_BYTES]) -> Vec<u8> {
        let mut payload = vec![ch585::CONFIG_MASK, 0];
        payload.extend_from_slice(&config);
        response(commands::READ_CONFIG, &payload)
    }

    fn test_flashing(
        chip_id: u8,
        uid: Vec<u8>,
        responses: Vec<Vec<u8>>,
    ) -> (Flashing<'static>, Rc<RefCell<Vec<Vec<u8>>>>) {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let transport = MockTransport {
            requests: requests.clone(),
            responses: responses.into(),
        };
        let chip = ChipDB::load().unwrap().find_chip(chip_id, 0x16).unwrap();
        (
            Flashing {
                transport: Box::new(transport),
                chip,
                chip_uid: uid,
                bootloader_version: [0, 2, 3, 0],
                code_flash_protected: false,
            },
            requests,
        )
    }

    #[test]
    fn ch585_prepare_restore_and_reset_use_one_exact_session() {
        let responses = vec![
            config_response(DEBUG_ENABLED),
            response(commands::WRITE_CONFIG, &[0, 0]),
            config_response(DEBUG_DISABLED),
            response(commands::ISP_KEY, &[0, 0]),
            config_response(DEBUG_DISABLED),
            response(commands::WRITE_CONFIG, &[0, 0]),
            config_response(DEBUG_ENABLED),
            response(commands::ISP_END, &[0, 0]),
        ];
        let (mut flashing, requests) = test_flashing(0x85, CH585_UID.to_vec(), responses);

        let session = flashing.prepare_ch585_isp_flash().unwrap();
        assert_eq!(session.config.original, DEBUG_ENABLED);
        assert_eq!(session.config.programmed, DEBUG_DISABLED);
        flashing.restore_ch585_flash_config(session).unwrap();
        flashing.reset().unwrap();

        let requests = requests.borrow();
        assert_eq!(requests.len(), 8);
        assert_eq!(requests[0], Command::read_config(ch585::CONFIG_MASK).into_raw().unwrap());
        assert_eq!(requests[3][0], commands::ISP_KEY);
        assert!((0x1e..=0x3c).contains(&usize::from(requests[3][1])));
        assert!(requests[3][3..].iter().any(|byte| *byte != 0));
        assert_eq!(requests[7], Command::isp_end(1).into_raw().unwrap());
    }

    #[test]
    fn ch582_reset_keeps_accepting_the_existing_empty_payload() {
        let responses = vec![response(commands::ISP_END, &[])];
        let (mut flashing, requests) = test_flashing(0x82, vec![0; 8], responses);

        flashing.reset().unwrap();

        assert_eq!(
            *requests.borrow(),
            vec![Command::isp_end(1).into_raw().unwrap()]
        );
    }

    #[test]
    fn ch585_enable_debug_changes_only_the_debug_bit_and_reads_back() {
        let responses = vec![
            config_response(DEBUG_DISABLED),
            response(commands::WRITE_CONFIG, &[0, 0]),
            config_response(DEBUG_ENABLED_ROM_READ_DISABLED),
        ];
        let (mut flashing, requests) = test_flashing(0x85, CH585_UID.to_vec(), responses);

        flashing.enable_debug().unwrap();

        assert_eq!(
            *requests.borrow(),
            vec![
                Command::read_config(ch585::CONFIG_MASK).into_raw().unwrap(),
                Command::write_config(
                    ch585::CONFIG_MASK,
                    DEBUG_ENABLED_ROM_READ_DISABLED.to_vec(),
                )
                .into_raw()
                .unwrap(),
                Command::read_config(ch585::CONFIG_MASK).into_raw().unwrap(),
            ]
        );
    }

    #[test]
    fn ch585_program_and_verify_keep_the_standard_packet_shape() {
        let responses = vec![
            response(commands::PROGRAM, &[0, 0]),
            response(commands::VERIFY, &[0, 0]),
        ];
        let (mut ch585, requests) = test_flashing(0x85, CH585_UID.to_vec(), responses);

        ch585.flash_chunk(0x20, &[0x11, 0x22], [0; 8]).unwrap();
        ch585.verify_chunk(0x20, &[0x11, 0x22], [0; 8]).unwrap();

        let requests = requests.borrow();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0][0], commands::PROGRAM);
        assert_eq!(&requests[0][3..7], &0x20_u32.to_le_bytes());
        assert_eq!(&requests[0][8..], &[0x11, 0x22]);
        assert_eq!(requests[1][0], commands::VERIFY);
        assert_eq!(&requests[1][3..7], &0x20_u32.to_le_bytes());
        assert_eq!(&requests[1][8..], &[0x11, 0x22]);
    }

    #[test]
    fn ch585_prepared_program_does_not_send_a_second_key_after_erase() {
        let responses = vec![
            config_response(DEBUG_DISABLED),
            response(commands::WRITE_CONFIG, &[0, 0]),
            config_response(DEBUG_DISABLED),
            response(commands::ISP_KEY, &[0, 0]),
            response(commands::ERASE, &[0, 0]),
            response(commands::PROGRAM, &[0, 0]),
            response(commands::PROGRAM, &[0, 0]),
        ];
        let (mut flashing, requests) = test_flashing(0x85, CH585_UID.to_vec(), responses);

        let session = flashing.prepare_ch585_isp_flash().unwrap();
        flashing.erase_code(8).unwrap();
        flashing
            .flash_prepared_ch585(&[0x11; 8], &session)
            .unwrap();

        let requests = requests.borrow();
        assert_eq!(
            requests
                .iter()
                .filter(|request| request[0] == commands::ISP_KEY)
                .count(),
            1
        );
        let sequence: Vec<u8> = requests.iter().map(|request| request[0]).collect();
        assert_eq!(
            sequence,
            [
                commands::READ_CONFIG,
                commands::WRITE_CONFIG,
                commands::READ_CONFIG,
                commands::ISP_KEY,
                commands::ERASE,
                commands::PROGRAM,
                commands::PROGRAM,
            ]
        );
    }
}
