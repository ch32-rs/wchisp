//! MCU Chip definition, with chip-specific or chip-family-specific flags
use std::collections::BTreeMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// MCU Family
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChipFamily {
    pub name: String,
    pub mcu_type: u8,
    pub device_type: u8,
    support_usb: Option<bool>,
    support_serial: Option<bool>,
    support_net: Option<bool>,
    pub description: String,
    pub variants: Vec<Chip>,
    #[serde(default)]
    pub config_registers: Vec<ConfigRegister>,
}

impl ChipFamily {
    fn validate(&self) -> Result<()> {
        for variant in &self.variants {
            variant.validate()?;
        }
        for register in &self.config_registers {
            register.validate()?;
        }
        Ok(())
    }
}

/// Represents an MCU chip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chip {
    /// Chip's name, without variants surfix
    pub name: String,
    pub chip_id: u8,
    #[serde(default, deserialize_with = "parse_alt_chip_id_or_all_marker")]
    alt_chip_ids: Vec<u8>,

    #[serde(default)]
    pub mcu_type: u8,
    #[serde(default)]
    pub device_type: u8,

    #[serde(deserialize_with = "parse_address_and_offset")]
    pub flash_size: u32,
    #[serde(default, deserialize_with = "parse_address_and_offset")]
    pub eeprom_size: u32,

    #[serde(default, deserialize_with = "parse_address_and_offset")]
    pub eeprom_start_addr: u32,

    support_net: Option<bool>,
    support_usb: Option<bool>,
    support_serial: Option<bool>,

    #[serde(default)]
    pub config_registers: Vec<ConfigRegister>,
}

impl ::std::fmt::Display for Chip {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(
            f,
            "{}[0x{:02x}{:02x}]",
            self.name,
            self.chip_id,
            self.device_type(),
        )
    }
}

impl Chip {
    pub fn validate(&self) -> Result<()> {
        for reg in &self.config_registers {
            reg.validate()?;
        }
        Ok(())
    }
}

/// A u32 config register, with reset values.
///
/// The reset value is NOT the value of the register when the device is reset,
/// but the value of the register when the device is in the flash-able mode.
///
/// Read in LE mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigRegister {
    pub offset: usize,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub reset: Option<u32>,
    pub enable_debug: Option<u32>,
    pub disable_debug: Option<u32>,
    #[serde(default)]
    pub explaination: BTreeMap<String, String>,
    #[serde(default)]
    pub fields: Vec<RegisterField>,
}

impl ConfigRegister {
    fn validate(&self) -> Result<()> {
        if self.offset % 4 != 0 {
            anyhow::bail!("Config register offset must be 4-byte aligned");
        }
        for field in &self.fields {
            field.validate()?;
        }
        Ok(())
    }
}

/// A range of bits in a register, with a name and a description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterField {
    // RangeInclusive is not supported well since serde_yaml 0.9
    pub bit_range: Vec<u8>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    // NOTE: use BTreeMap for strict ordering for digits and `_`
    #[serde(default)]
    pub explaination: BTreeMap<String, String>,
}

impl RegisterField {
    fn validate(&self) -> Result<()> {
        if self.bit_range.len() != 2 {
            anyhow::bail!("Invalid bit range: {:?}", self.bit_range);
        }
        if self.bit_range[0] < self.bit_range[1] {
            anyhow::bail!("Invalid bit range: {:?}", self.bit_range);
        }
        Ok(())
    }
}

pub struct ChipDB {
    pub families: Vec<ChipFamily>,
}

impl ChipDB {
    pub fn load() -> Result<Self> {
        let families: Vec<ChipFamily> = vec![
            serde_yaml::from_str(include_str!("../devices/0x10-CH56x.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x11-CH55x.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x12-CH54x.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x13-CH57x.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x14-CH32F103.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x15-CH32V103.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x16-CH58x.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x17-CH32V30x.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x18-CH32F20x.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x19-CH32V20x.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x20-CH32F20x-Compact.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x21-CH32V00x.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x22-CH59x.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x23-CH32X03x.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x24-CH643.yaml"))?,
            serde_yaml::from_str(include_str!("../devices/0x25-CH32L103.yaml"))?,
        ];
        for family in &families {
            family.validate()?;
        }
        Ok(ChipDB { families })
    }

