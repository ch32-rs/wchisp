# wchisp - WCH ISP Tool in Rust

Command-line implementation of the WCHISPTool in Rust, by the ch32-rs team.

This tool is a work in progress.

## Usage

```console
> cargo install wchisp --git https://github.com/ch32-rs/wchisp

> wchisp info
16:44:52 [INFO] Chip: CH32V307(0x7017) (Code Flash: 480KiB, Data EEPROM: 0KiB)
16:44:52 [INFO] Chip UID: .......
16:44:52 [INFO] BTVER(bootloader ver): 020600
16:44:52 [INFO] Code Flash protected: false

> wchisp flash ./path/to/firmware.{bin,hex,elf}
```

## Tested On

This tool should work on most WCH MCU chips. But I haven't tested it on any other chips.

- [x] CH32V307(VCT6)
- ... (feel free to open an issue whether it works on your chip or not)

## Related Works (Many Thanks!)

- https://github.com/MarsTechHAN/ch552tool
- https://github.com/Blinkinlabs/ch554_sdcc
- https://github.com/rgwan/librech551
- https://github.com/jobitjoseph/CH55XDuino
- https://github.com/frank-zago/isp55e0
