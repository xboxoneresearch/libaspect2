use std::fs::File;
use std::io::{self, Read, Write};
use std::time::Duration;
use clap::{Parser, Subcommand};

#[repr(u8)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum OpCode {
    Read = 1,
    Write,
    Erase,
    Reset,
    DumpFuses
}

/// Simple CLI for picoemmc USB CDC device
#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    /// Serial port device (e.g. /dev/ttyACM0)
    #[arg(short, long, default_value = "/dev/ttyACM0")]
    port: String,
    /// Baud rate (not used by USB CDC, but required by serialport)
    #[arg(short, long, default_value_t = 115200)]
    baud: u32,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Read a 512-byte page from device
    Read {
        lba: u32,
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Write a 512-byte page to device
    Write {
        lba: u32,
        #[arg(short, long)]
        input: String,
    },
    /// Erase a range of pages
    Erase {
        start: u32,
        len: u32,
    },
    /// Reset device
    Reset,
    /// Dump fuses
    DumpFuses,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut port = serialport::new(&cli.port, cli.baud)
        .timeout(Duration::from_millis(1000))
        .open()?;

    match cli.command {
        Commands::Read { lba, output } => {
            let mut cmd = vec![OpCode::Read as u8];
            cmd.extend(&lba.to_le_bytes());
            port.write_all(&cmd)?;
            let mut buf = [0u8; 512];
            port.read_exact(&mut buf)?;
            if let Some(path) = output {
                std::fs::write(path, &buf)?;
            } else {
                io::stdout().write_all(&buf)?;
            }
        }
        Commands::Write { lba, input } => {
            let mut data = [0u8; 512];
            File::open(&input)?.read_exact(&mut data)?;
            let mut cmd = vec![OpCode::Write as u8];
            cmd.extend(&lba.to_le_bytes());
            cmd.extend(&data);
            port.write_all(&cmd)?;
            let mut resp = [0u8; 3];
            port.read_exact(&mut resp)?;
            print!("{}", std::str::from_utf8(&resp).unwrap_or("?"));
        }
        Commands::Erase { start, len } => {
            let mut cmd = vec![OpCode::Erase as u8];
            cmd.extend(&start.to_le_bytes());
            cmd.extend(&len.to_le_bytes());
            port.write_all(&cmd)?;
            let mut resp = [0u8; 3];
            port.read_exact(&mut resp)?;
            print!("{}", std::str::from_utf8(&resp).unwrap_or("?"));
        }
        Commands::Reset => {
            port.write_all(&[OpCode::Reset as u8])?;
            let mut buf = [0u8; 16];
            let n = port.read(&mut buf)?;
            print!("{}", std::str::from_utf8(&buf[..n]).unwrap_or("?"));
        }
        Commands::DumpFuses => {
            port.write_all(&[OpCode::DumpFuses as u8])?;
            let mut buf = [0u8; 64];
            let n = port.read(&mut buf)?;
            print!("{}", std::str::from_utf8(&buf[..n]).unwrap_or("?"));
        }
    }
    Ok(())
}
