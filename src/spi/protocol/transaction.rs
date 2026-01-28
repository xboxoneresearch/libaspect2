/// Transaction types and builders for eMMC SPI protocol

use super::commands::{Command, Register, DataSize};

/// Transaction type (hardware-independent representation)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionType {
    /// Write data to a register
    Write {
        register: Register,
        data: u32,
    },
    /// Read data from a register (4 bytes)
    Read {
        register: Register,
    },
    /// Read data from data fifo
    ReadData {
        register: Register,
    },
}

impl TransactionType {
    /// Create a write transaction
    pub fn write(register: Register, data: u32) -> Self {
        Self::Write { register, data }
    }
    
    /// Create a read transaction
    pub fn read(register: Register) -> Self {
        Self::Read { register }
    }
    
    /// Create a block read transaction
    pub fn read_data(register: Register) -> Self {
        Self::ReadData { register }
    }
    
    /// Get the command type for this transaction
    pub fn command(&self) -> Command {
        match self {
            Self::Write { .. } => Command::Write,
            Self::Read { .. } | Self::ReadData { .. } => Command::Read,
        }
    }
    
    /// Get the register address
    pub fn register(&self) -> Register {
        match self {
            Self::Write { register, .. } => *register,
            Self::Read { register } => *register,
            Self::ReadData { register } => *register,
        }
    }
    
    /// Get expected response size (None for write operations)
    pub fn response_size(&self) -> Option<DataSize> {
        match self {
            Self::Write { .. } => None,
            Self::Read { .. } => Some(DataSize::Register),
            Self::ReadData { .. } => Some(DataSize::Page),
        }
    }
    
    /// Get the data to write (None for read operations)
    pub fn write_data(&self) -> Option<u32> {
        match self {
            Self::Write { data, .. } => Some(*data),
            _ => None,
        }
    }
}

/// Transaction builder for fluent API
pub struct Transaction;

impl Transaction {
    /// Start building a write transaction
    pub fn write(register: Register, data: u32) -> TransactionType {
        TransactionType::write(register, data)
    }
    
    /// Start building a read transaction
    pub fn read(register: Register) -> TransactionType {
        TransactionType::read(register)
    }
    
    /// Start building a block read transaction
    pub fn read_data(register: Register) -> TransactionType {
        TransactionType::read_data(register)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_transaction() {
        let txn = Transaction::write(Register::Argument, 0x12345678);
        assert_eq!(txn.command(), Command::Write);
        assert_eq!(txn.register(), Register::Argument);
        assert_eq!(txn.write_data(), Some(0x12345678));
        assert_eq!(txn.response_size(), None);
    }

    #[test]
    fn test_read_transaction() {
        let txn = Transaction::read(Register::PresentState);
        assert_eq!(txn.command(), Command::Read);
        assert_eq!(txn.register(), Register::PresentState);
        assert_eq!(txn.write_data(), None);
        assert_eq!(txn.response_size(), Some(DataSize::Register));
    }

    #[test]
    fn test_read_data_transaction() {
        let txn = Transaction::read_data(Register::Argument);
        assert_eq!(txn.command(), Command::Read);
        assert_eq!(txn.register(), Register::Argument);
        assert_eq!(txn.write_data(), None);
        assert_eq!(txn.response_size(), Some(DataSize::Page));
    }
}
