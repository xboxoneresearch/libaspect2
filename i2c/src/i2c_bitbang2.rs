use std::time::Duration;

use embedded_hal::i2c::{ErrorKind, ErrorType, I2c};
use libftd2xx::{BitMode, Ft4232h, FtdiCommon};
use log::{debug, trace};

const BITMODE: libftd2xx::BitMode = BitMode::AsyncBitbang;

#[allow(non_camel_case_types)]
#[derive(Debug)]
enum PinState2 {
    SDA_HI,
    SDA_LO,
    SCL_HI,
    SCL_LO,
}

pub struct I2cFtBitbang2 {
    device: Ft4232h,
    scl_pin: u8,
    sda_pin: u8,
    delay: Option<Duration>,
}

impl I2cFtBitbang2 {
    pub fn new(mut device: Ft4232h, scl_pin: u8, sda_pin: u8) -> Self {
        // Set all pins to bitbang mode
        device.set_bit_mode(0b_1100_0000, BITMODE).unwrap();

        Self {
            device,
            scl_pin,
            sda_pin,
            delay: None
        }
    }
}

impl I2cFtBitbang2 {
    /// SCL bitmask
    #[allow(non_snake_case)]
    fn SCL_MASK(&self) -> u8 {
        1 << self.scl_pin
    }

    /// SDA bitmask
    #[allow(non_snake_case)]
    fn SDA_MASK(&self) -> u8 {
        1 << self.sda_pin
    }

    fn dly(&self) {
        if let Some(delay) = self.delay {
            std::thread::sleep(delay);
        }
    }

    fn set_pins(&mut self, state: u8) -> Result<(), libftd2xx::FtStatus> {
        trace!("Set pins: {state:08b}");
        let count = self.device.write(&[state])?;

        // Clear the TX buffer
        let mut buf = vec![0u8; count];
        self.device.read(&mut buf)?;
        Ok(())
    }

    fn get_pins(&mut self) -> Result<u8, libftd2xx::FtStatus> {
        let state = self.device.bit_mode()?;
        trace!("Get pins: {:08b}", state);
        Ok(state)
    }

    fn read_data(&mut self) -> Result<u8, libftd2xx::FtStatus> {
        self.device.set_bit_mode(0x0, BITMODE)?;
        let state = self.get_pins()?;
        self.device
            .set_bit_mode(self.SCL_MASK() | self.SDA_MASK(), BITMODE)?;
        Ok(state)
    }

    fn set(&mut self, pinstate: PinState2) -> Result<(), libftd2xx::FtStatus> {
        let state = self.get_pins()?;
        trace!("Setting: {pinstate:?}");
        let new_state = match pinstate {
            PinState2::SDA_HI => state | self.SDA_MASK(),
            PinState2::SDA_LO => state & !self.SDA_MASK(),
            PinState2::SCL_HI => state | self.SCL_MASK(),
            PinState2::SCL_LO => state & !self.SCL_MASK(),
        };

        self.set_pins(new_state)?;
        Ok(())
    }

    fn read_sda(&mut self) -> Result<u8, libftd2xx::FtStatus> {
        // Set SDA  as input
        //self.device.set_bit_mode(self.SCL_MASK(), BITMODE)?;
        let new_pinstate = (self.read_data()? & self.SDA_MASK()) >> self.sda_pin;
        //self.device.set_bit_mode(self.SCL_MASK() | self.SDA_MASK(), BITMODE)?;
        trace!("Got pinstate (SDA): {:#b}", new_pinstate);

        Ok(new_pinstate)
    }

    fn read_scl(&mut self) -> Result<u8, libftd2xx::FtStatus> {
        // Set SCL as input
        //self.device.set_bit_mode(self.SDA_MASK(), BITMODE)?;
        let new_pinstate = (self.read_data()? & self.SCL_MASK()) >> self.scl_pin;
        //self.device.set_bit_mode(self.SCL_MASK() | self.SDA_MASK(), BITMODE)?;
        trace!("Got pinstate (SCL): {:#b}", new_pinstate);

        Ok(new_pinstate)
    }

    fn i2c_reset(&mut self) -> Result<(), libftd2xx::FtStatus> {
        self.set(PinState2::SCL_LO)?;
        self.set(PinState2::SDA_LO)
    }

