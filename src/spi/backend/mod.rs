use crate::error::Error;
/// Backend abstraction module - hardware-specific implementations
///
/// This module defines a common trait for SPI backends and provides
/// implementations for both FTDI and embedded-hal.
use crate::prelude::*;

pub mod eh;
#[cfg(feature = "ftdi")]
pub mod ftdi;

/// Common SPI backend trait
///
/// This trait abstracts the low-level SPI operations needed for the eMMC protocol.
/// It allows the same high-level protocol logic to work with different hardware backends.
pub trait SpiBackend {
    /// Execute a write transaction
    ///
    /// # Arguments
    /// * `register` - Target register address
    /// * `data` - 32-bit data to write (will be sent as little-endian)
    fn write_register<T: Into<u8>>(&mut self, register: T, data: u32) -> Result<(), Error>;

    /// Execute a read transaction
    ///
    /// # Arguments
    /// * `register` - Target register address
    ///
    /// # Returns
    /// 32-bit value read from register (little-endian)
    fn read_register<T: Into<u8>>(&mut self, register: T) -> Result<u32, Error>;

    /// Execute a read from data register
    ///
    /// # Arguments
    /// * `register` - Target register address
    /// * `buffer` - Buffer to store the data
    fn read_data<T: Into<u8>>(&mut self, register: T, buffer: &mut [u8]) -> Result<(), Error>;

    /// Execute a write to data register (bulk)
    ///
    /// # Arguments
    /// * `register` - Target register address
    /// * `buffer` - Data to write (will be sent in 4-byte chunks)
    fn write_data<T: Into<u8> + Copy>(&mut self, register: T, buffer: &[u8]) -> Result<(), Error> {
        for chunk in buffer.chunks(4) {
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_register(register, u32::from_le_bytes(word))?;
        }
        Ok(())
    }

    /// Set the SPI bus clock frequency in kHz.
    ///
    /// Default implementation is a no-op (for backends with fixed clocks).
    fn set_spi_clock(&mut self, _freq_khz: u32) -> Result<(), Error> {
        Ok(())
    }

    /// Reset the device
    fn reset(&mut self) -> Result<(), Error>;

    /// Initialize the SPI interface
    fn initialize(&mut self) -> Result<(), Error>;
}

/// Raw SPI backend trait for direct JEDEC SPI NOR flash access
///
/// Unlike `SpiBackend` (which frames all transfers as register read/writes for the
/// custom eMMC bridge IC), this trait exposes a plain half-duplex byte stream within
/// a single CS-asserted frame.  The NOR flash driver uses this to issue JEDEC opcodes
/// directly without any intermediate controller framing.
pub trait RawSpiBackend {
    /// Execute a CS-framed SPI transaction.
    ///
    /// Drives the bus as: assert CS → send `cmd` → send `write` → recv `read` → deassert CS.
    /// All three slices participate in one unbroken CS frame.  Any of them may be empty.
    fn spi_transaction(&mut self, cmd: &[u8], write: &[u8], read: &mut [u8]) -> Result<(), Error>;

    /// Set the SPI bus clock frequency in kHz.
    fn set_clock_freq(&mut self, freq_khz: u32) -> Result<(), Error>;

    /// Initialize the SPI interface (GPIO, MPSSE setup, reset sequence).
    fn initialize(&mut self) -> Result<(), Error>;

    /// Assert and release the hardware reset line.
    fn reset(&mut self) -> Result<(), Error>;
}

/// Helper trait for GPIO control (used by backends that need it)
pub trait GpioControl {
    /// Set chip select state
    fn set_chip_select(&mut self, asserted: bool) -> Result<(), Error>;

    /// Set reset pin state
    fn set_reset(&mut self, asserted: bool) -> Result<(), Error>;

    /// Set enable pin state
    fn set_enable(&mut self, enabled: bool) -> Result<(), Error>;
}
