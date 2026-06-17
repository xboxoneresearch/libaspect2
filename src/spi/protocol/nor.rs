//! JEDEC standard SPI NOR flash protocol constants

/// JEDEC standard SPI NOR flash opcodes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NorOpcode {
    WriteEnable   = 0x06,
    WriteDisable  = 0x04,
    ReadStatus1   = 0x05,
    ReadStatus2   = 0x35,
    WriteStatus   = 0x01,
    PageProgram   = 0x02,
    Read          = 0x03,
    FastRead      = 0x0B,
    SectorErase4K = 0x20,
    BlockErase32K = 0x52,
    BlockErase64K = 0xD8,
    ChipErase     = 0xC7,
    ReadJedecId   = 0x9F,
    ReadSfdp      = 0x5A,
    EnableReset   = 0x66,
    Reset         = 0x99,
    DeepPowerDown = 0xB9,
    ReleaseDpd    = 0xAB,
}

/// Status register 1 bit masks
pub mod status {
    /// Device is executing an internal operation (erase/program); poll until clear
    pub const BUSY: u8 = 1 << 0;
    /// Write enable latch — set by WREN, cleared by WRDI or after program/erase
    pub const WEL: u8 = 1 << 1;
}

/// 3-byte JEDEC manufacturer / device ID returned by opcode 0x9F
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JedecId {
    pub manufacturer: u8,
    pub device: [u8; 2],
}

impl JedecId {
    /// Returns `false` when the bus is floating (all-0xFF) or the device did not respond (0x00).
    pub fn is_valid(&self) -> bool {
        self.manufacturer != 0xFF && self.manufacturer != 0x00
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jedec_id_valid() {
        assert!(JedecId { manufacturer: 0xEF, device: [0x40, 0x18] }.is_valid()); // Winbond W25Q128
        assert!(!JedecId { manufacturer: 0xFF, device: [0xFF, 0xFF] }.is_valid()); // floating bus
        assert!(!JedecId { manufacturer: 0x00, device: [0x00, 0x00] }.is_valid()); // no response
    }

    #[test]
    fn test_opcode_values() {
        assert_eq!(NorOpcode::Read as u8, 0x03);
        assert_eq!(NorOpcode::PageProgram as u8, 0x02);
        assert_eq!(NorOpcode::ReadJedecId as u8, 0x9F);
        assert_eq!(NorOpcode::SectorErase4K as u8, 0x20);
        assert_eq!(NorOpcode::ChipErase as u8, 0xC7);
    }
}