    fn i2c_start(&mut self) -> Result<(), libftd2xx::FtStatus> {
        self.set(PinState2::SDA_HI)?;
        self.dly();
        self.set(PinState2::SCL_HI)?;
        self.dly();
        self.set(PinState2::SDA_LO)?;
        self.dly();
        self.set(PinState2::SCL_LO)?;
        self.dly();
        Ok(())
    }

    fn i2c_stop(&mut self) -> Result<(), libftd2xx::FtStatus> {
        self.set(PinState2::SDA_LO)?;
        self.dly();

        self.set(PinState2::SCL_HI)?;
        self.dly();
        self.set(PinState2::SDA_HI)?;
        self.dly();

        Ok(())
    }

    fn i2c_tx(&mut self, databyte: u8) -> Result<bool, libftd2xx::FtStatus> {
        let mut bit: u8;

        for i in 0..8 {
            bit = (databyte >> (7 - i)) & 0x01;

            if bit == 1 {
                self.set(PinState2::SDA_HI)?;
            } else {
                self.set(PinState2::SDA_LO)?;
            }
            self.dly();

            self.set(PinState2::SCL_HI)?;
            self.dly();
            self.set(PinState2::SCL_LO)?;
            self.dly();
        }

        self.set(PinState2::SDA_HI)?;
        self.set(PinState2::SCL_HI)?;
        self.dly();
        let ack = self.read_sda()? == 0;
        self.set(PinState2::SCL_LO)?;

        return Ok(ack);
    }

    fn i2c_rx(&mut self, ack: bool) -> Result<u8, libftd2xx::FtStatus> {
        let mut databyte = 0u8;

        self.set(PinState2::SDA_HI)?;

        for _ in 0..8 {
            databyte <<= 1;
            loop {
                self.set(PinState2::SCL_HI)?;
                if self.read_scl()? == 0x1 {
                    break;
                }
            }

            self.set(PinState2::SCL_HI)?;

            databyte |= self.read_sda()?;

            self.set(PinState2::SCL_LO)?;
        }

        if ack {
            self.set(PinState2::SDA_LO)?;
        } else {
            self.set(PinState2::SDA_HI)?;
        }

        self.set(PinState2::SCL_HI)?;
        self.set(PinState2::SCL_LO)?;
        self.set(PinState2::SDA_HI)?;

        return Ok(databyte);
    }
}

impl I2c for I2cFtBitbang2 {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        self.i2c_reset().map_err(|_| ErrorKind::Other)?;

        for op in operations {
            match op {
                embedded_hal::i2c::Operation::Read(rd) => {
                    self.i2c_start().map_err(|_| ErrorKind::Other)?;
                    self.dly();

                    // First, send target address
                    let ack = self.i2c_tx((address << 1) | 0x01).unwrap();
                    /*
                    if !ack {
                        return Err(ErrorKind::NoAcknowledge(embedded_hal::i2c::NoAcknowledgeSource::Address));
                    }
                    */
                    self.dly();

                    debug!(
                        "Read transaction with {} bytes, target: {address:#04x}",
                        rd.len()
                    );

                    // Now, receive data
                    for idx in 0..rd.len() {
                        let ack = false;
                        rd[idx] = self.i2c_rx(ack).unwrap();
                        self.dly();
                    }

                    self.i2c_stop().map_err(|_| ErrorKind::Other)?;
                    self.dly();
                }
                embedded_hal::i2c::Operation::Write(wr) => {
                    self.i2c_start().map_err(|_| ErrorKind::Other)?;
                    self.dly();

                    // First, send target address
                    debug!(
                        "Write transaction with {} bytes, target: {address:#04x}",
                        wr.len()
                    );
                    let ack = self.i2c_tx(address << 1).unwrap();

                    /*
                    if !ack {
                        return Err(ErrorKind::NoAcknowledge(embedded_hal::i2c::NoAcknowledgeSource::Address));
                    }
                    */
                    self.dly();

                    for idx in 0..wr.len() {
                        self.i2c_tx(wr[idx]).unwrap();
                        self.dly();
                    }

                    self.i2c_stop().map_err(|_| ErrorKind::Other)?;
                    self.dly();
                }
            }
        }

        Ok(())
    }
}

impl ErrorType for I2cFtBitbang2 {
    type Error = ErrorKind;
}
