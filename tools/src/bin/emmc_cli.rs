use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use libaspect2::prelude::*;
use libaspect2::spi::backend::SpiBackend;
use libaspect2::spi::backend::ftdi::FtdiBackend;
use libaspect2::spi::emmc_reader::EmmcReader;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

const MAX_NAND_PAGES: u32 = 0x9E0000;
const BLOCK_SIZE: usize = 512;
const CHUNK_PAGES: u32 = 128; // read 128 pages (64 KB) per CMD18

#[derive(Parser, Clone, PartialEq, Eq, Debug)]
struct FileOptions {
    /// File to read / write
    file: PathBuf,
}

#[derive(Subcommand, Clone, PartialEq, Debug)]
enum Command {
    /// Read eMMC
    Read(FileOptions),
    /// Write eMMC
    Write(FileOptions),
    /// Dump SMC fuses
    DumpFuses,
    /// Reset SMC
    Reset,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
/// Arasan eMMC SPI tool
struct Args {
    #[command(subcommand)]
    op: Command,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        Command::Read(opts) => {
            // Initialize the device
            println!("Initializing device...");
            if let Err(e) = reader.init() {
                return Err(anyhow::anyhow!("Error initializing device: {}", e).into());
            }

            println!("\nDevice initialized successfully!");

            let total_bytes = MAX_NAND_PAGES as u64 * BLOCK_SIZE as u64;
            let mut file = File::create(opts.file)?;
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
        Command::Write(opts) => {
            // Initialize the device
            println!("Initializing device...");
            if let Err(e) = reader.init() {
                return Err(anyhow::anyhow!("Error initializing device: {}", e).into());
            }

            println!("\nDevice initialized successfully!");

            let mut file = File::open(opts.file)?;
            let total_bytes = file.metadata()?.len();
            assert!(total_bytes.is_multiple_of(BLOCK_SIZE as u64));
            let total_pages = (total_bytes / BLOCK_SIZE as u64) as u32;

            let mut chunk_buf = vec![0u8; CHUNK_PAGES as usize * BLOCK_SIZE];

            // Write eMMC pages
            println!(
                "Writing eMMC ({} pages, {:.2} GiB)...",
                total_pages,
                total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            );

            let pb = ProgressBar::new(total_bytes);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}, eta:{eta}] {bar:40.cyan/blue} {bytes} / {total_bytes} ({binary_bytes_per_sec})")
                    .unwrap()
            );

            let mut page = 0u32;
            while page < total_pages {
                let remaining = total_pages - page;
                let count = remaining.min(CHUNK_PAGES);
                let byte_count = count as usize * BLOCK_SIZE;

                file.read_exact(&mut chunk_buf[..byte_count])?;
                reader.write_pages(page, &chunk_buf[..byte_count], count)?;

                pb.inc(byte_count as u64);
                page += count;
            }

            pb.finish_with_message("done");
        }
    }

    println!("\nAll operations completed successfully!");

    Ok(())
}
