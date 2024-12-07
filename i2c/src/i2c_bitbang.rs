use std::time::Duration;

use embedded_hal::i2c::{ErrorKind, ErrorType, I2c};
use libftd2xx::{BitMode, Ft4232h, FtdiCommon};
use log::{debug, trace};

const BITMODE: libftd2xx::BitMode = BitMode::SyncBitbang;

const I2C_ADDR_START: u8 = 0x03;
const I2C_ADDR_STOP: u8 = 0x77;

const I2C_START_SERIAL_SIZE: usize = 4;
const I2C_STOP_SERIAL_SIZE: usize = 3;
const I2C_SEND_SERIAL_SIZE: usize = 24 + 3;
const I2C_RECV_SERIAL_SIZE: usize = 24 + 3;

pub struct I2cCommand {
    buf: Vec<u8>,
    sda_mask: u8,
    scl_mask: u8,
}

impl I2cCommand {
    pub fn builder(sda_mask: u8, scl_mask: u8) -> Self {
        Self {
            buf: vec![],
            sda_mask,
            scl_mask,
        }
    }

    /// SCL bitmask
    #[allow(non_snake_case)]
    fn SCL_MASK(&self) -> u8 {
        self.scl_mask
    }

    /// SDA bitmask
    #[allow(non_snake_case)]
    fn SDA_MASK(&self) -> u8 {
        self.sda_mask
    }

    pub fn finish(&self) -> Vec<u8> {
        self.buf.clone()
    }

    fn i2c_start(mut self) -> Self {
        let mut dst = vec![];
        // SDA descending while SCL is HIGH.
        dst.push(self.SDA_MASK());
        dst.push(self.SCL_MASK() | self.SDA_MASK());
        dst.push(self.SCL_MASK());
        dst.push(0x00);

        assert_eq!(dst.len(), I2C_START_SERIAL_SIZE);

        self.buf.extend(&dst);
        self
    }

    fn i2c_stop(mut self) -> Self {
        let mut dst = vec![];
        // SDA rasing while SCL is HIGH.
        dst.push(0x00);
        dst.push(self.SCL_MASK());
        dst.push(self.SCL_MASK() | self.SDA_MASK());

        assert_eq!(dst.len(), I2C_STOP_SERIAL_SIZE);

        self.buf.extend(&dst);
        self
    }

    fn i2c_tx(mut self, byte: u8) -> Self {
        let mut bit: u8;
        let mut dat = byte;
        let mut dst = vec![];

        for _ in 0..8 {
            let sda_state = {
                // Set read/write bit
                if dat & 0x80 != 0 {
                    self.SDA_MASK()
                } else {
                    0
                }
            };
            bit = sda_state;
            dst.push(bit);
            dst.push(bit | self.SCL_MASK());
            dst.push(bit);
            dat <<= 1;
        }

        // Wait for ack
        dst.push(self.SDA_MASK());
        dst.push(self.SDA_MASK() | self.SCL_MASK());
        dst.push(self.SDA_MASK());

        assert_eq!(dst.len(), I2C_RECV_SERIAL_SIZE);

        self.buf.extend(dst);
        self
    }

    fn i2c_rx(mut self, ack: bool) -> Self {
        let mut dst = vec![];

        for _ in 0..8 {
            dst.push(self.SDA_MASK());
            dst.push(self.SDA_MASK() | self.SCL_MASK());
            dst.push(self.SDA_MASK());
        }

        if ack {
            dst.push(0x00);
            dst.push(self.SCL_MASK());
            dst.push(0x00);
        } else {
            dst.push(self.SDA_MASK());
            dst.push(self.SDA_MASK() | self.SCL_MASK());
            dst.push(self.SDA_MASK());
        }

        assert_eq!(dst.len(), I2C_RECV_SERIAL_SIZE);

        self.buf.extend(dst);
        self
    }

    pub fn i2c_tx_slice(mut self, data: &[u8]) -> Self {
        for &b in data {
            self = self.i2c_tx(b)
        }
        self
    }

    /// Write Device
    pub fn i2c_write(self, addr: u8) -> Self {
        self.i2c_start().i2c_tx(addr << 1)
    }

    /// Read Device
    pub fn i2c_read(self, addr: u8, len: usize, stop: bool) -> Self {
        let mut cmd_builder = self.i2c_start().i2c_tx((addr << 1) | 0x01);

        if stop {
            for _ in 0..(len - 1) {
                cmd_builder = cmd_builder.i2c_rx(true);
            }
            // Receive last byte with nak
            cmd_builder = cmd_builder.i2c_rx(false).i2c_stop();
        } else {
            for _ in 0..len {
                cmd_builder = cmd_builder.i2c_rx(true);
            }
        }

        cmd_builder
    }
}

pub struct I2cFtBitbang {
    device: Ft4232h,
    scl_pin: u8,
    sda_pin: u8,
}

impl I2cFtBitbang {
    pub fn new(mut device: Ft4232h, scl_pin: u8, sda_pin: u8) -> Self {
        // Set all pins to bitbang mode
        device.set_bit_mode(0b_1100_0000, BITMODE).unwrap();

        Self {
            device,
            scl_pin,
            sda_pin,
        }
    }
}

impl I2cFtBitbang {
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

    fn i2c_decode(&self, src: &[u8], len: usize) -> Vec<u8> {
        let mut dst = vec![];
        let start_offset = I2C_START_SERIAL_SIZE + I2C_SEND_SERIAL_SIZE;
        for i in 0..len {
            let mut v: u8 = 0x00;
            let curr_offset = start_offset + I2C_RECV_SERIAL_SIZE * i;
            for j in 0..8 {
                v <<= 1;
                if ((src[curr_offset + j * 3 + 1] & self.SDA_MASK())) != 0 {
                    v |= 1;
                }
            }
            dst.push(v);
        }

        dst
    }

    fn cmd_builder(&self) -> I2cCommand {
        I2cCommand::builder(self.SDA_MASK(), self.SCL_MASK())
    }

    fn write(&mut self, data: &[u8]) -> Vec<u8> {
        let mut resp = vec![0u8; data.len()];
        for &b in data {
            self.device.write(&[b]).unwrap();
        }
        loop {
            if self.device.queue_status().unwrap() == data.len() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.device.read(&mut resp).unwrap();

        resp
    }
}

impl I2c for I2cFtBitbang {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        for op in operations {
            match op {
                embedded_hal::i2c::Operation::Read(rd) => {
                    let cmd = self.cmd_builder()
                        .i2c_read(address, rd.len(), false)
                        .finish();

                    let resp = self.write(&cmd);
                    let decoded = self.i2c_decode(&resp, rd.len());

                    rd.copy_from_slice(&decoded);
                    std::thread::sleep(Duration::from_millis(10));
                }
                embedded_hal::i2c::Operation::Write(wr) => {
                    let cmd = self.cmd_builder()
                        .i2c_write(address)
                        .i2c_tx_slice(&wr)
                        .i2c_stop()
                        .finish();

                    self.write(&cmd);
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }

        Ok(())
    }
}

impl ErrorType for I2cFtBitbang {
    type Error = ErrorKind;
}
