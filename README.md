# wchisp - WCH ISP Tool in Rust

![crates.io](https://img.shields.io/crates/v/wchisp.svg)

Command-line implementation of the WCHISPTool in Rust, by the ch32-rs team.

This tool is a work in progress.

- **NOTE**: CH32V003 **DOES NOT** have a USB ISP interface, use WCH-Link to program
- **NOTE**: This tool is for USB ISP, not using with WCH-Link
  - [ch32-rs/wlink](https://github.com/ch32-rs/wlink) is a command line tool for WCH-Link

## Installing

The prebuilt binaries are available on the [Nightly release page](https://github.com/ch32-rs/wchisp/releases/tag/nightly).

For Windows users, you will need vc runtime to run the binary. You can download it from [Microsoft](https://learn.microsoft.com/en-US/cpp/windows/latest-supported-vc-redist?view=msvc-170).

Or else, you can install it from source.

```console
# install libusb for your platform
# macOS
> brew install libusb
# Ubuntu
> sudo apt install libusb-1.0-0-dev

# install wchisp
> cargo install wchisp --git https://github.com/ch32-rs/wchisp
# or use
> cargo install wchisp --force
```

### Prebuilt Binaries

Prebuilt binaries are available on the Github Actions Page.
Click the newest runs at [Github Actions Page](https://github.com/ch32-rs/wchisp/actions/workflows/rust.yml) and download the binary from "Artifacts" section.

### Note for Windows

If you are using Windows, you need to install the WinUSB driver for your device.
See [Zadig](https://zadig.akeo.ie/).

NOTE: This is not compatible with the Official WCH driver you installed with IDE.

### Note for Linux

If you are using Linux, you need to set the udev rules for your device.

```text
# /etc/udev/rules.d/50-wchisp.rules
SUBSYSTEM=="usb", ATTRS{idVendor}=="4348", ATTRS{idProduct}=="55e0", MODE="0666"
# or replace MODE="0666" with GROUP="plugdev" or something else
```

### Arch Linux

Arch Linux users can install [wchisp](https://aur.archlinux.org/packages/wchisp) or [wchisp-git](https://aur.archlinux.org/packages/wchisp-git) via the AUR.

```bash
yay -Syu wchisp
```

## Usage

```console
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

- [x] CH32V307
  - VCT6
  - RCT6 #8
- [x] CH32V103
- [x] CH32F103
- [x] CH552
  - Works but might be buggy #10 #14
- [x] CH582
  - CH58xM-EVT
- [x] CH573
  - [WeActStudio.WCH-BLE-Core](https://github.com/WeActStudio/WeActStudio.WCH-BLE-Core)
- [x] CH592
  - [WeActStudio.WCH-BLE-Core](https://github.com/WeActStudio/WeActStudio.WCH-BLE-Core)
  - [WeAct CH592F BLE5.4 Mini Core](https://www.aliexpress.com/item/1005006117859297.html)
- [x] CH579
  - BTVER: 02.90 #18
- [x] CH559
  - CH559TL_MINIEVT_V20 by wch.cn
- [x] CH32V203
  - [CH32V203G6 FlappyBoard](https://github.com/metro94/FlappyBoard)
  - [nanoCH32V203](https://github.com/wuxx/nanoCH32V203)
- ... (feel free to open an issue whether it works on your chip or not)

## TODOs

- [x] chip detection, identification
  - `wchisp probe`
  - `wchisp info`
- [x] flash and verify code
  - [x] ELF parsing
  - [x] hex, bin, ihex support
  - [x] skip erasing, verifying, resetting
- [x] chip config register dump
  - `wchisp config`
  - works for most chips, but not all. Issues and PRs are welcomed
- [ ] write config registers
  - [x] reset config registers to default
  - [ ] write config with friendly register names? like `wchisp config set SRAM_CODE_MODE=1 ...`
- [x] EEPROM dump
- [x] EEPROM erase
- [x] EEPROM write
- [x] select from multiple chips(using `-d` to select device index) `wchisp -d 0 info`
- [ ] ISP via UART or Net

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
