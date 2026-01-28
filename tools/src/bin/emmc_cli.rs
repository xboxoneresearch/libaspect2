use std::fs::File;
use std::io::Write;
use indicatif::{ProgressIterator, ProgressStyle};
use libaspect2::spi::emmc_reader::EmmcReader;
use libaspect2::spi::backend::ftdi::FtdiBackend;

const MAX_NAND_PAGES: u32 = 0x9E0000;
const PAGE_SIZE: usize = 512;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("eMMC SPI Reader - Initializing...\n");
    
    // Open FTDI device
    let backend = FtdiBackend::open("Facet2 FabA+ A")?;
    
    // Create reader with FTDI backend
    let mut reader = EmmcReader::new(backend);
    
    // Initialize the device
    println!("Initializing device...");
    reader.init()?;
    
    println!("\nDevice initialized successfully!");
    
    let mut file = File::create("dump.bin")?;

    let mut buf = [0u8; 512];
    // Read eMMC pages
    println!("Reading eMMC...");
    for page_num in (0..MAX_NAND_PAGES)
        .progress()
        .with_style(
            ProgressStyle::default_spinner()
            .template("[{elapsed_precise}, eta:{eta}] {bar:40.cyan/blue} {bytes} / {total_bytes} ({binary_bytes_per_sec})")
            .unwrap()
        )
        .step_by(PAGE_SIZE)
    {
        reader.read_page(page_num, &mut buf)?;
        file.write_all(&buf)?;
    }
    
    println!("\nAll operations completed successfully!");
    
    Ok(())
}
