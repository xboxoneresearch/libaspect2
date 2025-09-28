use std::time::Duration;

use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, Operation};
use libftd2xx::{BitMode, Ft4232h, FtdiCommon};

const BITMODE: libftd2xx::BitMode = BitMode::SyncBitbang;

const I2C_SCL: u8 = 1 << 6; // CDBUS6
const I2C_SDA: u8 = 1 << 7; // CDBUS7
const I2C_MASK: u8 = I2C_SCL | I2C_SDA;

pub struct I2cFtBitbang {
    device: Ft4232h,
    gpio_val: u8,
    gpio_dir: u8,
}

impl I2cFtBitbang {
    pub fn new(device: Ft4232h) -> Self {
        Self {
            device,
            gpio_val: I2C_MASK, // Both high
            gpio_dir: 0, // Both as input (high, open-drain)
        }
    }
}

impl I2cFtBitbang {
    fn gpio_write(&mut self, values: u8, direction: u8) {
        self.device.set_bit_mode(direction, BITMODE).unwrap();
        self.device.write(&[values]).unwrap();
    }

    fn gpio_read(&mut self) -> u8 {
        let bits = self.device.bit_mode().unwrap();
        bits
    }

    fn delay_ns(&self, ns: u64) {
        std::thread::sleep(Duration::from_nanos(ns));
    }

    /* Drive SDA high (release = input) */
    fn sda_high(&mut self) {
        self.gpio_val |= I2C_SDA;
        self.gpio_dir &= !I2C_SDA;  // input
        self.gpio_write(self.gpio_val, self.gpio_dir);
    }

    /* Drive SDA low */
    fn sda_low(&mut self) {
        self.gpio_val &= !I2C_SDA;
        self.gpio_dir |= I2C_SDA;   // output
        self.gpio_write(self.gpio_val, self.gpio_dir);
    }

    /* Set SCL high */
    fn scl_high(&mut self) {
        self.gpio_val |= I2C_SCL;
        self.gpio_dir &= !I2C_SCL;   // input
        self.gpio_write(self.gpio_val, self.gpio_dir);
    }

    /* Set SCL low */
    fn scl_low(&mut self) {
        self.gpio_val &= !I2C_SCL;
        self.gpio_dir |= I2C_SCL;   // output
        self.gpio_write(self.gpio_val, self.gpio_dir);
    }

    fn i2c_start(&mut self) {
        //let mut dst = vec![];
        // SDA descending while SCL is HIGH.
        self.sda_high(); self.scl_high(); self.delay_ns(800);
        self.sda_low(); self.delay_ns(800);
        self.scl_low(); self.delay_ns(800);
    }

    fn i2c_stop(&mut self) {
        // SDA rasing while SCL is HIGH.
        self.sda_low(); self.delay_ns(800);
        self.scl_high(); self.delay_ns(800);
        self.sda_high(); self.delay_ns(800);
    }

    fn i2c_tx(&mut self, byte: u8) -> bool {
        let mut byte = byte;
        for _ in 0..8 {
            if byte & 0x80 != 0 { self.sda_high(); } else { self.sda_low() };
            byte <<= 1;
            self.delay_ns(400);
            self.scl_high(); self.delay_ns(800);
            self.scl_low(); self.delay_ns(400);
        }

        // Release SDA for ACK
        self.sda_high(); self.delay_ns(400);
        self.scl_high(); self.delay_ns(800);

        // Sample SDA
        let pins = self.gpio_read();

        self.scl_low(); self.delay_ns(400);
        pins & I2C_SDA == 0
    }

    fn i2c_rx_byte(&mut self, send_nack: bool) -> u8 {
        let mut data = 0u8;

        self.sda_high(); // release SDA
        for _ in 0..8 {
            data <<= 1;
            self.scl_high(); self.delay_ns(800);

            let pins = self.gpio_read();
            if pins & I2C_SDA != 0
            {
                data |= 1;
            }

            self.scl_low(); self.delay_ns(800);
        }

        // Send ACK/NACK
        if send_nack { self.sda_high(); } else { self.sda_low() };
        self.delay_ns(400);
        self.scl_high(); self.delay_ns(800);
        self.scl_low(); self.delay_ns(400);
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
        //self.i2c_start();
        for op in operations {
            self.i2c_start();
            match op {
                Operation::Read(rd) => {
                    let ack = self.i2c_start_read(address);
                    if !ack {
                        println!("Read: NACK");
                    }
                    let resp = self
                        .i2c_read_bytes(rd.len());
                    //println!("{resp:?}");
                    rd.copy_from_slice(&resp);
                }
                Operation::Write(wr) => {
                    let ack = self.i2c_start_write(address);
                    if !ack {
                        println!("Write: NACK");
                    }
                    self.i2c_write_bytes(&wr);
                }
            }
        }
        self.i2c_stop();

        Ok(())
    }
}

impl ErrorType for I2cFtBitbang {
    type Error = ErrorKind;
}
