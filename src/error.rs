use crate::prelude::*;
use thiserror::Error as DeriveError;
#[cfg(feature = "ftdi")]
use libftd2xx::{TimeoutError as FtdiTimeout, FtStatus, DeviceTypeError};

#[derive(DeriveError, Debug)]
pub enum Error {
    #[error("Not implemented")]
    Todo,
    
    #[cfg(feature = "ftdi")]
    #[error("FTDI Timeout")]
    DeviceTimeout(#[from] FtdiTimeout),
    
    #[cfg(feature = "ftdi")]
    #[error("FTDI Status: {0}")]
    FtStatus(#[from] FtStatus),
    
    #[cfg(feature = "ftdi")]
    #[error("FTDI Device Type Error: {0}")]
    DeviceTypeError(#[from] DeviceTypeError),
    
    #[error("SPI error")]
    SpiError,

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
