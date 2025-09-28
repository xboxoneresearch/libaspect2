use embedded_hal::i2c::I2c;

pub const FLASH_SIZE: usize = 0x24400; // 145KB
const STATUS_PREFIX_SZ: usize = 2;

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

pub struct Isd9160<T>
{
    device: T,
    read_chunk_size: usize,
    position: u64,
}

impl<T> Isd9160<T>
where
    T: I2c
{
    /// Nuvoton ISD9160 Soundcorder Chip (RF Unit)
    pub const I2C_ADDR: u8 = 0x5A;

    pub fn new(device: T) -> Self {
        Self {
            device: device,
            read_chunk_size: 0x40,
            position: 0,
        }
    }

    pub fn flash_size(&self) -> usize { FLASH_SIZE }
    pub fn read_chunk_size(&self) -> usize { self.read_chunk_size }
    pub fn set_chunk_size(&mut self, value: usize)
    {
        self.read_chunk_size = value
    }

    pub fn read_interrupt(&mut self) -> u16 {
        let cmd: [u8; 1] = [Isd9160Commands::CMD_INTERRUPT_READ.into()];
        let mut read = [0u8; 2];
        self.device
            .write_read(Self::I2C_ADDR, &cmd, &mut read)
            .expect("Failed to read register");

        u16::from_le_bytes(read)
    }

    pub fn read_register<U: Into<u8>>(&mut self, register: U) -> u32 {
        let cmd = [Isd9160Commands::CMD_REG_READ.into(), register.into()];
        let mut read = [0u8; 4];
        self.device
            .write_read(Self::I2C_ADDR, &cmd, &mut read)
            .expect("Failed to read register");

        u32::from_le_bytes(read)
    }

    pub fn write_register<U: Into<u8>>(&mut self, register: U, data: &[u8]) {
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

    pub fn play_sound<U: Into<u8>>(&mut self, sound_index: U) {
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
    fn read_data(&mut self, addr: u32) -> Vec<u8> {
        let mut buf = vec![0u8; self.read_chunk_size + STATUS_PREFIX_SZ];

        let mut cmd = vec![Isd9160Commands::CMD_FLASH_READ.into()];
        let addr_bytes = addr.to_le_bytes();
        cmd.extend(&addr_bytes);

        self.device
            .write_read(Self::I2C_ADDR, &cmd, &mut buf)
            .expect("Failed to read data");

        buf[STATUS_PREFIX_SZ..].to_vec()
    }
}

impl<T> std::io::Seek for Isd9160<T>
where
    T: I2c
{
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        use std::io::SeekFrom;
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => {
                let end = FLASH_SIZE as i64;
                let np = end.checked_add(offset).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Seek out of bounds"))?;
                if np < 0 { return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Seek before start")); }
                np as u64
            }
            SeekFrom::Current(offset) => {
                let cur = self.position as i64;
                let np = cur.checked_add(offset).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Seek out of bounds"))?;
                if np < 0 { return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Seek before start")); }
                np as u64
            }
        };
        if new_pos > FLASH_SIZE as u64 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "Seek past end of flash"));
        }
        self.position = new_pos;
        Ok(self.position)
    }
}

impl<T> std::io::Read for Isd9160<T>
where
    T: I2c
{
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= FLASH_SIZE as u64 {
            return Ok(0);
        }
        let max_len = (FLASH_SIZE as u64 - self.position) as usize;
        let to_read = buf.len().min(max_len);
        let mut total_read = 0;
        while total_read < to_read {
            let addr = self.position as u32;
            let chunk = self.read_data(addr);
            let chunk_start = 0;
            let chunk_end = (to_read - total_read).min(self.read_chunk_size);
            buf[total_read..total_read+chunk_end].copy_from_slice(&chunk[chunk_start..chunk_end]);
            self.position += chunk_end as u64;
            total_read += chunk_end;
        }
        Ok(total_read)
    }
}
