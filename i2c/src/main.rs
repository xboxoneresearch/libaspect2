mod i2c_bitbang;
mod i2c_bitbang2;
mod isd9160;

use i2c_bitbang::I2cFtBitbang;
use i2c_bitbang2::I2cFtBitbang2;
use isd9160::{Isd9160I2c, Isd9160Sounds};
use libftd2xx::Ft4232h;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    simple_logger::init_with_level(log::Level::Warn)?;

    let device = Ft4232h::with_description("Facet2 FabA+ C")?;

    let SCL_PIN = 6u8; // CDBUS6
    let SDA_PIN = 7u8; // CDBUS7

    let i2c_if = I2cFtBitbang::new(device, SCL_PIN, SDA_PIN);

    let mut isd9160 = Isd9160I2c::new(i2c_if);

    isd9160.init();
    isd9160.stop();

    if false {
        isd9160.play_sound(Isd9160Sounds::NO_DISC);
    } else {
        println!("Reading flash...");
        let mut file = std::fs::File::create("flash.bin")?;
        isd9160.read_flash(Box::new(&mut file));
    }

    Ok(())
}
