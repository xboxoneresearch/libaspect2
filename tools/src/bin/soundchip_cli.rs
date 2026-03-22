use indicatif::{ProgressIterator, ProgressStyle};
use libaspect2::Ft4232h;
use libaspect2::i2c::i2c_bitbang::I2cFtBitbang;
use libaspect2::i2c::isd9160::{self, Isd9160, Isd9160Sounds};
use std::io::{Read, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    simple_logger::init_with_level(log::Level::Warn)?;

    let device = Ft4232h::with_description("Facet2 FabA+ C")?;
    let i2c_if = I2cFtBitbang::new(device);
    let mut isd = Isd9160::new(i2c_if);

    isd.init();
    isd.stop();

    if false {
        isd.play_sound(Isd9160Sounds::NO_DISC);
    } else {
        let mut buf = vec![0u8; isd9160::READ_CHUNK_SIZE];
        println!("Reading flash...");
        let mut file = std::fs::File::create("flash.bin")?;
        for _ in (0..isd.flash_size())
            .progress()
            .with_style(
                ProgressStyle::default_spinner()
                .template("[{elapsed_precise}, eta:{eta}] {bar:40.cyan/blue} {bytes} / {total_bytes} ({binary_bytes_per_sec})")
                .unwrap()
            )
            .step_by(buf.len())
        {
            isd.read_exact(&mut buf)?;
            file.write_all(&buf)?;
        }
    }

    Ok(())
}
