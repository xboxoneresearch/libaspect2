use bitflags::bitflags;
use libftd2xx::{DeviceInfo, Ft4232h, FtdiCommon, FtdiMpsse, MpsseCmdBuilder, MpsseCmdExecutor, list_devices};
/// FTDI backend implementation using libftd2xx
///
/// This backend provides direct FTDI MPSSE access for maximum performance.
use std::time::Duration;

use super::{GpioControl, RawSpiBackend, SpiBackend};
use crate::error::Error;
use crate::spi::protocol::constants::{Register, TransferOp};

/*
Pin assignments on FTDI FT4232H:
SPI_CLK:   AD0
SPI_MOSI:  AD1
SPI_MISO:  AD2
SPI_SS_N:  AD3
SPI_EN_N:  AD5
SPI_RST_N: AD7
*/

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct SpiPin: u8 {
        const CLK =        1;          // Mask 0x01, AD0
        const MOSI =       1 << 1;     // Mask 0x02, AD1
        const MISO =       1 << 2;     // Mask 0x04, AD2
        const SS_N =       1 << 3;     // Mask 0x08, AD3
        const SWO_DBG_EN = 1 << 4;     // Mask 0x10, AD4
        const EN_N =       1 << 5;     // Mask 0x20, AD5
        const UNUSED =     1 << 6;     // Mask 0x40, AD6
        const RST_N =      1 << 7;     // Mask 0x80, AD7
    }
}

/// FTDI SPI Backend
pub struct FtdiBackend {
    dev: Ft4232h,
    /// Cached GPIO pin state — avoids a USB round-trip per register access.
    cached_pins: SpiPin,
}

impl FtdiBackend {
    /// Create a new FTDI backend with the specified device
    pub fn new(dev: Ft4232h) -> Self {
        Self {
            dev,
            cached_pins: SpiPin::SS_N | SpiPin::EN_N | SpiPin::RST_N,
        }
    }

    pub fn scan() -> Result<Vec<DeviceInfo>, Error> {
        list_devices().map_err(std::convert::Into::into)
    }

    /// Open FTDI device by description
    pub fn open(description: &str) -> Result<Self, Error> {
        let dev = Ft4232h::with_description(description)?;
        Ok(Self::new(dev))
    }

    /// Get pin direction configuration (which pins are outputs)
    fn pin_directions() -> SpiPin {
        SpiPin::CLK | SpiPin::MOSI | SpiPin::SS_N | SpiPin::EN_N | SpiPin::RST_N
    }

    /// Return cached GPIO state (no USB round-trip).
    fn get_data_bits(&self) -> SpiPin {
        self.cached_pins
    }

    /// Set GPIO pins to specific absolute state
    fn set_data_bits_absolute(&mut self, state: SpiPin) -> Result<(), Error> {
        self.dev
            .set_gpio_lower(state.bits(), Self::pin_directions().bits())?;
        self.dev
            .set_gpio_upper(SpiPin::empty().bits(), SpiPin::empty().bits())?;
        self.cached_pins = state;
        Ok(())
    }

    /// Helper to set/clear specific bits
    fn set_data_bits_single(
        current_bits: SpiPin,
        target_bits: SpiPin,
        high: bool,
    ) -> Result<SpiPin, Error> {
        if target_bits.bits().count_ones() != 1 {
            return Err(Error::InvalidPinMask);
        }

        let bits_set = if high {
            current_bits | target_bits
        } else {
            current_bits & !target_bits
        };

        Ok(bits_set)
    }

    /// Set a single pin high or low
    pub fn set_single_pin(&mut self, target_pin: SpiPin, high: bool) -> Result<(), Error> {
        let current = self.get_data_bits();
        let updated = Self::set_data_bits_single(current, target_pin, high)?;
        self.dev
            .set_gpio_lower(updated.bits(), Self::pin_directions().bits())?;
        self.cached_pins = updated;
        Ok(())
    }
}

