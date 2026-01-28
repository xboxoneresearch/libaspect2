/// Protocol module - hardware-independent eMMC SPI protocol implementation
/// 
/// This module defines the protocol structures and operations without
/// depending on any specific hardware backend (FTDI, embedded-hal, etc.)

pub mod commands;
pub mod transaction;