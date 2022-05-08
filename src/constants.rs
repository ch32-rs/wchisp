//! Constants about protocol and devices.

pub const MAX_PACKET_SIZE: usize = 64;
pub const SECTOR_SIZE: usize = 1024;

/// All readable and writable registers.
/// - `RDPR`: Read Protection
/// - `USER`: User Config Byte (normally in Register Map datasheet)
/// - `WPR`:  Write Protection Mask, 1=unprotected, 0=protected
///
/// | BYTE0  | BYTE1  | BYTE2  | BYTE3  |
/// |--------|--------|--------|--------|
/// | RDPR   | nRDPR  | USER   | nUSER  |
/// | DATA0  | nDATA0 | DATA1  | nDATA1 |
/// | WPR0   | WPR1   | WPR2   | WPR3   |
pub const CFG_MASK_RDPR_USER_DATA_WPR: u8 = 0x07;
/// Bootloader version, in the format of `[0x00, major, minor, 0x00]`
pub const CFG_MASK_BTVER: u8 = 0x08;
/// Device Unique ID
pub const CFG_MASK_UID: u8 = 0x10;
/// All mask bits of CFGs
pub const CFG_MASK_ALL: u8 = 0x1f;

pub mod commands {
    pub const IDENTIFY: u8 = 0xa1;
    pub const ISP_END: u8 = 0xa2;
    pub const ISP_KEY: u8 = 0xa3;
    pub const ERASE: u8 = 0xa4;
    pub const PROGRAM: u8 = 0xa5;
    pub const VERIFY: u8 = 0xa6;
    pub const READ_CONFIG: u8 = 0xa7;
    pub const WRITE_CONFIG: u8 = 0xa8;
    pub const DATA_ERASE: u8 = 0xa9;
    pub const DATA_PROGRAM: u8 = 0xaa;
    pub const DATA_READ: u8 = 0xab;
    pub const WRITE_OTP: u8 = 0xc3;
    pub const READ_OTP: u8 = 0xc4;
    pub const SET_BAUD: u8 = 0xc5;
}
