# wchisp - WCH ISP Tool in Rust

Command-line implementation of the WCHISPTool in Rust, by the ch32-rs team.

This tool is a work in progress.

## Usage

```console
> cargo install wchisp --git https://github.com/ch32-rs/wchisp

> wchisp info
14:51:24 [INFO] Chip: CH32V307VCT6(0x7017) (Code Flash: 256KiB, Data EEPROM: 0KiB)
14:51:24 [INFO] Chip UID: 30-78-3e-26-3b-38-a9-d6
14:51:24 [INFO] BTVER(bootloader ver): 02.60
14:51:24 [INFO] Code Flash protected: false

> wchisp flash ./path/to/firmware.{bin,hex,elf}
```

## ChangeLog

- Unreleased
  - minior bug fixes for chip db and register name

- 0.1.2
  - Refactor chip db, add chip family & variants

- 0.1.1
  - support ELF parsing
  - refine chip db

- 0.1.0
  - Initial release

## Tested On

This tool should work on most WCH MCU chips. But I haven't tested it on any other chips.

- [x] CH32V307(VCT6)
- ... (feel free to open an issue whether it works on your chip or not)

## Related Works (Many Thanks!)

- https://github.com/MarsTechHAN/ch552tool
- https://github.com/MarsTechHAN/ch552tool/pull/21 by [@Pe3ucTop](https://github.com/Pe3ucTop/ch552tool/tree/global_rework)
- https://github.com/Blinkinlabs/ch554_sdcc
- https://github.com/rgwan/librech551
- https://github.com/jobitjoseph/CH55XDuino
- https://github.com/frank-zago/isp55e0
