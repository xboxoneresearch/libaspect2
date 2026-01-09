use std::fmt::Display;
use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::time::Duration;
use std::path::PathBuf;
use anyhow::{anyhow, Result};
use binrw::{
    binrw,    // #[binrw] attribute
    BinRead,  // trait for reading
};
use clap::{Parser, Subcommand, ValueEnum};
use indicatif::{ProgressStyle, ProgressBar};
use i2c::{I2cFtBitbang, Ft4232h};
use stm32_bootloader_client::{ProtocolVersion, Stm32, Stm32i2c};

/*
```
+-----------------------+ 0x0800_0000
| Preloader Vector      |
| & Code                |
|  (2 KB-32 bytes)      |
+-----------------------+ 0x0800_07E0
| Tombstone info IAPL   |
| (32 bytes, fixed info)|
+-----------------------+ 0x0800_0800
| Tombstone info UAPP   |
| (32 bytes, fixed info)|
+-----------------------+ 0x0800_0820
| User App Vector       |
| & Code                |
| (up to 30KB-32 bytes) |
+-----------------------+ 0x0800_8000 (end of flash)
```

* **Preloader code**:   `0x0800_0000 .. 0x0800_07E0`
*   **Tombstone IAPL**: `0x0800_07E0 .. 0x0800_0800`
*   **Tombstone UAPP**: `0x0800_0800 .. 0x0800_0820`
* **User code**:        `0x0800_0A00 .. 0x0800_8000`
*/

const TOMBSTONE_IAPL_MAGIC: &[u8; 4] = b"IAPL";
const TOMBSTONE_UAPP_MAGIC: &[u8; 4] = b"UAPP";

const PAGE_SZ: usize = 0x800; // 2KB

const FLASH_BASE: usize = 0x0800_0000;
const _FLASH_SZ: usize = 0x8000; // 32KB
const TOMBSTONE_SZ: usize = 0x20; // 32 bytes

const SECTION_PRELOADER_SZ: usize = PAGE_SZ; // 2KB
const SECTION_USERAPP_SZ: usize = _FLASH_SZ - SECTION_PRELOADER_SZ;

// Start of IAPL / Preloader firmware binary
const PRELOADER_OFFSET: usize = FLASH_BASE;
const PRELOADER_SZ: usize = SECTION_PRELOADER_SZ - TOMBSTONE_SZ;
const TOMBSTONE_IAPL_OFFSET: usize = PRELOADER_OFFSET + PRELOADER_SZ;

// Start of userapp firmware binary
const TOMBSTONE_UAPP_OFFSET: usize = TOMBSTONE_IAPL_OFFSET + TOMBSTONE_SZ;
const USERAPP_OFFSET: usize = TOMBSTONE_UAPP_OFFSET + 0x200;
const USERAPP_SZ: usize = SECTION_USERAPP_SZ - 0x200;

const STM32_BOOTLOADER_I2C_ADDR: u8 = 0x56;

#[binrw]
#[brw(little)]
#[derive(Debug, PartialEq)]
struct Tombstone {
    /* 0x00 */ magic: [u8; 4],
    /* 0x04 */ ver_major: u16,
    /* 0x06 */ ver_minor: u16,
    /* 0x08 */ size: u16,
    /* 0x0A */ crc: u32,
    /* 0x0E */ reserved: [u8; 0x12],
    /* 0x20 total */
}

impl Tombstone {
    pub fn magic(&self) -> String {
        String::from_utf8_lossy(&self.magic).to_string()
    }
}

impl Display for Tombstone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Version={}.{}, Size={:#04X}, CRC32={:#04X}", self.ver_major, self.ver_minor, self.size, self.crc)
    }
}

// Macro: convert absolute address to page index
macro_rules! page_for_offset {
    ($addr:expr) => {
        (($addr - FLASH_BASE) / PAGE_SZ)
    };
}

// Macro: compute number of pages required for size
macro_rules! pagecount_for_size {
    ($sz:expr) => {
        (($sz + PAGE_SZ - 1) / PAGE_SZ)
    };
}