impl GpioControl for FtdiBackend {
    fn set_chip_select(&mut self, asserted: bool) -> Result<(), Error> {
        // SS_N is active low, so asserted=true means pin=low
        self.set_single_pin(SpiPin::SS_N, !asserted)
    }

    fn set_reset(&mut self, asserted: bool) -> Result<(), Error> {
        // RST_N is active low, so asserted=true means pin=low
        self.set_single_pin(SpiPin::RST_N, !asserted)
    }

    fn set_enable(&mut self, enabled: bool) -> Result<(), Error> {
        // EN_N is active low, so enabled=true means pin=low
        self.set_single_pin(SpiPin::EN_N, !enabled)
    }
}

impl SpiBackend for FtdiBackend {
    fn write_register<T: Into<u8>>(&mut self, register: T, data: u32) -> Result<(), Error> {
        let bits = self.get_data_bits();

        let builder = MpsseCmdBuilder::new()
            // Assert ChipSelect
            .set_gpio_lower((bits & !SpiPin::SS_N).bits(), Self::pin_directions().bits())
            // Send command bits (2 bits: WRITE = 0x2)
            .clock_bits_out(
                libftd2xx::ClockBitsOut::LsbNeg,
                TransferOp::Write.bits(),
                TransferOp::bit_length(),
            )
            // Send register address (8 bits)
            .clock_bits_out(
                libftd2xx::ClockBitsOut::LsbNeg,
                register.into(),
                Register::bit_length(),
            )
            // Send data (4 bytes, little-endian)
            .clock_data_out(libftd2xx::ClockDataOut::LsbNeg, &data.to_le_bytes())
            // Release ChipSelect
            .set_gpio_lower((bits | SpiPin::SS_N).bits(), Self::pin_directions().bits());

        self.dev.send(builder.as_slice())?;
        Ok(())
    }

    fn read_register<T: Into<u8>>(&mut self, register: T) -> Result<u32, Error> {
        let bits = self.get_data_bits();

        let builder = MpsseCmdBuilder::new()
            // Assert ChipSelect
            .set_gpio_lower((bits & !SpiPin::SS_N).bits(), Self::pin_directions().bits())
            // Send command bits (2 bits: READ = 0x1)
            .clock_bits_out(
                libftd2xx::ClockBitsOut::LsbNeg,
                TransferOp::Read.bits(),
                TransferOp::bit_length(),
            )
            // Send register address (8 bits)
            .clock_bits_out(
                libftd2xx::ClockBitsOut::LsbNeg,
                register.into(),
                Register::bit_length(),
            );

        let builder2 = MpsseCmdBuilder::new()
            // Read 4 bytes of data
            .clock_data_in(libftd2xx::ClockDataIn::LsbPos, 4)
            // Release ChipSelect
            .set_gpio_lower((bits | SpiPin::SS_N).bits(), Self::pin_directions().bits())
            .send_immediate();

        let mut final_cmd = vec![];
        final_cmd.extend_from_slice(builder.as_slice());
        // Clock 8 cycles (wait time for device to prepare response)
        final_cmd.extend_from_slice(&[0x8F, 0x01, 0x00]);
        final_cmd.extend_from_slice(builder2.as_slice());

        self.dev.send(final_cmd.as_slice())?;

        let mut recv_buffer = [0u8; 4];
        self.dev.recv(&mut recv_buffer)?;

        Ok(u32::from_le_bytes(recv_buffer))
    }

