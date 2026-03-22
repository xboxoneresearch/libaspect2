use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use libaspect2::prelude::*;
use libaspect2::spi::backend::SpiBackend;
use libaspect2::spi::backend::ftdi::FtdiBackend;
use libaspect2::spi::emmc_reader::EmmcReader;
use std::fs::File;
use std::io::Write;

const MAX_NAND_PAGES: u32 = 0x9E0000;
const BLOCK_SIZE: usize = 512;
const CHUNK_PAGES: u32 = 128; // read 128 pages (64 KB) per CMD18

#[derive(Subcommand, Clone, PartialEq, Debug)]
enum Command {
    Reset,
    Read,
    Write,
    DumpFuses,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    op: Command,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("eMMC SPI Reader");

    let args = Args::parse();

    // Open FTDI device
    let backend = FtdiBackend::open("Facet2 FabA+ A").expect("Failed to open FTDI backend");

    // Create reader with FTDI backend
    let mut reader = EmmcReader::new(backend, StdClock);

    match args.op {
        Command::Reset => {
            println!("Resetting device...");
            reader.backend.reset()?;
            return Ok(());
        }
        Command::DumpFuses => {
            println!("Dumping fuses...");
            reader.dump_fuses()?;
            return Ok(());
        }
        Command::Write | Command::Read => {
            if args.op == Command::Write {
                todo!("Learn to read first");
            }
            // Initialize the device
            println!("Initializing device...");
            if let Err(e) = reader.init() {
                println!("Error initializing device: {}", e);
                return Err(anyhow::anyhow!(":(").into());
            }

            println!("\nDevice initialized successfully!");

            let total_bytes = MAX_NAND_PAGES as u64 * BLOCK_SIZE as u64;
            let mut file = File::create("dump.bin")?;
            let mut chunk_buf = vec![0u8; CHUNK_PAGES as usize * BLOCK_SIZE];

            // Read eMMC pages
            println!(
                "Reading eMMC ({} pages, {:.2} GiB)...",
                MAX_NAND_PAGES,
                total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            );

            let pb = ProgressBar::new(total_bytes);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}, eta:{eta}] {bar:40.cyan/blue} {bytes} / {total_bytes} ({binary_bytes_per_sec})")
                    .unwrap()
            );

            let mut page = 0u32;
            while page < MAX_NAND_PAGES {
                let remaining = MAX_NAND_PAGES - page;
                let count = remaining.min(CHUNK_PAGES);
                let byte_count = count as usize * BLOCK_SIZE;

                reader.read_pages(page, &mut chunk_buf[..byte_count], count)?;
                file.write_all(&chunk_buf[..byte_count])?;

                pb.inc(byte_count as u64);
                page += count;
            }

            pb.finish_with_message("done");
        }
    }

    println!("\nAll operations completed successfully!");

    Ok(())
}
