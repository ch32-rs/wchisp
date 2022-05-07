//! MCU Chip definition, with chip-specific or chip-family-specific flags
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Represents an MCU chip, defined in "devices.yaml"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chip {
    /// Chip's name, without variants surfix
    pub name: String,
    pub chip_id: u8,
    pub mcu_type: u8,
    #[serde(default)]
    pub skip_chip_id_check: bool,
    /// Max size of code flash, check agains firmware size.
    pub max_code_flash_size: u32,
    /// Size of data flash, i.e. EEPROM
    pub max_data_flash_size: u32,
    /// Start address used while programming data flash.
    /// 0 for standalone data flash, non-0 for offset somewhare
    #[serde(default)]
    pub data_flash_start: u32,
    pub support_net: bool,
    /// Flash via USB.
    pub support_usb: bool,
    /// Flash via UART.
    pub support_serial: bool,
}

impl ::std::fmt::Display for Chip {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "{}(0x{:02x}{:02x})", self.name, self.chip_id, self.device_type())
    }
}

impl Chip {
    pub fn guess(chip_id: u8, device_type: u8) -> Result<Self> {
        // TODO: compress chip DB
        let yaml: Vec<Chip> = serde_yaml::from_str(&include_str!("devices.yaml"))?;
        yaml.into_iter()
            .find(|chip| {
                chip.device_type() == device_type
                    && (chip.skip_chip_id_check() || chip.chip_id == chip_id)
            })
            .ok_or_else(|| {
                anyhow::format_err!(
                    "Can not find chip_id={:02x} in DB, please fire an issue",
                    chip_id
                )
            })
    }

    /// DeviceType = ChipSeries = SerialNumber = McuType + 0x10
    pub fn device_type(&self) -> u8 {
        self.mcu_type + 0x10
    }

    /// Used when erasing 1K sectors
    pub fn min_erase_sector_size(&self) -> usize {
        if self.device_type() == 0x10 {
            4
        } else {
            8
        }
    }

    /// Used when calculating XOR key
    pub fn uid_size(&self) -> usize {
        if self.device_type() == 0x11 {
            4
        } else {
            8
        }
    }

    /// Only checks device type, not chip id.
    pub fn skip_chip_id_check(&self) -> bool {
        self.skip_chip_id_check
        // i.e. self.device_type() == 0x14 || self.device_type() == 0x15
    }

    /// Code flash protect support
    pub fn support_code_flash_protect(&self) -> bool {
        [0x14, 0x15, 0x17, 0x18, 0x19, 0x20].contains(&self.device_type())
    }
}
