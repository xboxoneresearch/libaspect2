use std::{thread::sleep, time::Duration};
use libftd2xx::{Ft4232h, FtdiCommon, FtdiMpsse, MpsseCmd, MpsseCmdBuilder, MpsseCmdExecutor};
use bitflags::bitflags;

use crate::error::Error;

pub struct EmmcReader {
    dev: Ft4232h,
}

/*
SPI_CLK:   AD0
SPI_MOSI:  AD1
SPI_MISO:  AD2
SPI_SS_N:  AD3
SPI_EN_N:  AD5
SPI_RST_N: AD7
*/

bitflags! {
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct SpiPin: u8 {
        const CLK =    1;          // Mask 0x01, AD0
        const MOSI =   1 << 1;     // Mask 0x02, AD1
        const MISO =   1 << 2;     // Mask 0x04, AD2
        const SS_N =   1 << 3;     // Mask 0x08, AD3 
        const SWO_DBG_EN = 1 << 4; // Mask 0x10, AD4
        const EN_N =   1 << 5;     // Mask 0x20, AD5
        const UNUSED =     1 << 6; // Mask 0x40, AD6
        const RST_N =  1 << 7;     // Mask 0x80, AD7
    }
}

impl EmmcReader {
    pub fn new(dev: Ft4232h) -> Self {
        Self {
            dev
        }
    }

    fn pin_directions() -> SpiPin {
        SpiPin::CLK | SpiPin::MOSI | SpiPin::SS_N | SpiPin::EN_N | SpiPin::RST_N
    }

    fn get_data_bits(&mut self) -> Result<SpiPin, Error> {
        let bits = self.dev.gpio_lower()?;
        SpiPin::from_bits(bits)
            .ok_or(Error::Todo)
    }

    fn set_data_bits_absolute(&mut self, state: SpiPin) -> Result<(), Error> {
        self.dev.set_gpio_lower(state.bits(), Self::pin_directions().bits())?;
        self.dev.set_gpio_upper(SpiPin::empty().bits(), SpiPin::empty().bits())?;

        Ok(())
    }

    fn set_data_bits_single(current_bits: SpiPin, target_bits: SpiPin, high: bool) -> Result<SpiPin, Error> {
        if target_bits.bits().count_ones() != 1 {
            return Err(Error::Todo);
        }

        let bits_set = if high {
            // Set bits by OR-ing
            current_bits | target_bits
        } else {
            // Unset bits by AND-ing with inverted pinmask
            current_bits & !target_bits
        };

        Ok(bits_set)
    }

    fn set_single_pin(&mut self, target_pin: SpiPin, high: bool) -> Result<(), Error> {
        let current = self.get_data_bits()?;
        let updated = Self::set_data_bits_single(current, target_pin, high)?;
        self.dev.set_gpio_lower(updated.bits(), Self::pin_directions().bits())?;
        Ok(())
    }

    pub fn smc_reset(&mut self) -> Result<(), Error> {
        // Assert SMC RESET
        self.set_single_pin(SpiPin::RST_N, false)?;

        // Release SMC RESET
        std::thread::sleep(Duration::from_millis(100));
        self.set_single_pin(SpiPin::RST_N, true)?;

        Ok(())
    }

    pub fn init(&mut self) -> Result<(), Error> {
        self.dev.set_bit_mode(0x0, libftd2xx::BitMode::Mpsse)?;
        self.dev.set_latency_timer(Duration::from_millis(2))?;

        self.set_data_bits_absolute(
            SpiPin::SS_N | SpiPin::EN_N | SpiPin::RST_N,
        )?;

        // Enable SPI Level shifter
        self.set_single_pin(SpiPin::EN_N, false)?;

        // Assert chip select
        self.set_single_pin(SpiPin::SS_N, false)?;

        // TODO: Maybe init I2C (Port C) now ?
        self.smc_reset()?;

        // Release chip select
        self.set_single_pin(SpiPin::SS_N, true)?;

        // Setup clock
        // Disable Clock divide and set initial frequency
        //let cmd = MpsseCmdBuilder::new().set_clock(149_000, Some(false));
        //self.dev.send(cmd.as_slice())?;   
        //self.dev.send(&[0x8a, 0x86, 0x95,0x00])?;  
        self.dev.set_clock(149)?;


        self.send_cmd((0x2, 2), (0x44, 8), 3)?;

        // Sanity check 1/2
        self.send_cmd((0x2, 2), (0x02, 8), 0x12345678)?;
        let resp = self.recv_resp((0x1, 2), (0x2, 8), 4)?;

        assert_eq!(resp.len(), 4);
        assert_eq!(format!("{:#X}", u32::from_le_bytes(resp[..4].try_into().unwrap())), format!("{:#X}", 0x12345678), "Sanity check failed");

        // Sanity check 2/2
        self.send_cmd((0x2, 2), (0x02, 8), 0xedcba987)?;
        let resp = self.recv_resp((0x1, 2), (0x2, 8), 4)?;

        assert_eq!(resp.len(), 4);
        assert_eq!(format!("{:#X}", u32::from_le_bytes(resp[..4].try_into().unwrap())), format!("{:#X}", 0xedcba987u32), "Sanity check failed");


        println!("Sanity check success");

        Ok(())
    }

