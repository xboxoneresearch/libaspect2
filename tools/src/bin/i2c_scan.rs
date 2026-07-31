use std::time::Duration;

use clap::Parser;
use embedded_hal::i2c::I2c;
use libaspect2::Ft4232h;
use libaspect2::i2c::i2c_bitbang::I2cFtBitbang;

fn parse_maybe_hex(s: &str) -> Result<u8, std::num::ParseIntError> {
    if s.starts_with("0x") || s.starts_with("0X") {
        u8::from_str_radix(&s[2..], 16)
    } else {
        u8::from_str_radix(s, 10)
    }
}

#[derive(Parser)]
#[command(version, about = "Scan the I2C bus via FTDI bitbang and print an i2cdetect-style table")]
struct Args {
    /// FTDI device description string
    #[arg(long, default_value = "Facet2 FabA+ C")]
    device: String,

    #[arg(long, value_parser = parse_maybe_hex)]
    address: Option<u8>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let device = Ft4232h::with_description(&args.device)?;
    let mut i2c = I2cFtBitbang::new(device);

    if let Some(address) = args.address {
        let mut index = 0;
        loop {
            match i2c.write(address, &[]) {
                Ok(_) => {
                    println!("{index:08} Got Response!");
                    index += 1;
                },
                Err(_) => {
                    println!("...")
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        return Ok(());
    }

    println!("     0  1  2  3  4  5  6  7  8  9  a  b  c  d  e  f");
    for row in 0..8u8 {
        print!("{:02x}: ", row << 4);
        for col in 0..16u8 {
            let addr = (row << 4) | col;
            if !(0x03..=0x77).contains(&addr) {
                print!("   ");
                continue;
            }
            match i2c.write(addr, &[]) {
                Ok(()) => print!("{addr:02x} "),
                Err(_) => print!("-- "),
            }
        }
        println!();
    }

    Ok(())
}
