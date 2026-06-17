use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use libaspect2::prelude::*;
use libaspect2::spi::backend::ftdi::FtdiBackend;
use libaspect2::spi::backend::{RawSpiBackend, SpiBackend};
use libaspect2::spi::emmc_flash::EmmcFlash;
use libaspect2::spi::nor_flash::NorFlash;
use libaspect2::spi::protocol::nor::JedecId;
use sdmmc_core::register::{Cid, ExtCsd};
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

// eMMC
const EMMC_MAX_PAGES: u32 = 0x9E0000;
const EMMC_BLOCK: usize = 512;
const EMMC_CHUNK: u32 = 128; // pages per multi-block transfer

// SPI NOR
const NOR_PAGE: usize = 256;
const NOR_SECTOR: usize = 4096;
const NOR_READ_CHUNK: usize = 64 * 1024;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn parse_maybe_hex(s: &str) -> Result<u64, std::num::ParseIntError> {
    if s.starts_with("0x") || s.starts_with("0X") {
        u64::from_str_radix(&s[2..], 16)
    } else {
        u64::from_str_radix(s, 10)
    }
}

fn pb(total: u64) -> ProgressBar {
    let bar = ProgressBar::new(total);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}, eta:{eta}] {bar:40.cyan/blue} {bytes}/{total_bytes} ({binary_bytes_per_sec})")
            .unwrap(),
    );
    bar
}

fn nor_capacity(id: &JedecId) -> Option<u64> {
    let code = id.device[1];
    (code != 0 && code != 0xFF).then(|| 1u64 << code)
}

// ---------------------------------------------------------------------------
// eMMC subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum EmmcOp {
    /// Show CID and EXT_CSD
    Info,
    /// Read eMMC to a file
    Read {
        file: PathBuf,
        /// Start offset in bytes (0x… for hex) [default: 0]
        #[arg(default_value = "0", value_parser = parse_maybe_hex)]
        offset: u64,
        /// Byte count to read (0 = full device) [default: 0]
        #[arg(default_value = "0", value_parser = parse_maybe_hex)]
        length: u64,
    },
    /// Write a file to eMMC
    Write {
        file: PathBuf,
        /// Start offset in bytes, must be 512-byte aligned [default: 0]
        #[arg(default_value = "0", value_parser = parse_maybe_hex)]
        offset: u64,
    },
    /// Dump Xbox SMC fuses via XIP registers
    DumpFuses,
    /// Assert and release the hardware reset line
    Reset,
}

#[derive(Parser, Debug)]
struct EmmcArgs {
    /// Clock frequency in MHz (≤25 Legacy, ≤52 High-Speed, >52 HS200 with tuning)
    #[arg(long, default_value = "50.0")]
    freq: f64,

    #[command(subcommand)]
    op: EmmcOp,
}

