# wchisp - WCH ISP Tool in Rust

![crates.io](https://img.shields.io/crates/v/wchisp.svg)

Command-line implementation of the WCHISPTool in Rust, by the ch32-rs team.

This tool is a work in progress.

## Usage

```console
> cargo install wchisp --git https://github.com/ch32-rs/wchisp

> wchisp info
14:51:24 [INFO] Chip: CH32V307VCT6[0x7017] (Code Flash: 256KiB)
14:51:24 [INFO] Chip UID: 30-78-3e-26-3b-38-a9-d6
14:51:24 [INFO] BTVER(bootloader ver): 02.60
14:51:24 [INFO] Code Flash protected: false
RDPR_USER: 0x9F605AA5
  [7:0] RDPR 0b10100101 (0xA5)
    `- Unprotected
  [16:16] IWDG_SW 0b0 (0x0)
    `- IWDG enabled by the software
  [17:17] STOP_RST 0b0 (0x0)
    `- Enable
  [18:18] STANDBY_RST 0b0 (0x0)
    `- Enable
  [23:21] SRAM_CODE_MODE 0b11 (0x3)
    `- CODE-228KB + RAM-32KB
DATA: 0x00FF00FF
  [7:0] DATA0 0b11111111 (0xFF)
  [23:16] DATA1 0b11111111 (0xFF)
WRP: 0xFFFFFFFF
  `- Unprotected

> wchisp flash ./path/to/firmware.{bin,hex,elf}

> wchisp config info

> wchisp config reset
```

## Tested On

This tool should work on most WCH MCU chips. But I haven't tested it on any other chips.

- [x] CH32V307(VCT6)
- [x] CH32V103
- [x] CH32F103
- [x] CH582
  - CH58xM-EVT
- [x] CH559
  - CH559TL_MINIEVT_V20 by wch.cn
- [x] CH32V203
  - [CH32V203G6 FlappyBoard](https://github.com/metro94/FlappyBoard)
- ... (feel free to open an issue whether it works on your chip or not)

## Related Works (Many Thanks!)

- <https://github.com/MarsTechHAN/ch552tool>
- <https://github.com/MarsTechHAN/ch552tool/pull/21> by [@Pe3ucTop](https://github.com/Pe3ucTop/ch552tool/tree/global_rework)
- <https://github.com/Blinkinlabs/ch554_sdcc>
- <https://github.com/rgwan/librech551>
- <https://github.com/jobitjoseph/CH55XDuino>
- <https://github.com/frank-zago/isp55e0>

### Contribution

This project is under active development. If you have any suggestions or bug reports, please open an issue.

If it works for your devices, please open a pull request to modify this README page.

It it doesn't, please open an issue. Better provide the following information:

- chip type (with variant surfix)
- debug print of usb packets
- correct usb packets to negotiate with the chip (via USBPcap or other tools)
