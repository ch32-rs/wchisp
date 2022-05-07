
pub const MAX_PACKET_SIZE: usize = 64;

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

