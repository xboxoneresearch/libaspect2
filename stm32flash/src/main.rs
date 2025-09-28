use std::env;
use std::time::Duration;
use indicatif::ProgressStyle;
use indicatif::{ProgressBar};
use i2c::I2cFtBitbang;
use i2c::Ft4232h;
use stm32_bootloader_client::{ProtocolVersion, Stm32, Stm32i2c};

const FLASH_ADDR: u32 = 0x0800_0000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    let filename = args.get(1)
        .ok_or("[-] No filename provided")?;

    let firmware_data = std::fs::read(filename)
        .map_err(|_|format!("File '{filename}' not found!"))?;
    println!("* Using File={filename}, Size={} bytes", firmware_data.len());

    let dev = Ft4232h::with_description("Facet2 FabA+ C")?;
    let mut i2c_if = I2cFtBitbang::new(dev);

    let config = stm32_bootloader_client::Config::i2c_address(0x56);
    let mut stm32 = Stm32::new(Stm32i2c::new(&mut i2c_if, config), ProtocolVersion::Version1_1);

    let chip_id = stm32.get_chip_id()?;
    println!("[+] Found chip ID: 0x{chip_id:x}");

    fn delay(nanos: u64) {
        std::thread::sleep(Duration::from_nanos(nanos));
    }

    let progress = ProgressBar::new(firmware_data.len() as u64)
        .with_style(
            ProgressStyle::default_spinner()
                .template("[{elapsed_precise}, eta:{eta}] {msg} {bar:40.cyan/blue} {bytes} / {total_bytes} ({binary_bytes_per_sec})")
                .unwrap()
        );

    println!("[+] Erasing flash...");
    stm32.erase_flash(&mut delay)?;

    println!("[+] Writing firmware...");
    progress.set_message("Writing");
    stm32.write_bulk(FLASH_ADDR, &firmware_data, |p|{
        progress.set_position(p.bytes_complete as u64);
    })?;

    println!("[+] Verifying firmware...");
    progress.set_message("Verifying");
    let success = stm32.verify(FLASH_ADDR, &firmware_data, |p|{
        progress.set_position(p.bytes_complete as u64);
    });

    if let Err(e) = success {
        // So bootloader can start after power toggle
        println!("[!] Verification failed: {e:?}, erasing flash...");
        stm32.erase_flash(&mut delay)?;
    }

    Ok(())
}