#[derive(Parser)]
#[command(name = "aspect2-stm32-updater", version = "1.0")]
struct Args {
    /// Command to execute
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Flash STM32 chip
    Flash {
        /// Section to flash
        section: Section,
        /// Firmware binary
        binary: PathBuf,
    },
    Read {
        /// Section to read
        section: Section,
        /// Output binary
        binary: PathBuf,
    },
    /// Retrieve metadata of currently flashed firmware components
    Info,
    /// Wipe the whole flash memory
    Wipe
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Section {
    Preloader,
    UserApp,
}

fn to_tombstone_struct(data: &[u8; TOMBSTONE_SZ]) -> Tombstone {
    Tombstone::read(&mut Cursor::new(data)).unwrap()
}

fn main() -> Result<()> {
    let args = Args::parse();

    fn delay(nanos: u64) {
        std::thread::sleep(Duration::from_nanos(nanos));
    }

    let dev = Ft4232h::with_description("Facet2 FabA+ C")?;
    let mut i2c_if = I2cFtBitbang::new(dev);

    let mut config = stm32_bootloader_client::Config::i2c_address(STM32_BOOTLOADER_I2C_ADDR);
    config.mass_erase_max_ns = Duration::from_secs(1).as_nanos() as u64;
    let mut stm32 = Stm32::new(Stm32i2c::new(&mut i2c_if, config), ProtocolVersion::Version1_1);

    match args.command {
        Command::Read { section, binary} => {
            let (offset, size) = match section {
                Section::Preloader => (PRELOADER_OFFSET, SECTION_PRELOADER_SZ),
                Section::UserApp => (TOMBSTONE_UAPP_OFFSET, SECTION_USERAPP_SZ),
            };

            let chip_id = stm32.get_chip_id()?;
            println!("[+] Chip ID: 0x{chip_id:x}");

            let progress = ProgressBar::new(size as u64)
                .with_style(
                    ProgressStyle::default_spinner()
                        .template("[{elapsed_precise}, eta:{eta}] {msg} {bar:40.cyan/blue} {bytes} / {total_bytes} ({binary_bytes_per_sec})")
                        .unwrap()
                );

            let mut filebuf = vec![0u8; size];
            println!("[+] Reading firmware...");
            progress.set_message("Reading");
            let mut position = 0;
            for i in (offset..offset + size).step_by(0x80) {
                stm32.read_memory(i as u32, &mut filebuf[position..position + 0x80])?;
                position += 0x80;
                progress.set_position(position as u64);
            }

            let success = stm32.verify_crc(offset as u32, &filebuf);
            if let Err(e) = success {
                println!("[!] Verification failed: {e:?}...");
                return Err(anyhow!("Verification failed"));
            }

            let mut f = File::open(binary)?;
            f.write_all(&filebuf)?;
            println!("[*] Done");
        },
        Command::Flash { binary, section } => {
            if !binary.exists() {
                return Err(anyhow!("Binary file does not exist"));
            }

            let mut file = std::fs::File::open(&binary)?;
            let mut filebuf = vec![];
            file.read_to_end(&mut filebuf)?;
            println!("* Using File={:?}, Size={} bytes",
                binary.file_name().ok_or(anyhow!("Reading filename failed")),
                filebuf.len()
            );

            let (offset, size) = match section {
                Section::Preloader => (PRELOADER_OFFSET, Some(SECTION_PRELOADER_SZ)),
                Section::UserApp => (TOMBSTONE_UAPP_OFFSET, None),
            };

            // We do not have a fixed size of userapp
            let size = size.unwrap_or(filebuf.len());

            let start_page = page_for_offset!(offset);
            let page_count = pagecount_for_size!(size);
            println!("[*] About to write offset {:#08X} - {:#08X} ({:#X} bytes)", offset, offset + size, size);
            println!("[*] Start page: {start_page}, count: {page_count}");

            if filebuf.len() != size as usize {
                return Err(anyhow!("Expected firmware size {:#08X}, got: {:#08X}", size, filebuf.len()));
            }

            let chip_id = stm32.get_chip_id()?;
            println!("[+] Chip ID: 0x{chip_id:x}");

            let progress = ProgressBar::new(filebuf.len() as u64)
                .with_style(
                    ProgressStyle::default_spinner()
                        .template("[{elapsed_precise}, eta:{eta}] {msg} {bar:40.cyan/blue} {bytes} / {total_bytes} ({binary_bytes_per_sec})")
                        .unwrap()
                );

            println!("[+] Erasing flash...");
            stm32.erase_pages(start_page as u16, page_count as u16, &mut delay)?;

            println!("[+] Writing firmware...");
            progress.set_message("Writing");
            stm32.write_bulk(offset as u32, &filebuf, |p|{
                progress.set_position(p.bytes_complete as u64);
            })?;

            println!("[+] Verifying firmware...");
            progress.set_message("Verifying");
            let success = stm32.verify_crc(offset as u32, &filebuf);

            if let Err(e) = success {
                // So bootloader can start after power toggle
                println!("[!] Verification failed: {e:?}, erasing flash...");
                stm32.erase_flash(&mut delay)?;
            }
            println!("[*] Done");
        },
        Command::Wipe => {
            println!("[!] Wiping flash..");
            stm32.erase_flash(&mut delay)?;
            println!("[*] Done");
        },
        Command::Info => {
            let mut out = [0; TOMBSTONE_SZ];

            for (ts_offset, magic, data_offset) in [
                (TOMBSTONE_IAPL_OFFSET, TOMBSTONE_IAPL_MAGIC, PRELOADER_OFFSET), (TOMBSTONE_UAPP_OFFSET, TOMBSTONE_UAPP_MAGIC, USERAPP_OFFSET)
            ] {
                stm32.read_memory(ts_offset as u32, &mut out)?;
                let header = to_tombstone_struct(&out);
                if &header.magic == magic {
                    let hash_result = stm32.get_memory_checksum(data_offset as u32, header.size as u32);

                    let hash_valid = match hash_result {
                        Ok(chksum) => chksum == header.crc,
                        Err(_) => false,
                    };
                    println!("Magic '{}' @ {ts_offset:#08X}", header.magic());
                    println!("{header}");

                    println!("Hash valid: {}", if hash_valid { "✅" } else { "❌" });
                } else {
                    eprintln!("No firmware / tombstone found @ {ts_offset:#08X}");
                }
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_for_offset() {
        assert_eq!(page_for_offset!(0x0800_0000), 0);
        assert_eq!(page_for_offset!(0x0800_0800), 1);
        assert_eq!(page_for_offset!(0x0800_0820), 1);
        assert_eq!(page_for_offset!(0x0800_7800), 15);
    }

    #[test]
    fn test_pagecount_for_size() {
        assert_eq!(pagecount_for_size!(0), 0);
        assert_eq!(pagecount_for_size!(0x100), 1);
        assert_eq!(pagecount_for_size!(0x400), 1);
        assert_eq!(pagecount_for_size!(0x500), 1);
        assert_eq!(pagecount_for_size!(0x555), 1);
        assert_eq!(pagecount_for_size!(0x800), 1);
        assert_eq!(pagecount_for_size!(0x1000), 2);
        assert_eq!(pagecount_for_size!(0x1001), 3);
    }

    #[test]
    fn test_ts_struct_from_bytes() {
        let data: [u8; TOMBSTONE_SZ] = [
            0x49, 0x41, 0x50, 0x4C, 0x01, 0x00, 0x02, 0x00, 0x0B, 0xB0, 0x67, 0x45, 0x23, 0x01, 0x00, 0x00, 
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
        ];

        assert_eq!(size_of::<Tombstone>(), TOMBSTONE_SZ);
        let ts = to_tombstone_struct(&data);
        assert_eq!(ts.magic(), "IAPL");
        assert_eq!(ts.ver_major, 1);
        assert_eq!(ts.ver_minor, 2);
        assert_eq!(ts.size, 0xB00B);
        assert_eq!(ts.crc, 0x01234567);
    }
}