use anyhow::Result;

use wchisp::Flashing;

fn main() -> Result<()> {
    let _ = simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    );

    let mut flashing = Flashing::new_from_usb()?;

    flashing.dump_info()?;
    flashing.unprotect()?;
    flashing.erase_code(4)?;

    flashing.flash(&b"/tmp/wchisp.bin"[..])?;

    Ok(())
}
