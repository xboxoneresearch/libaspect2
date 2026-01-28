//! embedded-hal SPI backend
//!
//! This backend uses an `embedded_hal::eh0::spi::SpiDevice`
//! and optional GPIO controls for reset / enable.

use core::time::Duration;
use embedded_hal::eh0::{
    delay::DelayNs,
    digital::OutputPin,
    spi::SpiDevice,
};

use crate::error::Error;
use crate::protocol::{Command, Register};
use super::{SpiBackend, GpioControl};

/// embedded-hal SPI Backend
///
/// `SPI` is the SPI device (with CS handled by the implementation).
/// `RST`, `EN` are optional GPIO pins (active low).
/// `D` is a delay provider.
pub struct EhSpiBackend<SPI, RST, EN, D> {
    spi: SPI,
    reset: Option<RST>,
    enable: Option<EN>,
    delay: D,
}

impl<SPI, RST, EN, D> EhSpiBackend<SPI, RST, EN, D>
where
    SPI: SpiDevice,
    RST: OutputPin,
    EN: OutputPin,
    D: DelayNs,
{
    /// Create a new embedded-hal SPI backend
    pub fn new(
        spi: SPI,
        reset: Option<RST>,
        enable: Option<EN>,
        delay: D,
    ) -> Self {
        Self {
            spi,
            reset,
            enable,
            delay,
        }
    }

    /// Assert / deassert reset (active low)
    fn set_reset_internal(&mut self, asserted: bool) -> Result<(), Error> {
        if let Some(pin) = self.reset.as_mut() {
            if asserted {
                pin.set_low().map_err(|_| Error::Gpio)?;
            } else {
                pin.set_high().map_err(|_| Error::Gpio)?;
            }
        }
        Ok(())
    }

    /// Enable / disable level shifter (active low)
    fn set_enable_internal(&mut self, enabled: bool) -> Result<(), Error> {
        if let Some(pin) = self.enable.as_mut() {
            if enabled {
                pin.set_low().map_err(|_| Error::Gpio)?;
            } else {
                pin.set_high().map_err(|_| Error::Gpio)?;
            }
        }
        Ok(())
    }
}

impl<SPI, RST, EN, D> GpioControl for EhSpiBackend<SPI, RST, EN, D>
where
    SPI: SpiDevice,
    RST: OutputPin,
    EN: OutputPin,
    D: DelayNs,
{
    fn set_chip_select(&mut self, _asserted: bool) -> Result<(), Error> {
        // Chip select is owned by SpiDevice.
        // Intentionally a no-op.
        Ok(())
    }

    fn set_reset(&mut self, asserted: bool) -> Result<(), Error> {
        self.set_reset_internal(asserted)
    }

    fn set_enable(&mut self, enabled: bool) -> Result<(), Error> {
        self.set_enable_internal(enabled)
    }
}

impl<SPI, RST, EN, D> SpiBackend for EhSpiBackend<SPI, RST, EN, D>
where
    SPI: SpiDevice,
    RST: OutputPin,
    EN: OutputPin,
    D: DelayNs,
{
    fn write_register(&mut self, register: Register, data: u32) -> Result<(), Error> {
        let mut frame = [0u8; 1 + 1 + 4];

        // Command (WRITE)
        frame[0] = Command::Write.bits();

        // Register address
        frame[1] = register.address();

        // Data (little-endian)
        frame[2..6].copy_from_slice(&data.to_le_bytes());

        self.spi
            .write(&frame)
            .map_err(|_| Error::Spi)?;

        Ok(())
    }

    fn read_register(&mut self, register: Register) -> Result<u32, Error> {
        let mut tx = [0u8; 2];
        let mut rx = [0u8; 4];

        // Send READ command + register
        tx[0] = Command::Read.bits();
        tx[1] = register.address();

        self.spi
            .write(&tx)
            .map_err(|_| Error::Spi)?;

        // Small wait (matches FTDI dummy clocks)
        self.delay.delay_ns(1_000);

        // Read response
        self.spi
            .read(&mut rx)
            .map_err(|_| Error::Spi)?;

        Ok(u32::from_le_bytes(rx))
    }

    fn read_data(&mut self, register: Register, buffer: &mut [u8]) -> Result<(), Error> {
        let mut tx = [0u8; 2];

        // READ command + register
        tx[0] = Command::Read.bits();
        tx[1] = register.address();

        self.spi
            .write(&tx)
            .map_err(|_| Error::Spi)?;

        // Wait for device to prepare data
        self.delay.delay_ns(1_000);

        self.spi
            .read(buffer)
            .map_err(|_| Error::Spi)?;

        Ok(())
    }

    fn reset(&mut self) -> Result<(), Error> {
        self.set_reset_internal(true)?;
        self.delay.delay_ns(100_000_000); // 100 ms
        self.set_reset_internal(false)?;
        Ok(())
    }

    fn initialize(&mut self) -> Result<(), Error> {
        // Default states
        self.set_enable_internal(true)?;
        self.set_reset_internal(false)?;

        // Reset sequence
        self.reset()?;

        Ok(())
    }
}