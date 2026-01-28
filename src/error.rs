use thiserror::Error as DeriveError;
use libftd2xx::{TimeoutError as FtdiTimeout, FtStatus, DeviceTypeError};

#[derive(DeriveError, Debug)]
pub enum Error {
    #[error("Not implemented")]
    Todo,
    
    #[error("FTDI Timeout")]
    DeviceTimeout(#[from] FtdiTimeout),
    
    #[error("FTDI Status: {0}")]
    FtStatus(#[from] FtStatus),
    
    #[error("FTDI Device Type Error: {0}")]
    DeviceTypeError(#[from] DeviceTypeError),
    
    #[error("Invalid GPIO state")]
    InvalidGpioState,
    
    #[error("Invalid pin mask (must be single bit)")]
    InvalidPinMask,
    
    #[error("Sanity check failed: expected {expected:#X}, got {actual:#X}")]
    SanityCheckFailed { expected: u32, actual: u32 },
    
    #[error("Device initialization failed")]
    InitializationFailed,
    
    #[error("Register read/write failed")]
    RegisterAccessFailed,
    
    #[error("Operation timed out")]
    Timeout,
}
