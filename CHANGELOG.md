# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add CH32X033 support
- Add CH585 support
- New `enable-debug` subcommand, also added to chip metadata
- Add CH570/CH572 support
- Add `disable-debug` subcommand for disabling SWD/debug mode
- Add `--retry` flag for slow bootloader detection
- Windows: Support CH375DLL64.dll driver as alternative to WinUSB (x64 only)

## [0.2.2] - 2023-10-03

### Added

- Refine device opening error message on Windows and Linux
- Add CH59x support
- Add CH32X03x support
- Add CH643 support(not tested)
- Add prebuilt binaries for Windows, Linux and macOS in the nightly release page
- Serieal transport support (#56)

### Fixed

- Verification error of CH58x caused by wrong CFG reset value #26
- Ignore USB device handle drop errors

## [0.2.2] - 2023-02-20

### Added

- Enable 2-wire debug for ch57x, update default reset config reg values

### Fixed

- Hang on Linux caused by libusb timeout #22

## [0.2.1] - 2023-01-28

### Added

- EEPROM erase support
- EEPROM write support
- Config register for CH57x

### Fixed

- Enable adaptive timeout setting

## [0.2.0] - 2022-11-13

### Added

- EEPROM dump support, fix #12
- Refactor all subcommands, using clap v4
- Probe support, multiple chips can be selected by an index
- Progressbar for flash and verify commands

### Changed

- Disable debug log by default

### Fixed

- Wrong timeout setting for usb transport

## [0.1.4] - 2022-11-13

### Added

- Config register definition for CH32F10x, CH32V20x, CH55x, CH58x
- Code erase impl
- Add schema for device description yaml
- Add no-verify, no-reset, no-erase to flash cmd

### Fixed

- Wrong verify impl
- Ignore reset protocol errors
- Wrong field definitions #2 #3
- Wrong chip info of CH55x

## [0.1.3] - 2022-05-14

### Added

- Basic config register parsing
- Config register reset support (buggy for old chips)

### Changed

- Refine chip family naming

## [0.1.2] - 2022-05-11

### Added

- New style chip DB, now parses MCU variants more accurately
- dump `info` support

### Changed

- BTVER, UID formating

### Fxied

- Wrong ELF parsing
- Check response code for `verify`

## [0.1.1] - 2022-05-09

### Added

- flash support
- ELF parsing

### Changed

- Debug print format

## [0.1.0] - 2022-05-08

### Added

- Initialize project - first release