fn run_emmc(args: EmmcArgs, device: &str) -> anyhow::Result<()> {
    let backend = FtdiBackend::open(device)
        .map_err(|e| anyhow::anyhow!("Failed to open FTDI device {:?}: {e}", device))?;
    let mut flash = EmmcFlash::new(backend, StdClock);

    match args.op {
        EmmcOp::Reset => {
            SpiBackend::reset(&mut flash.backend)?;
            println!("Reset complete.");
            return Ok(());
        }

        EmmcOp::DumpFuses => {
            let fuses = flash.dump_fuses()?;
            println!("{fuses}");
            return Ok(());
        }

        EmmcOp::Info => {
            println!("Initializing eMMC at {:.0} MHz...", args.freq);
            flash.init_at_freq(args.freq)?;

            let mut ext_csd = [0u8; 512];
            flash.read_ext_csd(&mut ext_csd)?;
            ext_csd.reverse();

            let cid = flash.cid().clone();
            let cid_bytes: Vec<u8> = [
                cid[3].to_be_bytes(),
                cid[2].to_be_bytes(),
                cid[1].to_be_bytes(),
                cid[0].to_be_bytes(),
            ]
            .as_flattened()
            .to_vec();

            println!("CID:     {:?}", Cid::try_from_bytes(&cid_bytes));
            println!("EXT_CSD: {:?}", ExtCsd::try_from_inner(ext_csd));
        }

        EmmcOp::Read { file, offset, length } => {
            println!("Initializing eMMC at {:.0} MHz...", args.freq);
            flash.init_at_freq(args.freq)?;

            let start_page = offset / EMMC_BLOCK as u64;
            let page_count = if length == 0 {
                EMMC_MAX_PAGES as u64 - start_page
            } else {
                length / EMMC_BLOCK as u64
            };
            let total = page_count * EMMC_BLOCK as u64;

            println!(
                "Reading {:.2} GiB ({page_count} pages) → {:?}",
                total as f64 / (1u64 << 30) as f64,
                file
            );

            let mut out = File::create(&file)?;
            let mut buf = vec![0u8; EMMC_CHUNK as usize * EMMC_BLOCK];
            let bar = pb(total);
            let mut page = start_page as u32;

            while page < (start_page + page_count) as u32 {
                let remaining = ((start_page + page_count) - page as u64) as u32;
                let count = remaining.min(EMMC_CHUNK);
                let bytes = count as usize * EMMC_BLOCK;

                flash.read_pages(page, &mut buf[..bytes], count)?;
                out.write_all(&buf[..bytes])?;
                bar.inc(bytes as u64);
                page += count;
            }
            bar.finish_with_message("done");
        }

        EmmcOp::Write { file, offset } => {
            if offset % EMMC_BLOCK as u64 != 0 {
                anyhow::bail!("Offset {offset:#X} is not 512-byte aligned");
            }

            println!("Initializing eMMC at {:.0} MHz...", args.freq);
            flash.init_at_freq(args.freq)?;

            let mut f = File::open(&file)?;
            let total = f.metadata()?.len();

            if total % EMMC_BLOCK as u64 != 0 {
                anyhow::bail!("File size {total} is not a multiple of {EMMC_BLOCK} bytes");
            }

            let start_page = (offset / EMMC_BLOCK as u64) as u32;
            let total_pages = (total / EMMC_BLOCK as u64) as u32;

            println!(
                "Writing {:.2} GiB ({total_pages} pages) from {:?} at page {start_page}",
                total as f64 / (1u64 << 30) as f64,
                file
            );

            let mut buf = vec![0u8; EMMC_CHUNK as usize * EMMC_BLOCK];
            let bar = pb(total);
            let mut page = 0u32;

            while page < total_pages {
                let count = (total_pages - page).min(EMMC_CHUNK);
                let bytes = count as usize * EMMC_BLOCK;

                f.read_exact(&mut buf[..bytes])?;
                flash.write_pages(start_page + page, &buf[..bytes], count)?;
                bar.inc(bytes as u64);
                page += count;
            }
            bar.finish_with_message("done");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// SPI NOR subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum NorOp {
    /// Show JEDEC ID and derived flash capacity
    Info,
    /// Read flash contents to a file
    Read {
        file: PathBuf,
        /// Start offset in bytes (0x… for hex) [default: 0]
        #[arg(default_value = "0", value_parser = parse_maybe_hex)]
        offset: u64,
        /// Byte count to read (0 = derive from JEDEC density byte) [default: 0]
        #[arg(default_value = "0", value_parser = parse_maybe_hex)]
        length: u64,
    },
    /// Erase covered sectors then program flash from a file
    Write {
        file: PathBuf,
        /// Start offset in bytes, must be 4 KiB sector-aligned [default: 0]
        #[arg(default_value = "0", value_parser = parse_maybe_hex)]
        offset: u64,
    },
    /// Erase a byte range (addr and length must be 4 KiB aligned)
    Erase {
        #[arg(value_parser = parse_maybe_hex)]
        addr: u64,
        #[arg(value_parser = parse_maybe_hex)]
        length: u64,
    },
    /// Erase the entire chip
    ChipErase,
}

#[derive(Parser, Debug)]
struct NorArgs {
    /// SPI clock frequency in kHz
    #[arg(long, default_value = "10000")]
    spi_clock: u32,

    #[command(subcommand)]
    op: NorOp,
}

fn run_nor(args: NorArgs, device: &str) -> anyhow::Result<()> {
    let backend = FtdiBackend::open(device)
        .map_err(|e| anyhow::anyhow!("Failed to open FTDI device {:?}: {e}", device))?;
    let mut flash = NorFlash::new(backend, StdClock);

    let id = flash
        .init()
        .map_err(|e| anyhow::anyhow!("NOR init failed: {e}"))?;
    flash.backend.set_clock_freq(args.spi_clock)?;

    println!(
        "JEDEC ID: manufacturer={:#04X}  device={:#04X} {:#04X}",
        id.manufacturer, id.device[0], id.device[1]
    );
    if let Some(cap) = nor_capacity(&id) {
        println!("Capacity:  {} KiB ({} MiB)", cap / 1024, cap / (1024 * 1024));
    }

    match args.op {
        NorOp::Info => {} // already printed above

        NorOp::Read { file, offset, length } => {
            let length = if length == 0 {
                nor_capacity(&id)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Cannot derive flash size from JEDEC ID — pass an explicit length"
                        )
                    })?
                    .saturating_sub(offset)
            } else {
                length
            };

            println!(
                "Reading {:.2} MiB from {offset:#X} → {:?}",
                length as f64 / (1u64 << 20) as f64,
                file
            );

            let mut out = File::create(&file)?;
            let mut buf = vec![0u8; NOR_READ_CHUNK];
            let bar = pb(length);
            let mut pos = offset;

            while pos < offset + length {
                let chunk = ((offset + length) - pos).min(NOR_READ_CHUNK as u64) as usize;
                flash.read(pos as u32, &mut buf[..chunk])?;
                out.write_all(&buf[..chunk])?;
                bar.inc(chunk as u64);
                pos += chunk as u64;
            }
            bar.finish_with_message("done");
        }

        NorOp::Write { file, offset } => {
            if offset % NOR_SECTOR as u64 != 0 {
                anyhow::bail!("Write offset {offset:#X} must be 4 KiB sector-aligned");
            }

            let mut f = File::open(&file)?;
            let file_len = f.metadata()?.len();
            let sector_count = file_len.div_ceil(NOR_SECTOR as u64);

            println!(
                "Erasing {sector_count} sectors ({:.2} MiB) at {offset:#X}",
                sector_count as f64 * NOR_SECTOR as f64 / (1u64 << 20) as f64
            );
            let erase_bar = pb(sector_count * NOR_SECTOR as u64);
            for i in 0..sector_count {
                flash.sector_erase_4k((offset + i * NOR_SECTOR as u64) as u32)?;
                erase_bar.inc(NOR_SECTOR as u64);
            }
            erase_bar.finish_with_message("erase done");

            println!(
                "Programming {:.2} MiB from {:?}",
                file_len as f64 / (1u64 << 20) as f64,
                file
            );
            let write_bar = pb(file_len);
            let mut buf = [0u8; NOR_PAGE];
            let mut pos = 0u64;

            while pos < file_len {
                let chunk = (file_len - pos).min(NOR_PAGE as u64) as usize;
                buf[..chunk].fill(0xFF); // pad last partial page
                f.read_exact(&mut buf[..chunk])?;
                flash.page_program((offset + pos) as u32, &buf[..chunk])?;
                write_bar.inc(chunk as u64);
                pos += chunk as u64;
            }
            write_bar.finish_with_message("write done");
        }

        NorOp::Erase { addr, length } => {
            if addr % NOR_SECTOR as u64 != 0 || length % NOR_SECTOR as u64 != 0 {
                anyhow::bail!(
                    "Erase addr {addr:#X} and length {length:#X} must both be 4 KiB aligned"
                );
            }
            let sector_count = length / NOR_SECTOR as u64;
            println!("Erasing {sector_count} sectors at {addr:#X}");
            let bar = pb(length);
            for i in 0..sector_count {
                flash.sector_erase_4k((addr + i * NOR_SECTOR as u64) as u32)?;
                bar.inc(NOR_SECTOR as u64);
            }
            bar.finish_with_message("done");
        }

        NorOp::ChipErase => {
            println!("Chip erase — this may take several minutes...");
            flash.chip_erase()?;
            println!("Chip erase complete.");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Top-level
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum FlashKind {
    /// Arasan eMMC via SPI bridge controller
    Emmc(EmmcArgs),
    /// JEDEC SPI NOR flash (direct on the SPI bus, no bridge)
    Nor(NorArgs),
}

#[derive(Parser, Debug)]
#[command(version, about = "Unified flash tool — eMMC and SPI NOR via FTDI")]
struct Args {
    /// FTDI device description string
    #[arg(long, default_value = "Facet2 FabA+ A")]
    device: String,

    #[command(subcommand)]
    kind: FlashKind,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.kind {
        FlashKind::Emmc(a) => run_emmc(a, &args.device),
        FlashKind::Nor(a) => run_nor(a, &args.device),
    }
}
