//! MCU Chip definition, with chip-specific or chip-family-specific flags
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// MCU Family
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Family {
    pub name: String,
    pub mcu_type: u8,
    pub device_type: u8,
    support_usb: Option<bool>,
    support_serial: Option<bool>,
    support_net: Option<bool>,
    pub description: String,
    pub variants: Vec<Chip>,
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
}

impl ::std::fmt::Display for Chip {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(
            f,
            "{}(0x{:02x}{:02x})",
            self.name,
            self.chip_id,
            self.device_type(),
        )
    }
}

pub struct ChipDB {
    families: Vec<Family>,
}

impl ChipDB {
    pub fn load() -> Result<Self> {
        Ok(ChipDB {
            families: vec![
                serde_yaml::from_str(&include_str!("../devices/0x10-CH56x.yaml"))?,
                serde_yaml::from_str(&include_str!("../devices/0x11-CH55x.yaml"))?,
                serde_yaml::from_str(&include_str!("../devices/0x12-CH54x.yaml"))?,
                serde_yaml::from_str(&include_str!("../devices/0x13-CH57x.yaml"))?,
                serde_yaml::from_str(&include_str!("../devices/0x14-CH32F103.yaml"))?,
                serde_yaml::from_str(&include_str!("../devices/0x15-CH32V103.yaml"))?,
                serde_yaml::from_str(&include_str!("../devices/0x16-CH58x.yaml"))?,
                serde_yaml::from_str(&include_str!("../devices/0x17-CH32V30x.yaml"))?,
                serde_yaml::from_str(&include_str!("../devices/0x18-CH32F20x.yaml"))?,
                serde_yaml::from_str(&include_str!("../devices/0x19-CH32V20x.yaml"))?,
                serde_yaml::from_str(&include_str!("../devices/0x20-CH32F20x-Compact.yaml"))?,
            ],
        })
    }

    pub fn find_chip(chip_id: u8, device_type: u8) -> Result<Chip> {
        let db = ChipDB::load()?;

        let family = db
            .families
            .iter()
            .find(|f| f.device_type == device_type)
            .ok_or_else(|| anyhow::format_err!("Device type of 0x{:02x} not found", device_type))?;

        log::debug!("Find chip family: {}", family.name);
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