    pub fn find_chip(&self, chip_id: u8, device_type: u8) -> Result<Chip> {
        let family = self
            .families
            .iter()
            .find(|f| f.device_type == device_type)
            .ok_or_else(|| anyhow::format_err!("Device type of 0x{:02x} not found", device_type))?;

        let mut chip = family
            .variants
            .iter()
            .find(|c| c.chip_id == chip_id || c.alt_chip_ids.contains(&chip_id))
            .cloned()
            .ok_or_else(|| {
                anyhow::format_err!(
                    "Cannot find chip with id 0x{:02x} device_type 0x{:02x}",
                    chip_id,
                    device_type
                )
            })?;
        // FIXME: better way to patch chip type?
        chip.mcu_type = family.mcu_type;
        chip.device_type = family.device_type;
        if chip_id != chip.chip_id {
            log::warn!("Find chip via alternative id: 0x{:02x}", chip.chip_id);
            chip.chip_id = chip_id;
        }
        if chip.support_net.is_none() {
            chip.support_net = family.support_net;
        }
        if chip.support_usb.is_none() {
            chip.support_usb = family.support_usb;
        }
        if chip.support_serial.is_none() {
            chip.support_serial = family.support_serial;
        }
        if chip.config_registers.is_empty() {
            chip.config_registers = family.config_registers.clone();
        }
        Ok(chip)
    }
}

impl Chip {
    /// DeviceType = ChipSeries = SerialNumber = McuType + 0x10
    pub const fn device_type(&self) -> u8 {
        self.mcu_type + 0x10
    }

    /// Used when erasing 1K sectors
    pub const fn min_erase_sector_number(&self) -> u32 {
        if self.device_type() == 0x10 {
            4
        } else {
            8
        }
    }

    /// Used when calculating XOR key
    pub const fn uid_size(&self) -> usize {
        if self.device_type() == 0x11 {
            4
        } else {
            8
        }
    }

    /// Code flash protect support
    pub fn support_code_flash_protect(&self) -> bool {
        [0x14, 0x15, 0x17, 0x18, 0x19, 0x20].contains(&self.device_type())
    }
}

fn parse_alt_chip_id_or_all_marker<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let ids: Vec<String> = serde::Deserialize::deserialize(deserializer)?;
    Ok(ids
        .into_iter()
        .flat_map(|i| {
            if i.starts_with("0x") || i.starts_with("0X") {
                vec![i[2..].parse().unwrap()]
            } else if i == "all" || i == "ALL" {
                (0..=0xff).into_iter().collect()
            } else {
                vec![i.parse().unwrap()]
            }
        })
        .collect())
}

fn parse_address_and_offset<'de, D>(deserializer: D) -> std::result::Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    if s.starts_with("0x") || s.starts_with("0X") {
        Ok(u32::from_str_radix(&s[2..], 16).expect(&format!("error while parsering {:?}", s)))
    } else if s.ends_with("K") {
        Ok(1024
            * u32::from_str_radix(&s[..s.len() - 1], 10)
                .expect(&format!("error while parsering {:?}", s)))
    } else if s.ends_with("KiB") {
        Ok(1024
            * u32::from_str_radix(&s[..s.len() - 3], 10)
                .expect(&format!("error while parsering {:?}", s)))
    } else if s.ends_with("KB") {
        Ok(1024
            * u32::from_str_radix(&s[..s.len() - 2], 10)
                .expect(&format!("error while parsering {:?}", s)))
    } else {
        // parse pure digits here
        Ok(s.parse().unwrap())
    }
}

pub fn parse_number(s: &str) -> Option<u32> {
    if s.starts_with("0x") || s.starts_with("0X") {
        Some(u32::from_str_radix(&s[2..], 16).expect(&format!("error while parsering {:?}", s)))
    } else if s.starts_with("0b") || s.starts_with("0B") {
        Some(u32::from_str_radix(&s[2..], 2).expect(&format!("error while parsering {:?}", s)))
    } else {
        Some(s.parse().expect("must be a number"))
    }
}