    fn read_data<T: Into<u8>>(&mut self, register: T, buffer: &mut [u8]) -> Result<(), Error> {
        let bits = self.get_data_bits();

        let builder = MpsseCmdBuilder::new()
            // Assert ChipSelect
            .set_gpio_lower((bits & !SpiPin::SS_N).bits(), Self::pin_directions().bits())
            // Send command bits (2 bits: READ = 0x1)
            .clock_bits_out(
                libftd2xx::ClockBitsOut::LsbNeg,
                TransferOp::Read.bits(),
                TransferOp::bit_length(),
            )
            // Send register address (8 bits)
            .clock_bits_out(
                libftd2xx::ClockBitsOut::LsbNeg,
                register.into(),
                Register::bit_length(),
            );

        let builder2 = MpsseCmdBuilder::new()
            // Read 512 bytes of data
            .clock_data_in(libftd2xx::ClockDataIn::LsbPos, buffer.len())
            // Release ChipSelect
            .set_gpio_lower((bits | SpiPin::SS_N).bits(), Self::pin_directions().bits())
            .send_immediate();

        let mut final_cmd = vec![];
        final_cmd.extend_from_slice(builder.as_slice());
        // Clock 8 cycles (wait time)
        final_cmd.extend_from_slice(&[0x8F, 0x01, 0x00]);
        final_cmd.extend_from_slice(builder2.as_slice());

        self.dev.send(final_cmd.as_slice())?;
        self.dev.recv(buffer)?;

        Ok(())
    }

    fn write_data<T: Into<u8> + Copy>(&mut self, register: T, buffer: &[u8]) -> Result<(), Error> {
        // Write in 4-byte words via individual register writes
        for chunk in buffer.chunks(4) {
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_register(register, u32::from_le_bytes(word))?;
        }
        Ok(())
    }

    fn set_spi_clock_khz(&mut self, freq_khz: u32) -> Result<(), Error> {
        self.dev.set_clock(freq_khz * 1000)?;
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Error> {
        // Assert reset (active low)
        self.set_reset(true)?;

        // Hold for 100ms
        std::thread::sleep(Duration::from_millis(100));

        // Release reset
        self.set_reset(false)?;

        Ok(())
    }

    fn initialize(&mut self) -> Result<(), Error> {
        // Set MPSSE mode
        self.dev.set_bit_mode(0x0, libftd2xx::BitMode::Mpsse)?;

        // Set latency timer (lower = faster USB turnaround)
        self.dev.set_latency_timer(Duration::from_millis(1))?;

        self.dev.set_usb_parameters(65536)?;

        // Set initial GPIO state: SS_N=HIGH, EN_N=HIGH, RST_N=HIGH
        self.set_data_bits_absolute(SpiPin::SS_N | SpiPin::EN_N | SpiPin::RST_N)?;

        // Enable SPI level shifter (EN_N is active low)
        self.set_enable(true)?;

        // Assert chip select briefly
        self.set_chip_select(true)?;

        // Perform reset
        SpiBackend::reset(self)?;

        // Release chip select
        self.set_chip_select(false)?;

        // Setup clock frequency — conservative 5 kHz for init;
        // ramped up after the SPI bridge is verified.
        self.set_clock_freq_khz(5)?;

        Ok(())
    }
}

/// SPI NOR flash access — raw MSB-first byte stream, Mode 0 (CPOL=0, CPHA=0).
///
/// The eMMC bridge IC uses LSB-first framing (`SpiBackend`).  Standard JEDEC NOR
/// flash devices use MSB-first, so these MPSSE opcodes differ deliberately.
impl RawSpiBackend for FtdiBackend {
    fn spi_transaction(&mut self, cmd: &[u8], write: &[u8], read: &mut [u8]) -> Result<(), Error> {
        let bits = self.get_data_bits();
        let pins_cs_low = (bits & !SpiPin::SS_N).bits();
        let pins_cs_high = (bits | SpiPin::SS_N).bits();
        let dirs = Self::pin_directions().bits();

        // Assert CS
        let mut packet = MpsseCmdBuilder::new()
            .set_gpio_lower(pins_cs_low, dirs)
            .as_slice()
            .to_vec();

        // Send cmd bytes — MSB first, shift on falling CLK edge (Mode 0)
        if !cmd.is_empty() {
            packet.extend_from_slice(
                MpsseCmdBuilder::new()
                    .clock_data_out(libftd2xx::ClockDataOut::MsbNeg, cmd)
                    .as_slice(),
            );
        }

        // Send write bytes — MSB first, shift on falling CLK edge (Mode 0)
        if !write.is_empty() {
            packet.extend_from_slice(
                MpsseCmdBuilder::new()
                    .clock_data_out(libftd2xx::ClockDataOut::MsbNeg, write)
                    .as_slice(),
            );
        }

        // Receive read bytes — MSB first, sample on rising CLK edge (Mode 0)
        if !read.is_empty() {
            packet.extend_from_slice(
                MpsseCmdBuilder::new()
                    .clock_data_in(libftd2xx::ClockDataIn::MsbPos, read.len())
                    .as_slice(),
            );
        }

        // Deassert CS and flush the USB pipe
        packet.extend_from_slice(
            MpsseCmdBuilder::new()
                .set_gpio_lower(pins_cs_high, dirs)
                .send_immediate()
                .as_slice(),
        );

        self.dev.send(&packet)?;

        if !read.is_empty() {
            self.dev.recv(read)?;
        }

        Ok(())
    }

