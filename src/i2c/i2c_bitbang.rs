use crate::prelude::*;

use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation};
use libftd2xx::{BitMode, Ft4232h, FtdiCommon};

const BITMODE: libftd2xx::BitMode = BitMode::SyncBitbang;

const I2C_SCL: u8 = 1 << 6; // CDBUS6
const I2C_SDA: u8 = 1 << 7; // CDBUS7
const I2C_MASK: u8 = I2C_SCL | I2C_SDA;

pub struct I2cFtBitbang {
    device: Ft4232h,
    gpio_val: u8,
    gpio_dir: u8,
    gpio_dir_hw: Option<u8>,
}

impl I2cFtBitbang {
    pub fn new(device: Ft4232h) -> Self {
        Self {
            device,
            gpio_val: I2C_MASK, // Both high
            gpio_dir: 0,        // Both as input (high, open-drain)
            gpio_dir_hw: None,
        }
    }
}

impl I2cFtBitbang {
    fn gpio_write(&mut self, values: u8, direction: u8) {
        // skip set_bit_mode when the direction mask hasn't
        // actually changed since the last call.
        if self.gpio_dir_hw != Some(direction) {
            self.device.set_bit_mode(direction, BITMODE).unwrap();
            self.gpio_dir_hw = Some(direction);
        }
        self.device.write(&[values]).unwrap();
    }

    fn gpio_read(&mut self) -> u8 {
        let bits = self.device.bit_mode().unwrap();
        bits
    }

    /* Drive SDA high (release = input) */
    fn sda_high(&mut self) {
        if self.gpio_val & I2C_SDA != 0 && self.gpio_dir & I2C_SDA == 0 {
            return;
        }
        self.gpio_val |= I2C_SDA;
        self.gpio_dir &= !I2C_SDA; // input
        self.gpio_write(self.gpio_val, self.gpio_dir);
    }

    fn set_sda(&mut self, high: bool) {
        if high {
            self.sda_high();
        } else {
            self.sda_low();
        }
    }

    /* Drive SDA low */
    fn sda_low(&mut self) {
        if self.gpio_val & I2C_SDA == 0 && self.gpio_dir & I2C_SDA != 0 {
            return;
        }
        self.gpio_val &= !I2C_SDA;
        self.gpio_dir |= I2C_SDA; // output
        self.gpio_write(self.gpio_val, self.gpio_dir);
    }

    /* Set SCL high, then wait for the slave to release it (clock stretching).
     * Returns the last-sampled pin byte so callers that need to read SDA
     * right after a clock edge (ACK/data bit sampling) can reuse this read
     * instead of issuing a second one. */
    fn scl_high(&mut self) -> u8 {
        if self.gpio_val & I2C_SCL != 0 && self.gpio_dir & I2C_SCL == 0 {
            return self.gpio_val;
        }
        self.gpio_val |= I2C_SCL;
        self.gpio_dir &= !I2C_SCL; // input
        self.gpio_write(self.gpio_val, self.gpio_dir);

        let deadline = Instant::now() + Duration::from_millis(50);
        let mut pins = self.gpio_read();
        while pins & I2C_SCL == 0 {
            if Instant::now() >= deadline {
                break;
            }
            pins = self.gpio_read();
        }
        pins
    }

    /* Set SCL low */
    fn scl_low(&mut self) {
        if self.gpio_val & I2C_SCL == 0 && self.gpio_dir & I2C_SCL != 0 {
            return;
        }
        self.gpio_val &= !I2C_SCL;
        self.gpio_dir |= I2C_SCL; // output
        self.gpio_write(self.gpio_val, self.gpio_dir);
    }

    fn i2c_start(&mut self) {
        // SDA descending while SCL is HIGH.
        self.sda_high();
        self.scl_high();
        self.sda_low();
        self.scl_low();
    }

    fn i2c_stop(&mut self) {
        // SDA rasing while SCL is HIGH.
        self.sda_low();
        self.scl_high();
        self.sda_high();
    }

    fn i2c_tx(&mut self, byte: u8) -> bool {
        let mut byte = byte;
        for _ in 0..8 {
            self.set_sda(byte & 0x80 != 0);
            byte <<= 1;
            self.scl_high();
            self.scl_low();
        }

        self.sda_high();
        let pins = self.scl_high();

        self.scl_low();
        pins & I2C_SDA == 0
    }

    fn i2c_rx_byte(&mut self, send_nack: bool) -> u8 {
        let mut data = 0u8;

        self.sda_high(); // release SDA
        for _ in 0..8 {
            data <<= 1;
            let pins = self.scl_high();
            if pins & I2C_SDA != 0 {
                data |= 1;
            }

            self.scl_low();
        }

        // Send ACK/NACK
        if send_nack {
            self.sda_high();
        } else {
            self.sda_low()
        };
        self.scl_high();
        self.scl_low();
        self.sda_high(); // release

        data
    }

    pub fn i2c_write_bytes(&mut self, data: &[u8]) {
        for &b in data {
            self.i2c_tx(b);
        }
    }

    /// Write Device
    pub fn i2c_start_read(&mut self, addr: u8) -> bool {
        self.i2c_tx(addr << 1 | 0x01)
    }

    /// Write Device
    pub fn i2c_start_write(&mut self, addr: u8) -> bool {
        self.i2c_tx(addr << 1)
    }

    /// Read Device
    pub fn i2c_read_bytes(&mut self, len: usize) -> Vec<u8> {
        let mut received_bytes = vec![];
        for _ in 0..(len - 1) {
            received_bytes.push(self.i2c_rx_byte(false));
        }
        // Receive last byte with nak
        received_bytes.push(self.i2c_rx_byte(true));
        received_bytes
    }
}

impl I2c for I2cFtBitbang {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        let result = (|| {
            for op in operations {
                self.i2c_start();
                match op {
                    Operation::Read(rd) => {
                        let ack = self.i2c_start_read(address);
                        if !ack {
                            return Err(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address));
                        }
                        let resp = self.i2c_read_bytes(rd.len());
                        rd.copy_from_slice(&resp);
                    }
                    Operation::Write(wr) => {
                        let ack = self.i2c_start_write(address);
                        if !ack {
                            return Err(ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address));
                        }
                        self.i2c_write_bytes(wr);
                    }
                }
            }
            Ok(())
        })();

        // Always issue STOP, even on NACK — otherwise the master leaves SCL
        // held low mid-transaction, which wedges the target's I2C peripheral
        // (it never sees a STOP, so it keeps NACKing every future transaction).
        self.i2c_stop();

        result
    }
}

impl ErrorType for I2cFtBitbang {
    type Error = ErrorKind;
}