    pub fn send_cmd(&mut self, bits1_out: (u8, u8), bits2_out: (u8, u8), data: u32) -> Result<(), Error> {
        let bits = self.get_data_bits()?;

        let builder = MpsseCmdBuilder::new()
            // Assert ChipSelect
            .set_gpio_lower((bits.clone() & !SpiPin::SS_N).bits(), Self::pin_directions().bits())
            .clock_bits_out(libftd2xx::ClockBitsOut::LsbNeg, bits1_out.0, bits1_out.1)
            .clock_bits_out(libftd2xx::ClockBitsOut::LsbNeg, bits2_out.0, bits2_out.1)
            .clock_data_out(libftd2xx::ClockDataOut::LsbNeg, &data.to_le_bytes())
            // Release ChipSelect
            .set_gpio_lower((bits | SpiPin::SS_N).bits(), Self::pin_directions().bits()) ;

        self.dev.send(builder.as_slice())?;

        Ok(())
    }

    pub fn recv_resp(&mut self, bits1_out: (u8, u8), bits2_out: (u8, u8), recv_len: usize) -> Result<Vec<u8>, Error> {
        let bits = self.get_data_bits()?;

        let builder = MpsseCmdBuilder::new()
            // Assert ChipSelect
            .set_gpio_lower((bits.clone() & !SpiPin::SS_N).bits(), Self::pin_directions().bits())
            .clock_bits_out(libftd2xx::ClockBitsOut::LsbNeg, bits1_out.0, bits1_out.1)
            .clock_bits_out(libftd2xx::ClockBitsOut::LsbNeg, bits2_out.0, bits2_out.1);
    
        let builder2 = MpsseCmdBuilder::new()
            .clock_data_in(libftd2xx::ClockDataIn::LsbPos, recv_len)
            // Release ChipSelect
            .set_gpio_lower((bits | SpiPin::SS_N).bits(), Self::pin_directions().bits())
            .send_immediate();

        let mut final_cmd = vec![];

        final_cmd.extend_from_slice(&builder.as_slice());
        // Clock N8 Cycles
        final_cmd.extend_from_slice(&[0x8F, 0x01, 0x00]);
        final_cmd.extend_from_slice(&builder2.as_slice());

        self.dev.send(final_cmd.as_slice())?;
        let mut recv_buffer = vec![0u8; recv_len];
        self.dev.recv(&mut recv_buffer)?;

        Ok(recv_buffer)
    }

    pub fn test(&self) -> Result<(), Box<dyn std::error::Error>> {
        
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn flags() {
        assert_eq!(0xA8, (SpiPin::SS_N | SpiPin::EN_N | SpiPin::RST_N).bits());
        assert_eq!(0xAB, (SpiPin::CLK | SpiPin::MOSI | SpiPin::SS_N | SpiPin::EN_N | SpiPin::RST_N).bits());
    }

    #[test]
    pub fn set_bits_high() {
        assert_eq!(
            EmmcReader::set_data_bits_single(SpiPin::CLK, SpiPin::EN_N, true).unwrap(),
            SpiPin::CLK | SpiPin::EN_N
        );

        assert_eq!(
            EmmcReader::set_data_bits_single(SpiPin::CLK, SpiPin::CLK, true).unwrap(),
            SpiPin::CLK
        );

        assert_eq!(
            EmmcReader::set_data_bits_single(SpiPin::CLK | SpiPin::MOSI, SpiPin::CLK, true).unwrap(),
            SpiPin::CLK | SpiPin::MOSI
        );

        assert_eq!(
            EmmcReader::set_data_bits_single(SpiPin::CLK | SpiPin::MOSI, SpiPin::SS_N, true).unwrap(),
            SpiPin::CLK | SpiPin::MOSI | SpiPin::SS_N
        );
    }

    #[test]
    pub fn set_bits_low() {
        assert_ne!(
            EmmcReader::set_data_bits_single(SpiPin::CLK, SpiPin::EN_N, false).unwrap(),
            SpiPin::CLK | SpiPin::EN_N
        );

        assert_ne!(
            EmmcReader::set_data_bits_single(SpiPin::CLK, SpiPin::CLK, false).unwrap(),
            SpiPin::CLK
        );

        assert_ne!(
            EmmcReader::set_data_bits_single(SpiPin::CLK | SpiPin::MOSI, SpiPin::CLK, false).unwrap(),
            SpiPin::CLK | SpiPin::MOSI
        );

        assert_ne!(
            EmmcReader::set_data_bits_single(SpiPin::CLK | SpiPin::MOSI, SpiPin::SS_N, false).unwrap(),
            SpiPin::CLK | SpiPin::MOSI | SpiPin::SS_N
        );

        assert_eq!(
            EmmcReader::set_data_bits_single(SpiPin::CLK, SpiPin::EN_N, false).unwrap(),
            SpiPin::CLK
        );

        assert_eq!(
            EmmcReader::set_data_bits_single(SpiPin::CLK, SpiPin::CLK, false).unwrap(),
            SpiPin::empty()
        );

        assert_eq!(
            EmmcReader::set_data_bits_single(SpiPin::CLK | SpiPin::MOSI, SpiPin::CLK, false).unwrap(),
            SpiPin::MOSI
        );

        assert_eq!(
            EmmcReader::set_data_bits_single(SpiPin::CLK | SpiPin::MOSI, SpiPin::SS_N, false).unwrap(),
            SpiPin::CLK | SpiPin::MOSI
        );
    }

}