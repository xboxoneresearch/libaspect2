/// Command and register definitions for eMMC SPI protocol

/// SPI Command type (2 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    /// Read operation (0b01)
    Read = 0x1,
    /// Write operation (0b10)
    Write = 0x2,
}

impl Command {
    /// Get the 2-bit command value
    pub fn bits(self) -> u8 {
        self as u8
    }
    
    /// Get number of bits for this command field (always 2)
    pub const fn bit_length() -> u8 {
        2
    }
}

/// eMMC SPI Controller Register addresses (8 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Register {
    /// Register 0x01
    Reg_01 = 0x01,

    /// Argument register - buffer/FIFO config
    Argument = 0x02,
    
    /// Command and Transfer Mode register
    CommandAndTransferMode = 0x03,
    
    /// Response/Status register 0 (also used for status polling)
    Response0And1 = 0x04,
    Response2And3 = 0x05,
    Response4And5 = 0x06,
    Response6And7 = 0x07,
    
    /// Data FIFO register (for 512-byte block reads)
    DataFifo = 0x08,
    
    /// Present State register
    PresentState = 0x09,

    /// Register 0x0A
    Reg_0A = 0x0A,
    
    /// Command register (for issuing commands to eMMC) - also known as StatusConfig
    Command = 0x0B,
    
    /// InterruptStatus
    InterruptStatus = 0x0C,
    
    /// Configuration register 1
    Config1 = 0x0D,
    
    /// Configuration register 2
    Config2 = 0x0E,

    /// Register 0x0F
    Reg_0F = 0x0F,
    
    /// Initialization command register
    InitCommand = 0x44,

    /// Register 0x88
    XipOutputDelay = 0x88,
}

impl Into<u8> for Register {
    fn into(self) -> u8 {
        self as u8
    }
}


impl Register {
    /// Get the 8-bit register address
    pub fn address(self) -> u8 {
        self as u8
    }
    
    /// Get number of bits for register address field (always 8)
    pub const fn bit_length() -> u8 {
        8
    }
    
    /// Create from raw address value
    pub fn from_address(addr: u8) -> Option<Self> {
        match addr {
            0x02 => Some(Self::Argument),
            0x03 => Some(Self::CommandAndTransferMode),
            0x04 => Some(Self::Response0And1),
            0x05 => Some(Self::Response2And3),
            0x06 => Some(Self::Response4And5),
            0x07 => Some(Self::Response6And7),
            0x08 => Some(Self::DataFifo),
            0x09 => Some(Self::PresentState),
            0x0B => Some(Self::Command),
            0x0C => Some(Self::InterruptStatus),
            0x0D => Some(Self::Config1),
            0x0E => Some(Self::Config2),
            0x44 => Some(Self::InitCommand),
            _ => None,
        }
    }
}

/// Data size for register operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataSize {
    /// Standard register size (4 bytes)
    Register = 4,
    /// Block size for eMMC data blocks (512 bytes)
    Page = 512,
}

impl DataSize {
    /// Get the size in bytes
    pub fn bytes(self) -> usize {
        self as usize
    }
}

/// eMMC State enum (from Present State register bits 9-12)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MmcState {
    Idle = 0,
    Ready = 1,
    Ident = 2,
    Standby = 3,
    Transfer = 4,
    Data = 5,
    Receive = 6,
    Program = 7,
    Disabled = 8,
    _BTDST = 9,
    Sleep = 10,
}

impl MmcState {
    /// Parse state from status bits
    pub fn from_bits(bits: u8) -> Option<Self> {
        match bits & 0x0F {
            0 => Some(Self::Idle),
            1 => Some(Self::Ready),
            2 => Some(Self::Ident),
            3 => Some(Self::Standby),
            4 => Some(Self::Transfer),
            5 => Some(Self::Data),
            6 => Some(Self::Receive),
            7 => Some(Self::Program),
            8 => Some(Self::Disabled),
            9 => Some(Self::_BTDST),
            10 => Some(Self::Sleep),
            _ => None,
        }
    }
}

/// SPI Error flags (from status registers)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorFlags(u32);

bitflags::bitflags! {
    impl ErrorFlags: u32 {
        const ERASE_RESET       = 1 << 0x0D;
        const ERROR             = 1 << 0x13;
        const CC_ERROR          = 1 << 0x14;
        const DEVICE_ECC_FAILED = 1 << 0x15;
        const ILLEGAL_COMMAND   = 1 << 0x16;
        const CRC_ERROR         = 1 << 0x17;
        const DEVICE_IS_LOCKED  = 1 << 0x19;
        const BLOCK_LENGTH_ERROR= 1 << 0x1D;
        const ADDRESS_MISALIGN  = 1 << 0x1E;
    }
}

impl ErrorFlags {
    /// Check if any error flag is set
    pub fn has_error(&self) -> bool {
        !self.is_empty()
    }
}

/// Status codes from protocol trace analysis
/// These are the actual values observed in the hardware protocol
pub mod status {
    /// Data ready status - indicates 512 bytes are ready to read from DataFifo
    pub const DATA_READY: u32 = 0x00000020;
    
    /// Command accepted status - indicates command was accepted and processing started
    pub const CMD_ACCEPTED: u32 = 0x00000021;
    
    /// Transfer complete status - indicates block transfer is finished
    pub const TRANSFER_COMPLETE: u32 = 0x00000002;
    
    /// Command/Busy status - written to initiate operations
    pub const CMD_BUSY: u32 = 0x00000001;
    
    /// Status clear/reset value - written to clear status after acknowledgement
    pub const STATUS_CLEAR: u32 = 0xFFFFFFFF;
}

/// Transfer configuration values from protocol trace
pub mod transfer_config {
    /// Standard transfer configuration for 512-byte page reads
    /// This value was observed in actual protocol traces
    pub const PAGE_READ: u32 = 0x113A0010;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_bits() {
        assert_eq!(Command::Read.bits(), 0x1);
        assert_eq!(Command::Write.bits(), 0x2);
        assert_eq!(Command::bit_length(), 2);
    }

    #[test]
    fn test_register_addresses() {
        assert_eq!(Register::Argument.address(), 0x02);
        assert_eq!(Register::InitCommand.address(), 0x44);
        assert_eq!(Register::bit_length(), 8);
    }

    #[test]
    fn test_register_from_address() {
        assert_eq!(Register::from_address(0x02), Some(Register::Argument));
        assert_eq!(Register::from_address(0x44), Some(Register::InitCommand));
        assert_eq!(Register::from_address(0xFF), None);
    }

    #[test]
    fn test_data_sizes() {
        assert_eq!(DataSize::Register.bytes(), 4);
        assert_eq!(DataSize::Page.bytes(), 512);
    }
}
