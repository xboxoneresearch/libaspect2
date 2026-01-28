//! embedded-hal 1.0 SPI backend
//!
//! This backend uses `embedded_hal::spi::SpiDevice` (eh1)
//! with optional GPIO pins for reset and enable control.

use core::time::Duration;
use embedded_hal::{
    delay::DelayNs,
    digital::OutputPin,
    spi::SpiDevice,
};

use crate::error::Error;
use crate::protocol::{Command, Register};
use super::{SpiBackend, GpioControl};

/// embedded-hal 1.0 SPI Backend
///
/// * `SPI`  – SPI device (chip-select handled by SpiDevice)
/// * `RST`  – Optional reset pin (active low)
/// * `EN`   – Optional enable pin (active low)
/// * `D`    – Delay provider
pub struct Eh1SpiBackend<SPI, RST, EN, D> {
    spi: SPI,
    reset: Option<RST>,
    enable: Option<EN>,
    delay: D,
}

impl<SPI, RST, EN, D> Eh1SpiBackend<SPI, RST, EN, D>
where
    SPI: SpiDevice,
    RST: OutputPin,
    EN: OutputPin,
    D: DelayNs,
{
    /// Create a new eh1 SPI backend
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

    /// Control reset pin (active low)
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

    /// Control enable pin (active low)
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

impl<SPI, RST, EN, D> GpioControl for Eh1SpiBackend<SPI, RST, EN, D>
where
    SPI: SpiDevice,
    RST: OutputPin,
    EN: OutputPin,
    D: DelayNs,
{
    fn set_chip_select(&mut self, _asserted: bool) -> Result<(), Error> {
        // Chip select is managed by SpiDevice
        Ok(())
    }

    fn set_reset(&mut self, asserted: bool) -> Result<(), Error> {
        self.set_reset_internal(asserted)
    }

    fn set_enable(&mut self, enabled: bool) -> Result<(), Error> {
        self.set_enable_internal(enabled)
    }
}

impl<SPI, RST, EN, D> SpiBackend for Eh1SpiBackend<SPI, RST, EN, D>
where
    SPI: SpiDevice,
    RST: OutputPin,
    EN: OutputPin,
    D: DelayNs,
{
    fn write_register(&mut self, register: Register, data: u32) -> Result<(), Error> {
        let mut frame = [0u8; 6];

        frame[0] = Command::Write.bits();
        frame[1] = register.address();
        frame[2..6].copy_from_slice(&data.to_le_bytes());

        self.spi
            .write(&frame)
            .map_err(|_| Error::Spi)?;

        Ok(())
    }

    fn read_register(&mut self, register: Register) -> Result<u32, Error> {
        let tx = [Command::Read.bits(), register.address()];
        let mut rx = [0u8; 4];

        self.spi
            .write(&tx)
            .map_err(|_| Error::Spi)?;

        // Match FTDI dummy clock delay
        self.delay.delay_ns(1_000);

        self.spi
            .read(&mut rx)
            .map_err(|_| Error::Spi)?;

        Ok(u32::from_le_bytes(rx))
    }

    fn read_data(&mut self, register: Register, buffer: &mut [u8]) -> Result<(), Error> {
        let tx = [Command::Read.bits(), register.address()];

        self.spi
            .write(&tx)
            .map_err(|_| Error::Spi)?;

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
        // Default pin states
        self.set_enable_internal(true)?;
        self.set_reset_internal(false)?;

        // Perform reset sequence
        self.reset()?;

        Ok(())
    }
}