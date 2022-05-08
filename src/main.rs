use std::{thread::sleep, time::Duration};

use anyhow::Result;
use clap::StructOpt;

use wchisp::Flashing;

#[derive(clap::Parser)]
#[clap(
    name = "WCHISP Tool CLI",
    about = "Command-line implementation of the WCHISPTool in Rust, by the ch32-rs team",
    author = "Andelf <andelf@gmail.com>"
)]
enum Cli {
    /// Get info about current connected chip
    Info {},
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
    Verify {
        path: String,
    },
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
        Cli::Info {} => {
            flashing.dump_info()?;
        }
        Cli::Reset {} => {
            flashing.reset()?;
        }
        Cli::Erase {} => {
            unimplemented!()
        }
        Cli::Unprotect {} => {
            // force unprotect, ignore check
            flashing.unprotect(true)?;
        }
        Cli::Flash { path } => {
            flashing.dump_info()?;
            let binary = wchisp::format::read_firmware_from_file(path)?;
            log::info!("Firmware size: {}", binary.len());
            flashing.flash(&binary)?;
            sleep(Duration::from_secs(1));
            flashing.verify(&binary)?;
            sleep(Duration::from_secs(1));
            flashing.reset()?
        }
        Cli::Verify { path } => {
            let binary = wchisp::format::read_firmware_from_file(path)?;
            log::info!("Firmware size: {}", binary.len());
            flashing.verify(&binary)?;
            log::info!("Verified!");
        }
        _ => unimplemented!(),
    }

    Ok(())
}
