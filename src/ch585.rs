//! CH585-specific ISP policy.
//!
//! Keep the CH585 BootROM workarounds here so the established flashing paths
//! for older WCH devices remain unchanged.

use anyhow::{ensure, Result};

pub(crate) const CONFIG_MASK: u8 = 0x07;
pub(crate) const CONFIG_BYTES: usize = 12;
pub(crate) const USER_CFG_OFFSET: usize = 8;
pub(crate) const CFG_DEBUG_EN: u32 = 1 << 4;
pub(crate) const CFG_ROM_READ: u32 = 1 << 7;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlashConfigTransition {
    pub(crate) original: [u8; CONFIG_BYTES],
    pub(crate) programmed: [u8; CONFIG_BYTES],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlashSession {
    pub(crate) config: FlashConfigTransition,
    pub(crate) key: [u8; 8],
}

pub(crate) struct IspKey {
    pub(crate) payload: Vec<u8>,
    pub(crate) xor: [u8; 8],
}

impl FlashConfigTransition {
    pub(crate) fn requires_restore(self) -> bool {
        self.original != self.programmed
    }
}

pub(crate) fn parse_config_payload(payload: &[u8]) -> Result<[u8; CONFIG_BYTES]> {
    ensure!(
        payload.len() == CONFIG_BYTES + 2,
        "CH585 read_config returned {} bytes, expected {}",
        payload.len(),
        CONFIG_BYTES + 2
    );
    ensure!(
        payload[..2] == [CONFIG_MASK, 0],
        "CH585 read_config mask echo mismatch: {}",
        hex::encode(&payload[..2])
    );
    Ok(payload[2..]
        .try_into()
        .expect("CH585 configuration length was checked"))
}

pub(crate) fn prepare_flash_config(original: [u8; CONFIG_BYTES]) -> Result<FlashConfigTransition> {
    let user_cfg = user_cfg(&original);
    ensure!(
        user_cfg >> 28 == 0x4,
        "refusing invalid CH585 USER_CFG signature 0x{user_cfg:08x}"
    );

    let mut programmed = original;
    programmed[USER_CFG_OFFSET..USER_CFG_OFFSET + 4]
        .copy_from_slice(&(user_cfg & !(CFG_DEBUG_EN | CFG_ROM_READ)).to_le_bytes());
    Ok(FlashConfigTransition {
        original,
        programmed,
    })
}

pub(crate) fn set_debug(original: [u8; CONFIG_BYTES], enabled: bool) -> Result<[u8; CONFIG_BYTES]> {
    let user_cfg = user_cfg(&original);
    ensure!(
        user_cfg >> 28 == 0x4,
        "refusing invalid CH585 USER_CFG signature 0x{user_cfg:08x}"
    );
    let requested = if enabled {
        user_cfg | CFG_DEBUG_EN
    } else {
        user_cfg & !CFG_DEBUG_EN
    };
    let mut result = original;
    result[USER_CFG_OFFSET..USER_CFG_OFFSET + 4].copy_from_slice(&requested.to_le_bytes());
    Ok(result)
}

pub(crate) fn check_bootrom_status(payload: &[u8], operation: &str) -> Result<()> {
    ensure!(
        payload.len() == 2,
        "{operation} returned unexpected payload: {}",
        hex::encode(payload)
    );
    let status = u16::from_le_bytes([payload[0], payload[1]]);
    ensure!(
        status == 0,
        "{operation} rejected by BootROM with status 0x{status:04x}"
    );
    Ok(())
}

pub fn pad_firmware(raw: &mut Vec<u8>) {
    let remainder = raw.len() % 8;
    if remainder != 0 {
        raw.resize(raw.len() + 8 - remainder, 0xff);
    }
}

pub(crate) fn generate_isp_key(uid: &[u8], chip_id: u8) -> IspKey {
    let length = 0x1e + usize::from(rand::random::<u8>() % 0x1f);
    let payload: Vec<u8> = (0..length).map(|_| rand::random()).collect();
    let xor = derive_isp_xor_key(uid, chip_id, &payload);
    IspKey { payload, xor }
}

fn derive_isp_xor_key(uid: &[u8], chip_id: u8, payload: &[u8]) -> [u8; 8] {
    debug_assert!((0x1e..=0x3c).contains(&payload.len()));
    let uid_sum = uid
        .iter()
        .take(8)
        .fold(0_u8, |sum, byte| sum.wrapping_add(*byte));
    let fifth = payload.len() / 5;
    let mixed = payload.len() / 7 + (payload.len() - payload.len() / 7) / 2;
    let quarter = mixed / 4;
    let first = payload[quarter * 4] ^ uid_sum;

    [
        first,
        payload[fifth] ^ uid_sum,
        payload[quarter] ^ uid_sum,
        payload[quarter * 6] ^ uid_sum,
        payload[quarter * 3] ^ uid_sum,
        payload[fifth * 3] ^ uid_sum,
        payload[quarter * 5] ^ uid_sum,
        first.wrapping_add(chip_id),
    ]
}

fn user_cfg(config: &[u8; CONFIG_BYTES]) -> u32 {
    u32::from_le_bytes(
        config[USER_CFG_OFFSET..USER_CFG_OFFSET + 4]
            .try_into()
            .expect("CH585 USER_CFG has a fixed width"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEBUG_ENABLED: [u8; CONFIG_BYTES] = [
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xdf, 0x3f, 0x0f, 0x45,
    ];

    #[test]
    fn preparation_clears_only_isp_incompatible_bits() {
        let transition = prepare_flash_config(DEBUG_ENABLED).unwrap();
        assert_eq!(
            transition.programmed,
            [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x4f, 0x3f, 0x0f, 0x45,]
        );
        assert_eq!(transition.original, DEBUG_ENABLED);
        assert!(transition.requires_restore());
    }

    #[test]
    fn preparation_preserves_debug_disabled_config() {
        let mut original = DEBUG_ENABLED;
        original[USER_CFG_OFFSET] &= !(CFG_DEBUG_EN | CFG_ROM_READ) as u8;
        let transition = prepare_flash_config(original).unwrap();
        assert_eq!(transition.programmed, original);
        assert!(!transition.requires_restore());
    }

    #[test]
    fn preparation_rejects_invalid_signature() {
        let mut original = DEBUG_ENABLED;
        original[USER_CFG_OFFSET + 3] = 0x55;
        let error = prepare_flash_config(original).unwrap_err();
        assert!(error
            .to_string()
            .contains("invalid CH585 USER_CFG signature"));
    }

    #[test]
    fn debug_update_preserves_other_bits() {
        let disabled = set_debug(DEBUG_ENABLED, false).unwrap();
        assert_eq!(disabled[USER_CFG_OFFSET], 0xcf);
        assert_eq!(set_debug(disabled, true).unwrap(), DEBUG_ENABLED);
    }

    #[test]
    fn firmware_padding_is_ff_and_eight_byte_aligned() {
        let mut raw = vec![1, 2, 3];
        pad_firmware(&mut raw);
        assert_eq!(raw, [1, 2, 3, 0xff, 0xff, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn config_parser_checks_echo_and_length() {
        let mut response = vec![CONFIG_MASK, 0];
        response.extend_from_slice(&DEBUG_ENABLED);
        assert_eq!(parse_config_payload(&response).unwrap(), DEBUG_ENABLED);

        response[0] = 0x1f;
        assert!(parse_config_payload(&response).is_err());
        assert!(parse_config_payload(&response[..13]).is_err());
    }

    #[test]
    fn bootrom_status_checks_both_bytes() {
        check_bootrom_status(&[0, 0], "program").unwrap();
        let low = check_bootrom_status(&[0xfe, 0], "program").unwrap_err();
        assert!(low.to_string().contains("status 0x00fe"));
        let high = check_bootrom_status(&[0, 1], "program").unwrap_err();
        assert!(high.to_string().contains("status 0x0100"));
        assert!(check_bootrom_status(&[0], "program").is_err());
    }

    #[test]
    fn isp_key_derivation_uses_generated_payload_and_uid() {
        let uid = [0x98, 0x5b, 0x29, 0x5a, 0x04, 0xdc, 0xc5, 0x91];
        let payload: Vec<u8> = (0..0x1e).collect();
        assert_eq!(
            derive_isp_xor_key(&uid, 0x85, &payload),
            [0xbc, 0xaa, 0xa8, 0xb4, 0xa0, 0xbe, 0xb8, 0x41]
        );
    }
}