    fn set_clock_freq_khz(&mut self, freq_khz: u32) -> Result<(), Error> {
        self.dev.set_clock(freq_khz * 1000)?;
        Ok(())
    }

    fn initialize(&mut self) -> Result<(), Error> {
        // Set MPSSE mode
        self.dev.set_bit_mode(0x0, libftd2xx::BitMode::Mpsse)?;

        // Set latency timer (lower = faster USB turnaround)
        self.dev.set_latency_timer(Duration::from_millis(1))?;

        self.dev.set_usb_parameters(65536)?;

        // Set initial GPIO state: SS_N=HIGH, EN_N=HIGH, RST_N=HIGH
        self.set_data_bits_absolute(SpiPin::SS_N | SpiPin::EN_N | SpiPin::RST_N)?;

        // Enable SPI level shifter (EN_N is active low)
        self.set_enable(true)?;

        // Assert chip select briefly
        self.set_chip_select(true)?;

        // Assert and HOLD SMC Reset
        // for NOR interaction we don't want the SMC to be active
        self.set_reset(true)?;

        // Release chip select
        self.set_chip_select(false)?;
        
        // Setup clock frequency — conservative 5 kHz for init;
        // ramped up after the SPI bridge is verified.
        self.set_clock_freq_khz(5)?;

        Ok(())
    }

    fn reset(&mut self) -> Result<(), Error> {
        <Self as SpiBackend>::reset(self)
    }
}

impl Drop for FtdiBackend {
    fn drop(&mut self) {
        // Release SMC Reset
        self.set_reset(false).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pin_flags() {
        assert_eq!(0xA8, (SpiPin::SS_N | SpiPin::EN_N | SpiPin::RST_N).bits());
        assert_eq!(
            0xAB,
            (SpiPin::CLK | SpiPin::MOSI | SpiPin::SS_N | SpiPin::EN_N | SpiPin::RST_N).bits()
        );
    }

    #[test]
    fn test_set_bits_high() {
        assert_eq!(
            FtdiBackend::set_data_bits_single(SpiPin::CLK, SpiPin::EN_N, true).unwrap(),
            SpiPin::CLK | SpiPin::EN_N
        );

        assert_eq!(
            FtdiBackend::set_data_bits_single(SpiPin::CLK, SpiPin::CLK, true).unwrap(),
            SpiPin::CLK
        );
    }

    #[test]
    fn test_set_bits_low() {
        assert_eq!(
            FtdiBackend::set_data_bits_single(SpiPin::CLK, SpiPin::EN_N, false).unwrap(),
            SpiPin::CLK
        );

        assert_eq!(
            FtdiBackend::set_data_bits_single(SpiPin::CLK, SpiPin::CLK, false).unwrap(),
            SpiPin::empty()
        );
    }
}
