use std::time::Duration;

use anyhow::{Result, anyhow};
use libaspect2::Ft4232h;
use libaspect2::i2c::i2c_bitbang::I2cFtBitbang;
use libaspect2::eh_i2c::{I2c, Operation};
use rand::prelude::*;

fn main() -> Result<()> {
    let device = Ft4232h::with_description("Facet2 FabA+ C")?;
    let mut i2c_if = I2cFtBitbang::new(device);

    let mut rng = rand::rng();

    let mut code: u16;
    let mut typ: u8;
    let mut seg_idx: u8;

    let mut buf = [0u8; 6];

    loop {
        code = rand::random_range(0x0000..0xFFFF);
        typ = *[0x10, 0x30, 0x70, 0xF0].choose(&mut rng).unwrap();
        seg_idx = rand::random_range(0..4);

        println!("CODE={code:#04x} TYPE={typ:#02x} SEG={seg_idx:#02x}");

        buf[0] = 0x20;                          // REG: Digit_0
        buf[1] = (code & 0xF) as u8;            // Digit_0
        buf[2] = ((code & 0xF0) >> 4) as u8;    // Digit_1
        buf[3] = ((code & 0xF00) >> 8) as u8;   // Digit_2
        buf[4] = ((code & 0xF000) >> 12) as u8; // Digit_3
        buf[5] = typ | seg_idx;                 // Segment

        i2c_if.transaction(0x38, &mut [
            Operation::Write(&[0x04, 0x20]), // configuration clear packet
            Operation::Write(&buf)           // digit / segment data
        ]).map_err(|e|anyhow!(e))?;

        std::thread::sleep(Duration::from_millis(100));
    }

    Ok(())
}
