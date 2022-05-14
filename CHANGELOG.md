# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- Config register definition for CH32F10x
- Code erase impl
### Fixed
- Wrong verify impl
- Ignore reset protocol errors

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
- `erase` support
- ELF parsing

### Changed
- Debug print format

## [0.1.0] - 2022-05-08
### Added
- Initialize project - first reelase
