use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use libaspect2::prelude::*;
use libaspect2::spi::backend::SpiBackend;
use libaspect2::spi::backend::ftdi::FtdiBackend;
use libaspect2::spi::emmc_reader::EmmcReader;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use sdmmc_core::register::{ExtCsd, Cid};

const MAX_NAND_PAGES: u32 = 0x9E0000;
const BLOCK_SIZE: usize = 512;
const CHUNK_PAGES: u32 = 128; // read 128 pages (64 KB) per CMD18

fn parse_maybe_hex(arg: &str) -> Result<u64, std::num::ParseIntError> {
    if arg.starts_with("0x") || arg.starts_with("0X") {
        u64::from_str_radix(&arg[2..], 16)
    }
    else {
        u64::from_str_radix(arg, 10)
    }
}

#[derive(Parser, Clone, PartialEq, Eq, Debug)]
struct FileOptions {
    /// File to read / write
    file: PathBuf,
    /// Offset to start reading, beginning if not provided
    #[arg(value_parser = parse_maybe_hex)]
    offset: u64,
    /// Length to read, until EOF is not provided
    #[arg(value_parser = parse_maybe_hex)]
    length: u64,
}

#[derive(Subcommand, Clone, PartialEq, Debug)]
enum Command {
    /// Get eMMC info
    Info,
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
        Command::Info => {
            // Initialize the device
            println!("Initializing device...");
            if let Err(e) = reader.init() {
                return Err(anyhow::anyhow!("Error initializing device: {}", e).into());
            }

            println!("\nDevice initialized successfully!");

            let mut ext_csd = [0u8; 512];
            reader.read_ext_csd(&mut ext_csd)?;
            ext_csd.reverse();

            let cid = reader.cid().clone();
            let cid_bytes: Vec<u8> = [cid[3].to_be_bytes(), cid[2].to_be_bytes(), cid[1].to_be_bytes(), cid[0].to_be_bytes()].as_flattened().to_vec();
            let cid_parsed = Cid::try_from_bytes(&cid_bytes);
            let ext_csd_parsed = ExtCsd::try_from_inner(ext_csd);
            println!("CID: {cid_parsed:?}");
            println!("EXT CSD: {ext_csd_parsed:?}");
        }
        Command::Read(opts) => {
            // Initialize the device
            println!("Initializing device...");
            if let Err(e) = reader.init() {
                return Err(anyhow::anyhow!("Error initializing device: {}", e).into());
            }

            let start_page = match opts.offset {
                0 => 0,
                other => other / BLOCK_SIZE as u64
            };

            let page_count = match opts.length {
                0 => MAX_NAND_PAGES as u64 - start_page,
                other => other / BLOCK_SIZE as u64
            };

            println!("\nDevice initialized successfully!");

            let total_bytes = page_count * BLOCK_SIZE as u64;
            let mut file = File::create(opts.file)?;
            let mut chunk_buf = vec![0u8; CHUNK_PAGES as usize * BLOCK_SIZE];

            // Read eMMC pages
            println!(
                "Reading eMMC ({} pages, {:.2} GiB)...",
                page_count,
                total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            );

            let pb = ProgressBar::new(total_bytes);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}, eta:{eta}] {bar:40.cyan/blue} {bytes} / {total_bytes} ({binary_bytes_per_sec})")
                    .unwrap()
            );

            let mut page = start_page as u32;
            while page < (start_page + page_count) as u32 {
                let remaining = ((start_page + page_count) - page as u64) as u32;
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
