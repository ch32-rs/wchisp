use std::{thread::sleep, time::Duration};

use anyhow::Result;

use clap::{Parser, Subcommand};
use hxdmp::hexdump;

use wchisp::{
    constants::SECTOR_SIZE,
    transport::{SerialTransport, UsbTransport},
    Baudrate, Flashing,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[clap(group(clap::ArgGroup::new("transport").args(&["usb", "serial"])))]
struct Cli {
    /// Turn debugging information on
    #[arg(long = "verbose", short = 'v')]
    debug: bool,

    /// Use the USB transport layer
    #[arg(long, short, default_value_t = true, default_value_if("serial", clap::builder::ArgPredicate::IsPresent, "false"), conflicts_with_all = ["serial", "port", "baudrate"])]
    usb: bool,

    /// Use the Serial transport layer
    #[arg(long, short, conflicts_with_all = ["usb", "device"])]
    serial: bool,

    /// Optional USB device index to operate on
    #[arg(long, short, value_name = "INDEX", default_value = None, requires = "usb")]
    device: Option<usize>,

    /// Select the serial port
    #[arg(long, short, requires = "serial")]
    port: Option<String>,

    /// Select the serial baudrate
    #[arg(long, short, ignore_case = true, value_enum, requires = "serial")]
    baudrate: Option<Baudrate>,

    /// Retry scan for certain seconds, helpful on slow USB devices
    #[arg(long, short, default_value = "0")]
    retry: u32,

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
    /// Enable SWD mode(simulation mode)
    EnableDebug {},
    /// Disable SWD mode(simulation mode)
    DisableDebug {},
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
    /// Erase EEPROM data
    Erase {},
    /// Programming EEPROM data
    Write {
        /// The path to the file to be downloaded to the data flash
        path: String,
        /// Do not erase the data flash before programming
        #[clap(short = 'E', long)]
        no_erase: bool,
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

    if cli.retry > 0 {
        if !cli.usb && !cli.serial {
            log::warn!("No transport method specified (--usb or --serial); skipping retry logic.");
        } else {
            log::info!("Retrying scan for {} seconds", cli.retry);
            let start_time = std::time::Instant::now();
            while start_time.elapsed().as_secs() < cli.retry as u64 {
                if cli.usb {
                    let ndevices = UsbTransport::scan_devices()?;
                    if ndevices > 0 {
                        break;
                    }
                } else if cli.serial {
                    let ports = SerialTransport::scan_ports()?;
                    if !ports.is_empty() {
                        break;
                    }
                }
                sleep(Duration::from_millis(100));
            }
        }
    }

    match &cli.command {
        None | Some(Commands::Probe {}) => {
            if cli.usb {
                let ndevices = UsbTransport::scan_devices()?;
                log::info!(
                    "Found {ndevices} USB device{}",
                    match ndevices {
                        1 => "",
                        _ => "s",
                    }
                );
                for i in 0..ndevices {
                    let mut trans = UsbTransport::open_nth(i)?;
                    let chip = Flashing::get_chip(&mut trans)?;
                    log::info!("\tDevice #{i}: {chip}");
                }
            }
            if cli.serial {
                let ports = SerialTransport::scan_ports()?;
                let port_len = ports.len();
                log::info!(
                    "Found {port_len} serial port{}:",
                    match port_len {
                        1 => "",
                        _ => "s",
                    }
                );
                for p in ports {
                    log::info!("\t{p}");
                }
            }

            log::info!("hint: use `wchisp info` to check chip info");
        }
        Some(Commands::Info { chip }) => {
            let mut flashing = get_flashing(&cli)?;

            if let Some(expected_chip_name) = chip {
                flashing.check_chip_name(&expected_chip_name)?;
            }
            flashing.dump_info()?;
        }
        Some(Commands::Reset {}) => {
            let mut flashing = get_flashing(&cli)?;

            let _ = flashing.reset();
        }
        Some(Commands::Erase {}) => {
            let mut flashing = get_flashing(&cli)?;

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
            let mut flashing = get_flashing(&cli)?;

            flashing.dump_info()?;

            let mut binary = wchisp::format::read_firmware_from_file(path)?;
            extend_firmware_to_sector_boundary(&mut binary);
            log::info!("Firmware size: {}", binary.len());

            if *no_erase {
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

            if *no_verify {
                log::warn!("Skipping verify");
            } else {
                log::info!("Verifying...");
                flashing.verify(&binary)?;
                log::info!("Verify OK");
            }

            if *no_reset {
                log::warn!("Skipping reset");
            } else {
                log::info!("Now reset device and skip any communication errors");
                let _ = flashing.reset();
            }
        }
        Some(Commands::Verify { path }) => {
            let mut flashing = get_flashing(&cli)?;

            let mut binary = wchisp::format::read_firmware_from_file(path)?;
            extend_firmware_to_sector_boundary(&mut binary);
            log::info!("Firmware size: {}", binary.len());
            log::info!("Verifying...");
            flashing.verify(&binary)?;
            log::info!("Verify OK");
        }
        Some(Commands::Eeprom { command }) => {
            let mut flashing = get_flashing(&cli)?;

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
                Some(EepromCommands::Erase {}) => {
                    flashing.reidenfity()?;

                    log::info!("Erasing EEPROM(Data Flash)...");
                    flashing.erase_data()?;
                    log::info!("EEPROM erased");
                }
                Some(EepromCommands::Write { path, no_erase }) => {
                    flashing.reidenfity()?;

                    if *no_erase {
                        log::warn!("Skipping erase");
                    } else {
                        log::info!("Erasing EEPROM(Data Flash)...");
                        flashing.erase_data()?;
                        log::info!("EEPROM erased");
                    }

                    let eeprom = std::fs::read(path)?;
                    log::info!("Read {} bytes from bin file", eeprom.len());
                    if eeprom.len() as u32 != flashing.chip.eeprom_size {
                        anyhow::bail!(
                            "EEPROM size mismatch: expected {}, got {}",
                            flashing.chip.eeprom_size,
                            eeprom.len()
                        );
                    }

                    log::info!("Writing EEPROM(Data Flash)...");
                    flashing.write_eeprom(&eeprom)?;
                    log::info!("EEPROM written");
                }
            }
        }
        Some(Commands::Config { command }) => {
            let mut flashing = get_flashing(&cli)?;

            match command {
                None | Some(ConfigCommands::Info {}) => {
                    flashing.dump_config()?;
                }
                Some(ConfigCommands::Reset {}) => {
                    flashing.reset_config()?;
                    log::info!(
                        "Config register restored to default value(non-protected, debug-enabled)"
                    );
                }
                Some(ConfigCommands::EnableDebug {}) => {
                    flashing.enable_debug()?;
                    log::info!("Debug mode enabled");
                }
                Some(ConfigCommands::DisableDebug {}) => {
                    flashing.disable_debug()?;
                    log::info!("Debug mode disabled");
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

fn get_flashing(cli: &Cli) -> Result<Flashing<'_>> {
    if cli.usb {
        Flashing::new_from_usb(cli.device)
    } else if cli.serial {
        Flashing::new_from_serial(cli.port.as_deref(), cli.baudrate)
    } else {
        unreachable!("No transport specified");
    }
}
