use std::{thread::sleep, time::Duration};

use anyhow::Result;

use clap::{Parser, Subcommand};
use hxdmp::hexdump;

use wchisp::{constants::SECTOR_SIZE, transport::UsbTransport, Flashing};

#[derive(clap::Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Optional device index to operate on
    #[arg(long, short = 'd', value_name = "INDEX")]
    device: Option<usize>,

    /// Turn debugging information on
    #[arg(long = "verbose", short = 'v')]
    debug: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Probe any connected devices
    Probe {},
    /// Get info about current connected chip
    Info {
        /// Chip name(prefix) check
        #[arg(long)]
        chip: Option<String>,
    },
    /// Reset the target connected
    Reset {},
    /// Erase code flash
    Erase {},
    /// Download to code flash and reset
    Flash {
        /// The path to the file to be downloaded to the code flash
        path: String,
        /// Do not erase the code flash before flashing
        #[clap(short = 'E', long)]
        no_erase: bool,
        /// Do not verify the code flash after flashing
        #[clap(short = 'V', long)]
        no_verify: bool,
        /// Do not reset the target after flashing
        #[clap(short = 'R', long)]
        no_reset: bool,
    },
    /// Verify code flash content
    Verify { path: String },
    /// EEPROM(data flash) operations
    Eeprom {
        #[command(subcommand)]
        command: Option<EepromCommands>,
    },
    /// Config CFG register
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommands>,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Dump config register info
    Info {},
    /// Reset config register to default
    Reset {},
    /// Set config register to new value
    Set {
        /// New value of the config register
        #[arg(value_name = "HEX")]
        value: String,
    },
    /// Unprotect code flash
    Unprotect {},
}

#[derive(Subcommand)]
enum EepromCommands {
    /// Dump EEPROM data
    Dump {
        /// The path of the file to be written to
        path: Option<String>,
    },
    /// Programming EEPROM data
    Flash {
        /// The path to the file to be downloaded to the data flash
        path: String,
        /// Do not erase the data flash before flashing
        #[clap(short = 'E', long)]
        no_erase: bool,
        /// Do not verify the data flash after flashing
        #[clap(short = 'V', long)]
        no_verify: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.debug {
        let _ = simplelog::TermLogger::init(
            simplelog::LevelFilter::Debug,
            simplelog::Config::default(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        );
    } else {
        let _ = simplelog::TermLogger::init(
            simplelog::LevelFilter::Info,
            simplelog::Config::default(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        );
    }

    let device_idx = cli.device.unwrap_or_default();

    match cli.command {
        None | Some(Commands::Probe {}) => {
            let ndevices = UsbTransport::scan_devices()?;
            log::info!("Found {} devices", ndevices);
            log::info!("hint: use `wchisp info` to check chip info");
            for i in 0..ndevices {
                let mut trans = UsbTransport::open_nth(i)?;
                let chip = Flashing::get_chip(&mut trans)?;
                println!("Device #{}: {}", i, chip);
            }
        }
        Some(Commands::Info { chip }) => {
            let mut flashing = Flashing::open_nth_usb_device(device_idx)?;
            if let Some(expected_chip_name) = chip {
                flashing.check_chip_name(&expected_chip_name)?;
            }
            flashing.dump_info()?;
        }
        Some(Commands::Reset {}) => {
            let mut flashing = Flashing::open_nth_usb_device(device_idx)?;
            let _ = flashing.reset();
        }
        Some(Commands::Erase {}) => {
            let mut flashing = Flashing::open_nth_usb_device(device_idx)?;

            let sectors = flashing.chip.flash_size / 1024;
            flashing.erase_code(sectors)?;
        }
        // WRITE_CONFIG => READ_CONFIG => ISP_KEY => ERASE => PROGRAM => VERIFY => RESET
        Some(Commands::Flash {
            path,
            no_erase,
            no_verify,
            no_reset,
        }) => {
            let mut flashing = Flashing::open_nth_usb_device(device_idx)?;

            flashing.dump_info()?;

            let mut binary = wchisp::format::read_firmware_from_file(path)?;
            extend_firmware_to_sector_boundary(&mut binary);
            log::info!("Firmware size: {}", binary.len());

            if no_erase {
                log::warn!("Skipping erase");
            } else {
                log::info!("Erasing...");
                let sectors = binary.len() / SECTOR_SIZE + 1;
                flashing.erase_code(sectors as u32)?;

                sleep(Duration::from_secs(1));
                log::info!("Erase done");
            }

            log::info!("Writing to code flash...");
            flashing.flash(&binary)?;
            sleep(Duration::from_millis(500));

            if no_verify {
                log::warn!("Skipping verify");
            } else {
                log::info!("Verifying...");
                flashing.verify(&binary)?;
                sleep(Duration::from_secs(1));
                log::info!("Verify OK");
            }

            if no_reset {
                log::warn!("Skipping reset");
            } else {
                log::info!("Now reset device and skip any communication errors");
                let _ = flashing.reset();
            }
        }
        Some(Commands::Verify { path }) => {
            let mut flashing = Flashing::open_nth_usb_device(device_idx)?;
            let mut binary = wchisp::format::read_firmware_from_file(path)?;
            extend_firmware_to_sector_boundary(&mut binary);
            log::info!("Firmware size: {}", binary.len());
            log::info!("Verifying...");
            flashing.verify(&binary)?;
            log::info!("Verify OK");
        }
        Some(Commands::Eeprom { command }) => {
            let mut flashing = Flashing::open_nth_usb_device(device_idx)?;
            match command {
                None | Some(EepromCommands::Dump { .. }) => {
                    flashing.reidenfity()?;

                    log::info!("Reading EEPROM(Data Flash)...");

                    let eeprom = flashing.dump_eeprom()?;
                    log::info!("EEPROM data size: {}", eeprom.len());

                    if let Some(EepromCommands::Dump {
                        path: Some(ref path),
                    }) = command
                    {
                        std::fs::write(path, eeprom)?;
                        log::info!("EEPROM data saved to {}", path);
                    } else {
                        let mut buf = vec![];
                        hexdump(&eeprom, &mut buf)?;
                        println!("{}", String::from_utf8_lossy(&buf));
                    }
                }
                Some(EepromCommands::Flash {
                    //path,
                    //no_erase,
                    //no_verify,
                    ..
                }) => {
                    unimplemented!()
                }
            }
        }
        Some(Commands::Config { command }) => {
            let mut flashing = Flashing::open_nth_usb_device(device_idx)?;
            match command {
                None | Some(ConfigCommands::Info {}) => {
                    flashing.dump_config()?;
                }
                Some(ConfigCommands::Reset {}) => {
                    flashing.reset_config()?;
                }
                Some(ConfigCommands::Set { value }) => {
                    // flashing.write_config(value)?;
                    log::info!("setting cfg value {}", value);
                    unimplemented!()
                }
                Some(ConfigCommands::Unprotect {}) => {
                    flashing.unprotect(true)?;
                }
            }
        }
    }

    Ok(())
}

fn extend_firmware_to_sector_boundary(buf: &mut Vec<u8>) {
    if buf.len() % 1024 != 0 {
        let remain = 1024 - (buf.len() % 1024);
        buf.extend_from_slice(&vec![0; remain]);
    }
}
