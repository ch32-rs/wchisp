use std::{thread::sleep, time::Duration};

use anyhow::Result;
use clap::StructOpt;

use wchisp::Flashing;

/// Common options and logic when interfacing with a [Probe].
#[derive(clap::Parser, Debug)]
pub struct ProbeOptions {
    #[structopt(long)]
    pub chip: Option<String>,
}

#[derive(clap::Parser)]
#[clap(
    name = "WCHISP Tool CLI",
    about = "Command-line implementation of the WCHISPTool in Rust, by the ch32-rs team",
    author = "Andelf <andelf@gmail.com>"
)]
enum Cli {
    /// Get info about current connected chip
    Info {
        #[clap(flatten)]
        common: ProbeOptions,
    },
    /// Reset the target connected
    Reset {},
    /// Remove code flash protect(RDPR and WPR) and reset
    Unprotect {},
    /// Erase flash
    Erase {},
    /// Download to code flash and reset
    Flash {
        /// The path to the file to be downloaded to the code flash
        path: String,
    },
    /// Config CFG register
    Config {},
    /// Verify flash content
    Verify { path: String },
    /// Read EEPROM
    Eeprom {},
}

fn main() -> Result<()> {
    let _ = simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    );

    let matches = Cli::parse();
    let mut flashing = Flashing::new_from_usb()?;
    match matches {
        Cli::Info { common } => {
            if let Some(expected_chip_name) = common.chip {
                flashing.check_chip_name(&expected_chip_name)?;
            }
            flashing.dump_info()?;
        }
        Cli::Reset {} => {
            let _ = flashing.reset();
        }
        Cli::Erase {} => {
            let sectors = flashing.chip.flash_size / 1024;
            flashing.erase_code(sectors)?;
        }
        Cli::Unprotect {} => {
            // force unprotect, ignore check
            flashing.unprotect(true)?;
        }
        // WRITE_CONFIG => READ_CONFIG => ISP_KEY => ERASE => PROGRAM => VERIFY => RESET
        Cli::Flash { path } => {
            flashing.dump_info()?;
            let mut binary = wchisp::format::read_firmware_from_file(path)?;

            extend_firmware_to_sector_boundary(&mut binary);
            log::info!("Firmware size: {}", binary.len());
            flashing.flash(&binary)?;

            sleep(Duration::from_secs(1));
            flashing.verify(&binary)?;
            sleep(Duration::from_secs(1));

            log::info!("Flash and Verify OK, now reset device");
            let _ = flashing.reset();
        }
        Cli::Verify { path } => {
            let binary = wchisp::format::read_firmware_from_file(path)?;
            log::info!("Firmware size: {}", binary.len());
            flashing.verify(&binary)?;
            log::info!("Verified!");
        }
        Cli::Eeprom {} => {
            // FIXME: cannot read?
            sleep(Duration::from_secs(1));
            let eeprom = flashing.dump_eeprom()?;
            log::info!("EEPROM size: {}", eeprom.len());
        }
        Cli::Config {  } => {
            flashing.reset_config()?;
        }
        _ => unimplemented!(),
    }

    Ok(())
}


fn extend_firmware_to_sector_boundary(buf: &mut Vec<u8>) {
    if buf.len() % 1024 != 0 {
        let remain = 1024 - (buf.len() % 1024);
        buf.extend_from_slice(&vec![0; remain]);
    }
}
