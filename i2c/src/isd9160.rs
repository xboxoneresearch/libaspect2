use embedded_hal::i2c::{ErrorKind, I2c};
use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use log::debug;

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug)]
pub enum Isd9160Commands {
    CMD_REG_WRITE = 0x48,
    CMD_REG_READ = 0xC1,
    CMD_INTERRUPT_READ = 0xC0,
    CMD_FLASH_READ = 0xC3,

    CMD_START = 0x81,
    CMD_STOP = 0x02,
    CMD_RESET = 0x4A,
}

impl Into<u8> for Isd9160Commands {
    fn into(self) -> u8 {
        self as u8
    }
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug)]
pub enum Isd9160Registers {
    /// R/W I2C Control Register
    REG_CTL = 0x00,
    /// R/W I2C Slave address Register0
    REG_ADDR0 = 0x04,
    /// R/W I2C DATA Register
    REG_DAT = 0x08,
    /// R I2C Status Register
    REG_STATUS = 0x0C,
    /// R/W I2C clock divided Register
    REG_CLKDIV = 0x10,
    /// R/W I2C Time out control Register
    REG_TOCTL = 0x14,
    /// R/W I2C Slave address Register1
    REG_ADDR1 = 0x18,
    /// R/W I2C Slave address Register2
    REG_ADDR2 = 0x1C,
    /// R/W I2C Slave address Register3
    REG_ADDR3 = 0x20,
    /// R/W I2C Slave address Mask Register0
    REG_ADDRMSK0 = 0x24,
    /// R/W I2C Slave address Mask Register1
    REG_ADDRMSK1 = 0x28,
    /// R/W I2C Slave address Mask Register2
    REG_ADDRMSK2 = 0x2C,
    /// R/W I2C Slave address Mask Register3
    REG_ADDRMSK3 = 0x30,
}

impl Into<u8> for Isd9160Registers {
    fn into(self) -> u8 {
        self as u8
    }
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug)]
pub enum Isd9160Sounds {
    POWERON = 0x00,
    BING = 0x01,
    POWEROFF = 0x02,

    DISC_DRIVE_1 = 0x03,
    DISC_DRIVE_2 = 0x04,
    DISC_DRIVE_3 = 0x05,

    PLOPP = 0x06,
    NO_DISC = 0x07,
    PLOPP_LOUDER = 0x08,
}

impl Into<u8> for Isd9160Sounds {
    fn into(self) -> u8 {
        self as u8
    }
}

pub struct Isd9160I2c {
    device: Box<dyn I2c<Error = ErrorKind> + 'static>,
}

impl Isd9160I2c {
    /// Nuvoton ISD9160 Soundcorder Chip (RF Unit)
    pub const I2C_ADDR: u8 = 0x5A;
    pub const FLASH_SIZE: u32 = 0x24400; // 145KB

    pub fn new(device: impl I2c<Error = ErrorKind> + 'static) -> Self {
        Self {
            device: Box::new(device),
        }
    }

    pub fn read_interrupt(&mut self) -> u16 {
        let cmd: [u8; 1] = [Isd9160Commands::CMD_INTERRUPT_READ.into()];
        let mut read = [0u8; 2];
        self.device
            .write_read(Self::I2C_ADDR, &cmd, &mut read)
            .expect("Failed to read register");

        u16::from_le_bytes(read)
    }

    pub fn read_register<T: Into<u8>>(&mut self, register: T) -> u32 {
        let cmd = [Isd9160Commands::CMD_REG_READ.into(), register.into()];
        let mut read = [0u8; 4];
        self.device
            .write_read(Self::I2C_ADDR, &cmd, &mut read)
            .expect("Failed to read register");

        u32::from_le_bytes(read)
    }

    pub fn write_register<T: Into<u8>>(&mut self, register: T, data: &[u8]) {
        let mut cmd = vec![Isd9160Commands::CMD_REG_WRITE.into(), register.into()];
        cmd.extend_from_slice(&data);
        self.device
            .write(Self::I2C_ADDR, &cmd)
            .expect("Failed to write register");
    }

    pub fn init(&mut self) {
        self.write_register(Isd9160Registers::REG_STATUS, &[0x01]);
        self.write_register(Isd9160Registers::REG_ADDR0, &[0xFF, 0xFF]);
    }

    pub fn reset(&mut self) {
        self.device
            .write(Self::I2C_ADDR, &[Isd9160Commands::CMD_RESET.into(), 0x55])
            .expect("Failed to reset");
    }

    pub fn play_sound<T: Into<u8>>(&mut self, sound_index: T) {
        self.device
            .write(
                Self::I2C_ADDR,
                &[Isd9160Commands::CMD_START.into(), sound_index.into()],
            )
            .expect("Failed to play sound");
    }

    pub fn stop(&mut self) {
        self.device
            .write(Self::I2C_ADDR, &[Isd9160Commands::CMD_STOP.into()])
            .expect("Failed to stop");
    }

    /// This reads 6 bytes at a time
    pub fn read_data(&mut self, addr: u32) -> [u8; 6] {
        let mut buf = vec![0u8; 8];

        let mut cmd = vec![Isd9160Commands::CMD_FLASH_READ.into()];
        let addr_bytes = addr.to_le_bytes();
        cmd.extend(&addr_bytes);

        self.device
            .write_read(Self::I2C_ADDR, &cmd, &mut buf)
            .expect("Failed to read data");

        buf[2..8].try_into().unwrap()
    }

    pub fn read_flash(&mut self, writer: Box<&mut dyn std::io::Write>) {
        for addr in (0..Self::FLASH_SIZE)
            .step_by(6)
            .progress()
            .with_style(ProgressStyle::default_spinner().template("[{elapsed_precise}, eta:{eta}] {bar:40.cyan/blue} {bytes} / {total_bytes}").unwrap())
        {
            let ret = self.read_data(addr);
            writer.write(&ret).expect("Failed to write read data");
        }
    }
}
