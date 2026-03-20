use super::protocol::commands::Register;
use super::protocol::transaction::TransactionType;
/// Backend abstraction module - hardware-specific implementations
///
/// This module defines a common trait for SPI backends and provides
/// implementations for both FTDI and embedded-hal.
use crate::error::Error;

pub mod ftdi;

#[cfg(feature = "embedded-hal")]
pub mod eh0;
#[cfg(feature = "embedded-hal")]
pub mod eh1;
#[cfg(feature = "embedded-hal")]
pub mod embedded_hal;

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

    /// Execute a generic transaction
    ///
    /// This is a convenience method that dispatches to the appropriate
    /// method based on transaction type.
    fn execute_transaction(&mut self, txn: &TransactionType) -> Result<Option<Vec<u8>>, Error> {
        match txn {
            TransactionType::Write { register, data } => {
                self.write_register(*register, *data)?;
                Ok(None)
            }
            TransactionType::Read { register } => {
                let value = self.read_register(*register)?;
                Ok(Some(value.to_le_bytes().to_vec()))
            }
            TransactionType::ReadData { register } => {
                let mut buffer = [0u8; 512];
                self.read_data(*register, &mut buffer)?;
                Ok(Some(buffer.to_vec()))
            }
        }
    }

    /// Reset the device
    fn reset(&mut self) -> Result<(), Error>;

    /// Initialize the SPI interface
    fn initialize(&mut self) -> Result<(), Error>;
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
